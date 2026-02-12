//! System Call Interface
//!
//! Windows NT-like syscall interface.
//! System calls use architecture-specific conventions:
//! - AArch64: SVC with arguments in x0-x5, syscall number in x8, return value in x0.
//! - x86_64: INT 0x80 with syscall number in rax, arguments in rdi, rsi, rdx, r10, return value in rax.

use super::{Pid, Tid};
use crate::arch::exceptions::ExceptionContext;

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
pub const SYS_NT_WAIT_FOR_PROCESS: u64 = 0x0009;

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

// GUI syscalls
pub const SYS_GUI_CREATE_WINDOW: u64 = 0x0060;
pub const SYS_GUI_DESTROY_WINDOW: u64 = 0x0061;
pub const SYS_GUI_SET_CONTENT: u64 = 0x0062;
pub const SYS_GUI_POLL_EVENT: u64 = 0x0063;
pub const SYS_GUI_SET_PIXEL_BUFFER: u64 = 0x0064;
pub const SYS_GUI_GET_WINDOW_SIZE: u64  = 0x0065;

// Network / socket syscalls
pub const SYS_NET_SOCKET: u64     = 0x0050;
pub const SYS_NET_CONNECT: u64    = 0x0051;
pub const SYS_NET_SEND: u64       = 0x0052;
pub const SYS_NET_RECV: u64       = 0x0053;
pub const SYS_NET_CLOSE: u64      = 0x0054;
pub const SYS_NET_BIND: u64       = 0x0055;
pub const SYS_NET_LISTEN: u64     = 0x0056;
pub const SYS_NET_ACCEPT: u64     = 0x0057;

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
    // Extract syscall number and arguments based on architecture
    #[cfg(target_arch = "aarch64")]
    let (syscall_num, a0, a1, a2, a3, a4) = {
        // AArch64: x8 = syscall number, x0-x4 = arguments
        (ctx.regs[8], ctx.regs[0], ctx.regs[1], ctx.regs[2], ctx.regs[3], ctx.regs[4])
    };
    
    #[cfg(target_arch = "x86_64")]
    let (syscall_num, a0, a1, a2, a3, a4) = {
        // x86_64: rax = syscall number, rdi, rsi, rdx, r10 = arguments
        (ctx.rax, ctx.rdi, ctx.rsi, ctx.rdx, ctx.r10, 0)
    };

    let result = dispatch_syscall(syscall_num, a0, a1, a2, a3, a4);

    // Return result based on architecture
    #[cfg(target_arch = "aarch64")]
    {
        ctx.regs[0] = result;  // x0
        // Note: On AArch64, ELR_EL1 for SVC already points to the instruction
        // AFTER the SVC (preferred return address = SVC + 4), so we do NOT
        // advance ctx.pc here.
    }
    
    #[cfg(target_arch = "x86_64")]
    {
        ctx.rax = result;  // rax
        // Note: On x86_64, RIP for INT already points to the instruction
        // AFTER the INT, so we do NOT advance ctx.rip here.
    }
}

fn dispatch_syscall(num: u64, a0: u64, a1: u64, a2: u64, a3: u64, _a4: u64) -> u64 {
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
        SYS_NT_WAIT_FOR_PROCESS => sys_wait_for_process(a0 as u32),

        // Memory management
        SYS_NT_ALLOCATE_VIRTUAL_MEMORY => sys_allocate_memory(a0, a1),
        SYS_NT_FREE_VIRTUAL_MEMORY => sys_free_memory(a0, a1),

        // File I/O
        SYS_NT_CREATE_FILE => sys_create_file(a0, a1),
        SYS_NT_READ_FILE => sys_read_file(a0 as u32, a1, a2 as usize),
        SYS_NT_WRITE_FILE => sys_write_file(a0 as u32, a1, a2 as usize),
        SYS_NT_CLOSE => sys_close(a0 as u32),
        SYS_NT_CREATE_DIRECTORY => sys_mkdir(a0, a1),
        SYS_NT_QUERY_DIRECTORY => sys_query_directory(a0, a1, a2, a3 as usize),
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

        // GUI
        SYS_GUI_CREATE_WINDOW => sys_gui_create_window(a0, a1, a2),
        SYS_GUI_DESTROY_WINDOW => sys_gui_destroy_window(a0 as u32),
        SYS_GUI_SET_CONTENT => sys_gui_set_content(a0 as u32, a1, a2),
        SYS_GUI_POLL_EVENT => sys_gui_poll_event(a0 as u32, a1),
        SYS_GUI_SET_PIXEL_BUFFER => sys_gui_set_pixel_buffer(a0 as u32, a1, a2 as u32, a3 as u32),
        SYS_GUI_GET_WINDOW_SIZE => sys_gui_get_window_size(a0 as u32, a1),

        // Networking / sockets
        SYS_NET_SOCKET => sys_net_socket(),
        SYS_NET_CONNECT => sys_net_connect(a0 as u32, a1, a2 as u16),
        SYS_NET_SEND => sys_net_send(a0 as u32, a1, a2 as usize),
        SYS_NET_RECV => sys_net_recv(a0 as u32, a1, a2 as usize),
        SYS_NET_CLOSE => sys_net_close(a0 as u32),
        SYS_NET_BIND => sys_net_bind(a0 as u32, a1 as u16),
        SYS_NET_LISTEN => sys_net_listen(a0 as u32),
        SYS_NET_ACCEPT => sys_net_accept(a0 as u32),

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

    // Try open, then create
    let file_handle = match crate::fs::open(path, crate::fs::AccessMode::ReadWrite) {
        Ok(h) => h,
        Err(_) => match crate::fs::create(path) {
            Ok(h) => h,
            Err(_) => return NtStatus::ObjectNotFound as u64,
        },
    };

    // Store in current process's FD table
    let pid = match super::current_pid() {
        Some(p) => p,
        None => return NtStatus::InvalidHandle as u64,
    };
    match super::process_alloc_fd(pid, super::FdKind::File(file_handle)) {
        Some(fd) => fd as u64,
        None => NtStatus::NoMemory as u64,
    }
}

