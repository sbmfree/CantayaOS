//! System Call Interface
//!
//! Windows NT-like syscall interface.
//! System calls use SVC with arguments in x0-x5, syscall number in x8,
//! and return value in x0.

use super::{Pid, Tid};
use crate::arch::aarch64::exceptions::ExceptionContext;

/// Maximum user-space buffer size we'll accept (1MB)
const MAX_USER_BUF: usize = 1024 * 1024;

/// Validate a user-mode pointer range.
/// Returns true if the buffer appears to be in a valid user-accessible range.
#[inline]
fn validate_user_ptr(ptr: u64, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    if len > MAX_USER_BUF {
        return false;
    }
    let start = ptr as usize;
    let end = start.wrapping_add(len);
    // User pointers must be below the kernel region and must not wrap
    end > start && end < 0xFFFF_0000_0000_0000
}

/// Safely read a user-space string. Returns None if pointer is invalid.
fn read_user_str(ptr: u64, len: u64) -> Option<&'static str> {
    let len = len as usize;
    if !validate_user_ptr(ptr, len) {
        return None;
    }
    unsafe {
        let slice = core::slice::from_raw_parts(ptr as *const u8, len);
        core::str::from_utf8(slice).ok()
    }
}

/// System call numbers (NT-like naming)
pub const SYS_NT_CREATE_PROCESS: u64 = 0x0001;
pub const SYS_NT_CREATE_THREAD: u64 = 0x0002;
pub const SYS_NT_TERMINATE_PROCESS: u64 = 0x0003;
pub const SYS_NT_TERMINATE_THREAD: u64 = 0x0004;
pub const SYS_NT_GET_PROCESS_ID: u64 = 0x0005;
pub const SYS_NT_GET_THREAD_ID: u64 = 0x0006;
pub const SYS_NT_YIELD: u64 = 0x0007;
pub const SYS_NT_SLEEP: u64 = 0x0008;

pub const SYS_NT_ALLOCATE_VIRTUAL_MEMORY: u64 = 0x0010;
pub const SYS_NT_FREE_VIRTUAL_MEMORY: u64 = 0x0011;
pub const SYS_NT_READ_VIRTUAL_MEMORY: u64 = 0x0012;
pub const SYS_NT_WRITE_VIRTUAL_MEMORY: u64 = 0x0013;
pub const SYS_NT_QUERY_VIRTUAL_MEMORY: u64 = 0x0014;

pub const SYS_NT_CREATE_FILE: u64 = 0x0020;
pub const SYS_NT_READ_FILE: u64 = 0x0021;
pub const SYS_NT_WRITE_FILE: u64 = 0x0022;
pub const SYS_NT_CLOSE: u64 = 0x0023;
pub const SYS_NT_CREATE_DIRECTORY: u64 = 0x0024;
pub const SYS_NT_QUERY_DIRECTORY: u64 = 0x0025;
pub const SYS_NT_DELETE_FILE: u64 = 0x0026;

pub const SYS_NT_CREATE_EVENT: u64 = 0x0030;
pub const SYS_NT_SET_EVENT: u64 = 0x0031;
pub const SYS_NT_RESET_EVENT: u64 = 0x0032;
pub const SYS_NT_CREATE_MUTEX: u64 = 0x0033;
pub const SYS_NT_RELEASE_MUTEX: u64 = 0x0034;
pub const SYS_NT_CREATE_SEMAPHORE: u64 = 0x0035;
pub const SYS_NT_RELEASE_SEMAPHORE: u64 = 0x0036;
pub const SYS_NT_CREATE_PIPE: u64 = 0x0037;

pub const SYS_NT_WAIT_FOR_SINGLE_OBJECT: u64 = 0x0040;
pub const SYS_NT_WAIT_FOR_MULTIPLE_OBJECTS: u64 = 0x0041;

pub const SYS_NT_DEBUG_PRINT: u64 = 0x00FF;

/// Status codes (NTSTATUS-like)
#[repr(u64)]
#[derive(Clone, Copy, Debug)]
pub enum NtStatus {
    Success = 0,
    Pending = 0x0000_0103,
    InvalidParameter = 0xC000_000D,
    NoMemory = 0xC000_0017,
    AccessDenied = 0xC000_0022,
    InvalidHandle = 0xC000_0008,
    NotImplemented = 0xC000_0002,
    ObjectNotFound = 0xC000_0034,
    EndOfFile = 0xC000_0011,
    BufferTooSmall = 0xC000_0023,
}

