//! Process and Thread Management
//! 
//! Windows NT-like Executive layer with processes and threads

pub mod thread;
pub mod scheduler;
pub mod syscall;

use spin::Mutex;
use crate::sync::IrqMutex;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

extern crate alloc;

/// Process ID type
pub type Pid = u32;
pub type Tid = u32;

/// Process states
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProcessState {
    Created,
    Ready,
    Running,
    Waiting,
    Terminated,
}

impl ProcessState {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProcessState::Created => "CREATED",
            ProcessState::Ready => "READY",
            ProcessState::Running => "RUNNING",
            ProcessState::Waiting => "WAITING",
            ProcessState::Terminated => "STOPPED",
        }
    }
}

/// Process Control Block (EPROCESS-like)
pub struct Process {
    pub pid: Pid,
    pub name: [u8; 32],
    pub state: ProcessState,
    pub threads: Vec<Tid>,
    pub page_directory: usize,
    pub handle_table: BTreeMap<u32, Handle>,
}

impl Process {
    pub fn name_str(&self) -> &str {
        let len = self.name.iter().position(|&b| b == 0).unwrap_or(32);
        core::str::from_utf8(&self.name[..len]).unwrap_or("???")
    }
}

/// Thread Control Block (ETHREAD-like)
pub struct Thread {
    pub tid: Tid,
    pub pid: Pid,
    pub state: ProcessState,
    pub priority: u8,
    pub context: ThreadContext,
    pub kernel_stack: usize,
    pub user_stack: usize,
}

/// Thread execution context
#[repr(C)]
pub struct ThreadContext {
    pub regs: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub pstate: u64,
}

impl ThreadContext {
    pub const fn new() -> Self {
        ThreadContext {
            regs: [0; 31],
            sp: 0,
            pc: 0,
            pstate: 0,
        }
    }
}