fn sys_read_file(handle_id: u32, buf_ptr: u64, buf_len: usize) -> u64 {
    if !validate_user_ptr(buf_ptr, buf_len) {
        return NtStatus::InvalidParameter as u64;
    }

    let pid = match super::current_pid() {
        Some(p) => p,
        None => return NtStatus::InvalidHandle as u64,
    };

    // Check if this is a Console FD (stdin) or File FD
    {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(process) = procs.get(&pid) {
            if let Some(desc) = process.fd_table.get(&handle_id) {
                match &desc.kind {
                    super::FdKind::Console => {
                        drop(procs);
                        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
                        let n = crate::hal::console::read_line(buf);
                        return n as u64;
                    }
                    super::FdKind::TcpSocket(key) => {
                        let key = *key;
                        drop(procs);
                        let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
                        let n = crate::net::tcp::recv_blocking(&key, buf, 10_000);
                        return n as u64;
                    }
                    super::FdKind::File(_) => { /* handled below with mutable borrow */ }
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    }

    // File FD — need mutable borrow to update FileHandle.position
    let mut procs = super::PROCESS_TABLE.lock();
    if let Some(process) = procs.get_mut(&pid) {
        if let Some(desc) = process.fd_table.get_mut(&handle_id) {
            if let super::FdKind::File(ref mut fh) = desc.kind {
                let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, buf_len) };
                match crate::fs::read(fh, buf) {
                    Ok(n) => return n as u64,
                    Err(_) => return NtStatus::EndOfFile as u64,
                }
            }
        }
    }
    NtStatus::InvalidHandle as u64
}

fn sys_write_file(handle_id: u32, buf_ptr: u64, buf_len: usize) -> u64 {
    if !validate_user_ptr(buf_ptr, buf_len) {
        return NtStatus::InvalidParameter as u64;
    }

    let pid = match super::current_pid() {
        Some(p) => p,
        None => return NtStatus::InvalidHandle as u64,
    };

    // Check FD type
    {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(process) = procs.get(&pid) {
            if let Some(desc) = process.fd_table.get(&handle_id) {
                match &desc.kind {
                    super::FdKind::Console => {
                        drop(procs);
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
                    super::FdKind::TcpSocket(key) => {
                        let key = *key;
                        drop(procs);
                        let data = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, buf_len) };
                        if crate::net::tcp::send(&key, data) {
                            return buf_len as u64;
                        } else {
                            return NtStatus::AccessDenied as u64;
                        }
                    }
                    super::FdKind::File(_) => { /* handled below */ }
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    }

    // File FD
    let mut procs = super::PROCESS_TABLE.lock();
    if let Some(process) = procs.get_mut(&pid) {
        if let Some(desc) = process.fd_table.get_mut(&handle_id) {
            if let super::FdKind::File(ref mut fh) = desc.kind {
                let buf = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, buf_len) };
                match crate::fs::write(fh, buf) {
                    Ok(n) => return n as u64,
                    Err(_) => return NtStatus::AccessDenied as u64,
                }
            }
        }
    }
    NtStatus::InvalidHandle as u64
}

fn sys_close(handle_id: u32) -> u64 {
    let pid = match super::current_pid() {
        Some(p) => p,
        None => return NtStatus::InvalidHandle as u64,
    };

    // Don't allow closing stdin/stdout/stderr
    if handle_id <= 2 {
        return NtStatus::InvalidParameter as u64;
    }

    match super::process_remove_fd(pid, handle_id) {
        Some(desc) => {
            match desc.kind {
                super::FdKind::File(handle) => crate::fs::close(handle),
                super::FdKind::TcpSocket(key) => crate::net::tcp::close(&key),
                super::FdKind::Console => {}
            }
            NtStatus::Success as u64
        }
        None => NtStatus::InvalidHandle as u64,
    }
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

// ---------------------------------------------------------------------------
// Process wait syscall (Feature 3 — waitpid)
// ---------------------------------------------------------------------------

fn sys_wait_for_process(pid: u32) -> u64 {
    let exit_code = super::waitpid(pid);
    exit_code as u64
}

// ---------------------------------------------------------------------------
// GUI syscalls (Feature 6)
// ---------------------------------------------------------------------------

/// Create a window: a0 = title_ptr, a1 = title_len,
/// a2 encodes x(12)|y(12)|w(12)|h(12) packed into 48 bits (top bits).
/// For simplicity: a2 = (w << 16 | h), x=100, y=100 default.
fn sys_gui_create_window(title_ptr: u64, title_len: u64, dimensions: u64) -> u64 {
    let title = match read_user_str(title_ptr, title_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };
    let w = ((dimensions >> 16) & 0xFFFF) as u32;
    let h = (dimensions & 0xFFFF) as u32;
    let w = if w == 0 { 400 } else { w };
    let h = if h == 0 { 300 } else { h };

    let pid = super::current_pid().unwrap_or(0);
    let id = crate::gui::window::WM.lock().create_window_owned(title, 100, 80, w, h, pid);
    id as u64
}

/// Destroy a window.
fn sys_gui_destroy_window(win_id: u32) -> u64 {
    crate::gui::window::WM.lock().close_window(win_id);
    NtStatus::Success as u64
}

/// Set window content text.
fn sys_gui_set_content(win_id: u32, text_ptr: u64, text_len: u64) -> u64 {
    let text = match read_user_str(text_ptr, text_len) {
        Some(s) => s,
        None => return NtStatus::InvalidParameter as u64,
    };
    crate::gui::window::WM.lock().set_content(win_id, text);
    NtStatus::Success as u64
}

/// Poll a window event. Writes event data to user buffer.
/// a0 = window_id, a1 = out_buf_ptr (must be at least 16 bytes).
/// Returns: 0 = event written, 1 = no event pending.
fn sys_gui_poll_event(win_id: u32, out_ptr: u64) -> u64 {
    if !validate_user_ptr(out_ptr, 16) {
        return NtStatus::InvalidParameter as u64;
    }
    let evt = crate::gui::window::WM.lock().poll_event(win_id);
    match evt {
        Some(ev) => {
            // Write event to user buffer as [type: u32, x_or_code: i32, y: i32, button: u32]
            let buf = out_ptr as *mut u32;
            unsafe {
                match ev {
                    crate::gui::event::GuiEvent::MouseMove { x, y } => {
                        *buf = 1;
                        *buf.add(1) = x as u32;
                        *buf.add(2) = y as u32;
                        *buf.add(3) = 0;
                    }
                    crate::gui::event::GuiEvent::MouseDown { x, y, button } => {
                        *buf = 2;
                        *buf.add(1) = x as u32;
                        *buf.add(2) = y as u32;
                        *buf.add(3) = button as u32;
                    }
                    crate::gui::event::GuiEvent::MouseUp { x, y, button } => {
                        *buf = 3;
                        *buf.add(1) = x as u32;
                        *buf.add(2) = y as u32;
                        *buf.add(3) = button as u32;
                    }
                    crate::gui::event::GuiEvent::KeyDown { code } => {
                        *buf = 4;
                        *buf.add(1) = code as u32;
                        *buf.add(2) = 0;
                        *buf.add(3) = 0;
                    }
                    crate::gui::event::GuiEvent::KeyUp { code } => {
                        *buf = 5;
                        *buf.add(1) = code as u32;
                        *buf.add(2) = 0;
                        *buf.add(3) = 0;
                    }
                }
            }
            0 // event was written
        }
        None => 1, // no event pending
    }
}

fn sys_gui_set_pixel_buffer(win_id: u32, buf_ptr: u64, width: u32, height: u32) -> u64 {
    let len = (width * height) as usize;
    if len == 0 || !validate_user_ptr(buf_ptr, len * 4) {
        return NtStatus::InvalidParameter as u64;
    }
    let pixels = unsafe { core::slice::from_raw_parts(buf_ptr as *const u32, len) };
    let pixel_vec = alloc::vec::Vec::from(pixels);
    crate::gui::window::WM.lock().set_pixel_buffer(win_id, pixel_vec, width, height);
    NtStatus::Success as u64
}

fn sys_gui_get_window_size(win_id: u32, out_ptr: u64) -> u64 {
    if !validate_user_ptr(out_ptr, 8) {
        return NtStatus::InvalidParameter as u64;
    }
    let wm = crate::gui::window::WM.lock();
    if let Some(win) = wm.windows.iter().find(|w| w.id == win_id) {
        unsafe {
            let ptr = out_ptr as *mut u32;
            *ptr = win.width;
            *ptr.add(1) = win.height;
        }
        NtStatus::Success as u64
    } else {
        NtStatus::InvalidHandle as u64
    }
}
// ---------------------------------------------------------------------------
// Network / socket syscalls
// ---------------------------------------------------------------------------

fn sys_net_socket() -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let key = tcp::socket();
    match super::process_alloc_fd(pid, super::FdKind::TcpSocket(key)) {
        Some(fd) => fd as u64,
        None => NtStatus::NoMemory as u64,
    }
}

fn sys_net_connect(fd: u32, ip_ptr: u64, port: u16) -> u64 {
    use crate::net::tcp;
    let ip_bytes: [u8; 4] = unsafe {
        let p = ip_ptr as *const u8;
        [*p, *p.add(1), *p.add(2), *p.add(3)]
    };

    match tcp::connect(ip_bytes, port) {
        Some(key) => {
            // Update the FD to the established key
            let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
            let mut procs = super::PROCESS_TABLE.lock();
            if let Some(proc) = procs.get_mut(&pid) {
                if let Some(desc) = proc.fd_table.get_mut(&fd) {
                    desc.kind = super::FdKind::TcpSocket(key);
                }
            }
            NtStatus::Success as u64
        }
        None => NtStatus::InvalidParameter as u64,
    }
}

fn sys_net_send(fd: u32, buf_ptr: u64, len: usize) -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let key = {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get(&pid) {
            if let Some(desc) = proc.fd_table.get(&fd) {
                if let super::FdKind::TcpSocket(ref k) = desc.kind {
                    *k
                } else {
                    return NtStatus::InvalidHandle as u64;
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    };

    let data = unsafe { core::slice::from_raw_parts(buf_ptr as *const u8, len) };
    if tcp::send(&key, data) {
        len as u64
    } else {
        NtStatus::InvalidParameter as u64
    }
}

fn sys_net_recv(fd: u32, buf_ptr: u64, len: usize) -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let key = {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get(&pid) {
            if let Some(desc) = proc.fd_table.get(&fd) {
                if let super::FdKind::TcpSocket(ref k) = desc.kind {
                    *k
                } else {
                    return NtStatus::InvalidHandle as u64;
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    };

    let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr as *mut u8, len) };
    let n = tcp::recv_blocking(&key, buf, 10_000);
    n as u64
}

fn sys_net_close(fd: u32) -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let key = {
        let mut procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get_mut(&pid) {
            if let Some(desc) = proc.fd_table.remove(&fd) {
                if let super::FdKind::TcpSocket(k) = desc.kind {
                    k
                } else {
                    return NtStatus::InvalidHandle as u64;
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    };
    tcp::close(&key);
    NtStatus::Success as u64
}

fn sys_net_bind(_fd: u32, _port: u16) -> u64 {
    // Bind is implicit — the listen call creates the listener
    NtStatus::Success as u64
}

fn sys_net_listen(fd: u32) -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let port = {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get(&pid) {
            if let Some(desc) = proc.fd_table.get(&fd) {
                if let super::FdKind::TcpSocket(ref k) = desc.kind {
                    k.local_port
                } else {
                    return NtStatus::InvalidHandle as u64;
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    };

    let key = tcp::listen(port);
    {
        let mut procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get_mut(&pid) {
            if let Some(desc) = proc.fd_table.get_mut(&fd) {
                desc.kind = super::FdKind::TcpSocket(key);
            }
        }
    }
    NtStatus::Success as u64
}

fn sys_net_accept(fd: u32) -> u64 {
    use crate::net::tcp;
    let pid = match crate::process::current_pid() { Some(p) => p, None => return NtStatus::InvalidHandle as u64 };
    let listen_key = {
        let procs = super::PROCESS_TABLE.lock();
        if let Some(proc) = procs.get(&pid) {
            if let Some(desc) = proc.fd_table.get(&fd) {
                if let super::FdKind::TcpSocket(ref k) = desc.kind {
                    *k
                } else {
                    return NtStatus::InvalidHandle as u64;
                }
            } else {
                return NtStatus::InvalidHandle as u64;
            }
        } else {
            return NtStatus::InvalidHandle as u64;
        }
    };

    match tcp::accept(&listen_key) {
        Some(child_key) => {
            match super::process_alloc_fd(pid, super::FdKind::TcpSocket(child_key)) {
                Some(new_fd) => new_fd as u64,
                None => NtStatus::NoMemory as u64,
            }
        }
        None => NtStatus::InvalidParameter as u64,
    }
}