/// Handle syscall from exception handler (old interface — kept for compat)
pub fn handle_syscall() {
    // Legacy path — should not be used with new exception handling
}

/// Handle syscall with full exception context
pub fn handle_syscall_ctx(ctx: &mut ExceptionContext) {
    let syscall_num = ctx.regs[8];  // x8 = syscall number
    let a0 = ctx.regs[0];           // x0
    let a1 = ctx.regs[1];           // x1
    let a2 = ctx.regs[2];           // x2
    let _a3 = ctx.regs[3];          // x3
    let _a4 = ctx.regs[4];          // x4

    // Debug: trace first syscall from user mode
    if syscall_num == SYS_NT_DEBUG_PRINT {
        crate::kprintln!("[syscall] DEBUG_PRINT from ELR={:#x} ptr={:#x} len={}", ctx.pc, a0, a1);
    }

    let result = dispatch_syscall(syscall_num, a0, a1, a2, _a3, _a4);

    // Return result in x0
    ctx.regs[0] = result;

    // Note: On AArch64, ELR_EL1 for SVC already points to the instruction
    // AFTER the SVC (preferred return address = SVC + 4), so we do NOT
    // advance ctx.pc here.
}

fn dispatch_syscall(num: u64, a0: u64, a1: u64, a2: u64, _a3: u64, _a4: u64) -> u64 {
    match num {
        // Process management
        SYS_NT_CREATE_PROCESS => sys_create_process(a0, a1),
        SYS_NT_CREATE_THREAD => sys_create_thread(a0 as u32, a1 as usize),
        SYS_NT_TERMINATE_PROCESS => sys_terminate_process(a0 as u32, a1 as i32),
        SYS_NT_TERMINATE_THREAD => sys_terminate_thread(a0 as u32),
        SYS_NT_GET_PROCESS_ID => sys_get_process_id(),
        SYS_NT_GET_THREAD_ID => sys_get_thread_id(),
        SYS_NT_YIELD => sys_yield(),
        SYS_NT_SLEEP => sys_sleep(a0),

        // Memory management
        SYS_NT_ALLOCATE_VIRTUAL_MEMORY => sys_allocate_memory(a0, a1),
        SYS_NT_FREE_VIRTUAL_MEMORY => sys_free_memory(a0, a1),

        // File I/O
        SYS_NT_CREATE_FILE => sys_create_file(a0, a1),
        SYS_NT_READ_FILE => sys_read_file(a0 as u32, a1, a2 as usize),
        SYS_NT_WRITE_FILE => sys_write_file(a0 as u32, a1, a2 as usize),
        SYS_NT_CLOSE => sys_close(a0 as u32),
        SYS_NT_CREATE_DIRECTORY => sys_mkdir(a0, a1),
        SYS_NT_QUERY_DIRECTORY => sys_query_directory(a0, a1, a2, _a3 as usize),
        SYS_NT_DELETE_FILE => sys_delete_file(a0, a1),

        // IPC
        SYS_NT_CREATE_EVENT => sys_create_event(a0 != 0),
        SYS_NT_SET_EVENT => sys_set_event(a0 as u32),
        SYS_NT_RESET_EVENT => sys_reset_event(a0 as u32),
        SYS_NT_CREATE_MUTEX => sys_create_mutex(),
        SYS_NT_RELEASE_MUTEX => sys_release_mutex(a0 as u32),
        SYS_NT_CREATE_SEMAPHORE => sys_create_semaphore(a0 as u32),
        SYS_NT_RELEASE_SEMAPHORE => sys_release_semaphore(a0 as u32),
        SYS_NT_CREATE_PIPE => sys_create_pipe(),

        // Wait
        SYS_NT_WAIT_FOR_SINGLE_OBJECT => sys_wait_for_single_object(a0 as u32, a1),
        SYS_NT_WAIT_FOR_MULTIPLE_OBJECTS => {
            NtStatus::NotImplemented as u64 // TODO: implement
        }

        // Debug
        SYS_NT_DEBUG_PRINT => sys_debug_print(a0, a1 as usize),

        _ => {
            crate::kprintln!("Unknown syscall: {:#x}", num);
            NtStatus::NotImplemented as u64
        }
    }
}