/// Kernel handle
pub struct Handle {
    pub object_type: ObjectType,
    pub access: u32,
    pub object_ptr: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum ObjectType {
    Process,
    Thread,
    File,
    Event,
    Mutex,
    Section,
}

static PROCESS_TABLE: IrqMutex<BTreeMap<Pid, Process>> = IrqMutex::new(BTreeMap::new());
pub static THREAD_TABLE: IrqMutex<BTreeMap<Tid, Thread>> = IrqMutex::new(BTreeMap::new());
static NEXT_PID: Mutex<Pid> = Mutex::new(1);
pub(crate) static NEXT_TID: Mutex<Tid> = Mutex::new(1);

/// Boot thread TID (the kernel_main / shell thread)
pub const BOOT_TID: Tid = 0;

/// Initialize process manager
pub fn init() {
    // Create System process (PID 0)
    create_system_process();
}

fn create_system_process() {
    let mut process = Process {
        pid: 0,
        name: [0; 32],
        state: ProcessState::Running,
        threads: Vec::new(),
        page_directory: crate::mm::virtual_mem::kernel_pgd_phys(),
        handle_table: BTreeMap::new(),
    };
    
    let name = b"System";
    process.name[..name.len()].copy_from_slice(name);
    
    // Create the boot thread (TID 0) representing kernel_main.
    // Its context will be filled in by the scheduler on first preemption.
    let boot_thread = Thread {
        tid: BOOT_TID,
        pid: 0,
        state: ProcessState::Running,
        priority: scheduler::PRIORITY_NORMAL,
        context: ThreadContext::new(),
        kernel_stack: 0, // uses the boot stack from boot.rs
        user_stack: 0,
    };
    THREAD_TABLE.lock().insert(BOOT_TID, boot_thread);
    process.threads.push(BOOT_TID);
    
    PROCESS_TABLE.lock().insert(0, process);
}

/// Create a new process
pub fn create_process(name: &str) -> Option<Pid> {
    let mut next_pid = NEXT_PID.lock();
    let pid = *next_pid;
    *next_pid += 1;
    
    let mut process = Process {
        pid,
        name: [0; 32],
        state: ProcessState::Created,
        threads: Vec::new(),
        page_directory: crate::mm::virtual_mem::clone_kernel_tables(),
        handle_table: BTreeMap::new(),
    };
    
    let name_bytes = name.as_bytes();
    let len = core::cmp::min(name_bytes.len(), 31);
    process.name[..len].copy_from_slice(&name_bytes[..len]);
    
    PROCESS_TABLE.lock().insert(pid, process);
    Some(pid)
}

/// Create a new thread
pub fn create_thread(pid: Pid, entry_point: usize) -> Option<Tid> {
    let mut next_tid = NEXT_TID.lock();
    let tid = *next_tid;
    *next_tid += 1;
    
    // Allocate kernel stack
    let stack_top = thread::alloc_kernel_stack()?;
    
    // Create thread with proper context
    let mut thread = Thread {
        tid,
        pid,
        state: ProcessState::Ready,
        priority: 8, // Normal priority
        context: ThreadContext::for_entry(entry_point, stack_top),
        kernel_stack: stack_top,
        user_stack: 0,
    };
    
    // Initialize x30 (link register) to entry point for first context switch
    thread.context.regs[30] = entry_point as u64;
    
    THREAD_TABLE.lock().insert(tid, thread);
    
    if let Some(process) = PROCESS_TABLE.lock().get_mut(&pid) {
        process.threads.push(tid);
    }
    
    Some(tid)
}

/// Terminate a process and clean up its resources
pub fn terminate_process(pid: Pid, _exit_code: i32) {
    let thread_ids: Vec<Tid>;
    
    // Mark process as terminated and collect thread IDs
    {
        let mut procs = PROCESS_TABLE.lock();
        if let Some(process) = procs.get_mut(&pid) {
            process.state = ProcessState::Terminated;
            thread_ids = process.threads.clone();
        } else {
            return;
        }
    }
    
    // Clean up each thread
    {
        let mut threads = THREAD_TABLE.lock();
        for &tid in &thread_ids {
            if let Some(thread) = threads.remove(&tid) {
                // Free the kernel stack (if it's not the boot stack)
                if thread.kernel_stack != 0 && tid != BOOT_TID {
                    let stack_base = thread.kernel_stack - thread::THREAD_STACK_SIZE;
                    let pages = thread::THREAD_STACK_SIZE / crate::arch::aarch64::mmu::PAGE_SIZE;
                    for i in 0..pages {
                        crate::mm::physical::free_frame(stack_base + i * crate::arch::aarch64::mmu::PAGE_SIZE);
                    }
                }
            }
        }
    }

    // Free per-process page tables (PGD, PUD, PMD) and user-mapped pages.
    // PID 0 (System) uses the kernel PGD and must never be freed.
    if pid != 0 {
        let pgd = get_process_pgd(pid);
        if pgd != 0 {
            crate::mm::virtual_mem::free_process_page_tables(pgd);
        }
    }
}

/// Process info snapshot for listing
pub struct ProcessInfo {
    pub pid: Pid,
    pub name: String,
    pub state: ProcessState,
    pub threads: usize,
    pub priority: u8,
}

/// List all processes
pub fn list_processes() -> Vec<ProcessInfo> {
    let procs = PROCESS_TABLE.lock();
    let threads = THREAD_TABLE.lock();
    
    let mut list = Vec::new();
    for (_, proc) in procs.iter() {
        // Find highest priority thread in this process
        let max_pri = proc.threads.iter()
            .filter_map(|tid| threads.get(tid))
            .map(|t| t.priority)
            .max()
            .unwrap_or(0);
        
        list.push(ProcessInfo {
            pid: proc.pid,
            name: String::from(proc.name_str()),
            state: proc.state,
            threads: proc.threads.len(),
            priority: max_pri,
        });
    }
    list
}

/// Get total number of threads
pub fn thread_count() -> usize {
    THREAD_TABLE.lock().len()
}

/// Get total number of processes
pub fn process_count() -> usize {
    PROCESS_TABLE.lock().len()
}

/// Spawn a kernel task (creates a process + thread that runs a Rust function)
pub fn spawn_kernel_task(name: &str, entry: fn() -> !, priority: u8) -> Option<Pid> {
    let pid = create_process(name)?;
    
    let mut next_tid = NEXT_TID.lock();
    let tid = *next_tid;
    *next_tid += 1;
    
    // Allocate kernel stack
    let stack_top = thread::alloc_kernel_stack()?;
    
    // Create thread with proper context for kernel entry
    let mut thread = Thread {
        tid,
        pid,
        state: ProcessState::Ready,
        priority,
        context: ThreadContext::for_entry(entry as usize, stack_top),
        kernel_stack: stack_top,
        user_stack: 0,
    };
    
    // Set x30 (link register) to entry point - context_switch returns via ret
    thread.context.regs[30] = entry as usize as u64;
    
    THREAD_TABLE.lock().insert(tid, thread);
    
    if let Some(process) = PROCESS_TABLE.lock().get_mut(&pid) {
        process.threads.push(tid);
        process.state = ProcessState::Ready;
    }
    
    // Add to scheduler
    scheduler::ready(tid);
    
    Some(pid)
}

/// Spawn a user-mode process from raw ELF data
/// Creates the process (with per-process page tables), loads the ELF into
/// the process's address space, sets up a user stack, and schedules the thread.
pub fn spawn_user_process(name: &str, elf_data: &[u8], priority: u8) -> Option<Pid> {
    let pid = create_process(name)?;
    let pgd = get_process_pgd(pid);
    
    // Load ELF into the process's own page tables
    let user_program = match crate::fs::elf::load_elf_for_user(elf_data, pgd) {
        Ok(prog) => prog,
        Err(_) => return None,
    };
    
    let mut next_tid = NEXT_TID.lock();
    let tid = *next_tid;
    *next_tid += 1;
    
    // Allocate kernel stack (needed for syscall handling)
    let kernel_stack = thread::alloc_kernel_stack()?;
    
    // Create thread with user-mode context (trampoline-based for first scheduling)
    let thread = Thread {
        tid,
        pid,
        state: ProcessState::Ready,
        priority,
        context: ThreadContext::for_user_entry(user_program.entry_point, user_program.stack_top, kernel_stack),
        kernel_stack,
        user_stack: user_program.stack_top,
    };
    
    THREAD_TABLE.lock().insert(tid, thread);
    
    if let Some(process) = PROCESS_TABLE.lock().get_mut(&pid) {
        process.threads.push(tid);
        process.state = ProcessState::Ready;
    }
    
    // Add to scheduler
    scheduler::ready(tid);
    
    Some(pid)
}

/// Get the page directory (PGD physical address) for a given process
pub fn get_process_pgd(pid: Pid) -> usize {
    PROCESS_TABLE.lock()
        .get(&pid)
        .map(|p| p.page_directory)
        .unwrap_or(0)
}

/// Attach an already-created thread to a process and mark both as Ready
pub fn attach_thread_to_process(pid: Pid, tid: Tid) {
    if let Some(process) = PROCESS_TABLE.lock().get_mut(&pid) {
        process.threads.push(tid);
        process.state = ProcessState::Ready;
    }
}