// ---------------------------------------------------------------------------
// Process syscalls
// ---------------------------------------------------------------------------

fn sys_create_process(name_ptr: u64, name_len: u64) -> u64 {
    let name = match read_user_str(name_ptr, name_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };

    match super::create_process(name) {
        Some(pid) => pid as u64,
        None => NtStatus::NoMemory as u64,
    }
}

fn sys_create_thread(pid: Pid, entry: usize) -> u64 {
    match super::create_thread(pid, entry) {
        Some(tid) => {
            crate::process::scheduler::ready(tid);
            tid as u64
        }
        None => NtStatus::NoMemory as u64,
    }
}

fn sys_terminate_process(pid: Pid, exit_code: i32) -> u64 {
    super::terminate_process(pid, exit_code);
    NtStatus::Success as u64
}

fn sys_terminate_thread(_tid: Tid) -> u64 {
    // TODO: implement thread termination
    NtStatus::Success as u64
}

fn sys_get_process_id() -> u64 {
    // Return PID of current thread's process
    if let Some(tid) = crate::process::scheduler::current_tid() {
        if let Some(thread) = super::THREAD_TABLE.lock().get(&tid) {
            return thread.pid as u64;
        }
    }
    0
}

fn sys_get_thread_id() -> u64 {
    crate::process::scheduler::current_tid().unwrap_or(0) as u64
}

fn sys_yield() -> u64 {
    crate::process::scheduler::yield_thread();
    NtStatus::Success as u64
}

fn sys_sleep(ms: u64) -> u64 {
    if ms == 0 {
        crate::process::scheduler::yield_thread();
    } else {
        crate::process::scheduler::sleep_current_ms(ms);
    }
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// Memory syscalls
// ---------------------------------------------------------------------------

fn sys_allocate_memory(addr: u64, size: u64) -> u64 {
    match crate::mm::virtual_mem::allocate_virtual_memory(addr as usize, size as usize) {
        Some(base) => base as u64,
        None => NtStatus::NoMemory as u64,
    }
}

fn sys_free_memory(addr: u64, size: u64) -> u64 {
    crate::mm::virtual_mem::free_virtual_memory(addr as usize, size as usize);
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// File I/O syscalls
// ---------------------------------------------------------------------------

fn sys_create_file(path_ptr: u64, path_len: u64) -> u64 {
    let path = match read_user_str(path_ptr, path_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };

    match crate::fs::create(path) {
        Ok(handle) => handle.id as u64,
        Err(_) => {
            // Try open instead
            match crate::fs::open(path, crate::fs::AccessMode::ReadWrite) {
                Ok(handle) => handle.id as u64,
                Err(_) => NtStatus::ObjectNotFound as u64,
            }
        }
    }
}

fn sys_read_file(handle_id: u32, buf_ptr: u64, buf_len: usize) -> u64 {
    if !validate_user_ptr(buf_ptr, buf_len) {
        return NtStatus::InvalidParameter as u64;
    }
    
    // Handle 0 = stdin (console)
    if handle_id == 0 {
        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
        let n = crate::hal::console::read_line(buf);
        return n as u64;
    }

    // For other handles, try to use the filesystem via handle
    // Simplified: handle_id is used as a file handle index
    // In a complete NT implementation, the process handle table maps IDs to kernel objects
    NtStatus::NotImplemented as u64
}

fn sys_write_file(handle_id: u32, buf_ptr: u64, buf_len: usize) -> u64 {
    if !validate_user_ptr(buf_ptr, buf_len) {
        return NtStatus::InvalidParameter as u64;
    }
    
    // Handle 1 = stdout (console)
    if handle_id == 1 {
        let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, buf_len) };
        if let Ok(s) = core::str::from_utf8(buf) {
            crate::kprint!("{}", s);
        } else {
            for &b in buf {
                crate::kprint!("{}", b as char);
            }
        }
        return buf_len as u64;
    }

    NtStatus::NotImplemented as u64
}

fn sys_close(_handle_id: u32) -> u64 {
    NtStatus::Success as u64
}

fn sys_mkdir(path_ptr: u64, path_len: u64) -> u64 {
    let path = match read_user_str(path_ptr, path_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };

    match crate::fs::mkdir(path) {
        Ok(()) => NtStatus::Success as u64,
        Err(_) => NtStatus::ObjectNotFound as u64,
    }
}

fn sys_query_directory(path_ptr: u64, path_len: u64, buf_ptr: u64, buf_len: usize) -> u64 {
    let path = match read_user_str(path_ptr, path_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };

    if !validate_user_ptr(buf_ptr, buf_len) {
        return NtStatus::InvalidParameter as u64;
    }

    match crate::fs::readdir(path) {
        Ok(entries) => {
            // Serialize entries as newline-separated names into user buffer
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
            let mut pos = 0;
            for entry in &entries {
                let name = entry.name.as_bytes();
                if pos + name.len() + 1 > buf_len {
                    break;
                }
                buf[pos..pos + name.len()].copy_from_slice(name);
                pos += name.len();
                buf[pos] = b'\n';
                pos += 1;
            }
            pos as u64
        }
        Err(_) => NtStatus::ObjectNotFound as u64,
    }
}

fn sys_delete_file(path_ptr: u64, path_len: u64) -> u64 {
    let path = match read_user_str(path_ptr, path_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };

    match crate::fs::delete(path) {
        Ok(()) => NtStatus::Success as u64,
        Err(_) => NtStatus::ObjectNotFound as u64,
    }
}

// ---------------------------------------------------------------------------
// IPC syscalls
// ---------------------------------------------------------------------------

fn sys_create_event(auto_reset: bool) -> u64 {
    crate::ipc::event::create_event(auto_reset) as u64
}

fn sys_set_event(id: u32) -> u64 {
    if crate::ipc::event::set_event(id) {
        NtStatus::Success as u64
    } else {
        NtStatus::InvalidHandle as u64
    }
}

fn sys_reset_event(id: u32) -> u64 {
    if crate::ipc::event::reset_event(id) {
        NtStatus::Success as u64
    } else {
        NtStatus::InvalidHandle as u64
    }
}

fn sys_create_mutex() -> u64 {
    crate::ipc::mutex::create_mutex() as u64
}

fn sys_release_mutex(id: u32) -> u64 {
    if let Some(tid) = crate::process::scheduler::current_tid() {
        if crate::ipc::mutex::release_mutex(id, tid) {
            NtStatus::Success as u64
        } else {
            NtStatus::InvalidHandle as u64
        }
    } else {
        NtStatus::InvalidHandle as u64
    }
}

fn sys_create_semaphore(max_count: u32) -> u64 {
    crate::ipc::semaphore::create_semaphore(max_count) as u64
}

fn sys_release_semaphore(id: u32) -> u64 {
    if crate::ipc::semaphore::release_semaphore(id) {
        NtStatus::Success as u64
    } else {
        NtStatus::InvalidHandle as u64
    }
}

fn sys_create_pipe() -> u64 {
    crate::ipc::pipe::create_pipe() as u64
}

// ---------------------------------------------------------------------------
// Wait syscalls
// ---------------------------------------------------------------------------

/// Object types for WaitForSingleObject
#[allow(dead_code)]
const WAIT_OBJECT_EVENT: u32 = 0x0030;
#[allow(dead_code)]
const WAIT_OBJECT_MUTEX: u32 = 0x0033;
#[allow(dead_code)]
const WAIT_OBJECT_SEMAPHORE: u32 = 0x0035;

/// WaitForSingleObject — blocks the calling thread until the object is signaled.
/// a0 = object handle (ID), a1 = timeout_ms (0 = infinite)
fn sys_wait_for_single_object(handle: u32, _timeout_ms: u64) -> u64 {
    let tid = match crate::process::scheduler::current_tid() {
        Some(t) => t,
        None => return NtStatus::InvalidHandle as u64,
    };

    // Try to determine the object type by probing the IPC subsystems.
    // In a full NT implementation this would use typed handles.
    // We try event first, then mutex, then semaphore.
    
    // Try as event
    if crate::ipc::event::wait_event(handle, tid) {
        return NtStatus::Success as u64;
    }
    // wait_event returned false → thread was added to waiter list
    crate::process::scheduler::block_current();
    NtStatus::Success as u64
}

// ---------------------------------------------------------------------------
// Debug syscalls
// ---------------------------------------------------------------------------

fn sys_debug_print(ptr: u64, len: usize) -> u64 {
    let s = match read_user_str(ptr, len as u64) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };
    crate::kprint!("{}", s);
    NtStatus::Success as u64
}
