//! CantayaOS Userspace Syscall Library
//!
//! Provides safe Rust wrappers around CantayaOS system calls
//! for use by user-space applications.

#![no_std]

use core::arch::asm;
use core::fmt::{self, Write};

// -------------------------------------------------------------------------
// Raw syscall interface
// -------------------------------------------------------------------------

/// Perform a raw system call.
/// Convention for AArch64: syscall number in x8, arguments in x0-x4, return in x0.
/// Convention for x86_64: syscall number in rax, arguments in rdi, rsi, rdx, r10, r8, r9, return in rax.
#[inline(always)]
pub unsafe fn syscall0(num: u64) -> u64 {
    let ret: u64;
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") num,
        lateout("x0") ret,
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall1(num: u64, a0: u64) -> u64 {
    let ret: u64;
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") a0,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(num: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") a0,
        in("rsi") a1,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(num: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let ret: u64;
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall4(num: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ret: u64;
    #[cfg(target_arch = "aarch64")]
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        options(nostack)
    );
    #[cfg(target_arch = "x86_64")]
    asm!(
        "int 0x80",
        inlateout("rax") num => ret,
        in("rdi") a0,
        in("rsi") a1,
        in("rdx") a2,
        in("r10") a3,
        options(nostack)
    );
    ret
}

// -------------------------------------------------------------------------
// Syscall numbers
// -------------------------------------------------------------------------

pub const SYS_NT_CREATE_PROCESS: u64      = 0x0001;
pub const SYS_NT_CREATE_THREAD: u64       = 0x0002;
pub const SYS_NT_TERMINATE_PROCESS: u64   = 0x0003;
pub const SYS_NT_TERMINATE_THREAD: u64    = 0x0004;
pub const SYS_NT_GET_PROCESS_ID: u64      = 0x0005;
pub const SYS_NT_GET_THREAD_ID: u64       = 0x0006;
pub const SYS_NT_YIELD: u64              = 0x0007;
pub const SYS_NT_SLEEP: u64             = 0x0008;
pub const SYS_NT_WAIT_FOR_PROCESS: u64  = 0x0009;

pub const SYS_NT_ALLOCATE_VIRTUAL_MEMORY: u64 = 0x0010;
pub const SYS_NT_FREE_VIRTUAL_MEMORY: u64     = 0x0011;

pub const SYS_NT_CREATE_FILE: u64        = 0x0020;
pub const SYS_NT_READ_FILE: u64          = 0x0021;
pub const SYS_NT_WRITE_FILE: u64         = 0x0022;
pub const SYS_NT_CLOSE: u64             = 0x0023;
pub const SYS_NT_CREATE_DIRECTORY: u64   = 0x0024;
pub const SYS_NT_QUERY_DIRECTORY: u64    = 0x0025;
pub const SYS_NT_DELETE_FILE: u64        = 0x0026;

pub const SYS_NT_CREATE_EVENT: u64       = 0x0030;
pub const SYS_NT_SET_EVENT: u64          = 0x0031;
pub const SYS_NT_RESET_EVENT: u64        = 0x0032;
pub const SYS_NT_CREATE_MUTEX: u64       = 0x0033;
pub const SYS_NT_RELEASE_MUTEX: u64      = 0x0034;
pub const SYS_NT_CREATE_SEMAPHORE: u64   = 0x0035;
pub const SYS_NT_RELEASE_SEMAPHORE: u64  = 0x0036;
pub const SYS_NT_CREATE_PIPE: u64        = 0x0037;

pub const SYS_NT_WAIT_FOR_SINGLE_OBJECT: u64    = 0x0040;
pub const SYS_NT_WAIT_FOR_MULTIPLE_OBJECTS: u64  = 0x0041;

pub const SYS_GUI_CREATE_WINDOW: u64    = 0x0060;
pub const SYS_GUI_DESTROY_WINDOW: u64   = 0x0061;
pub const SYS_GUI_SET_CONTENT: u64      = 0x0062;
pub const SYS_GUI_POLL_EVENT: u64       = 0x0063;

// Network / socket syscalls
pub const SYS_NET_SOCKET: u64           = 0x0050;
pub const SYS_NET_CONNECT: u64          = 0x0051;
pub const SYS_NET_SEND: u64             = 0x0052;
pub const SYS_NET_RECV: u64             = 0x0053;
pub const SYS_NET_CLOSE: u64            = 0x0054;
pub const SYS_NET_BIND: u64             = 0x0055;
pub const SYS_NET_LISTEN: u64           = 0x0056;
pub const SYS_NET_ACCEPT: u64           = 0x0057;

// GUI pixel buffer syscalls
pub const SYS_GUI_SET_PIXEL_BUFFER: u64 = 0x0064;
pub const SYS_GUI_GET_WINDOW_SIZE: u64  = 0x0065;

pub const SYS_NT_DEBUG_PRINT: u64        = 0x00FF;

// -------------------------------------------------------------------------
// Safe wrappers — process / thread
// -------------------------------------------------------------------------

/// Print a string to the kernel debug console.
pub fn debug_print(s: &str) {
    unsafe { syscall2(SYS_NT_DEBUG_PRINT, s.as_ptr() as u64, s.len() as u64); }
}

/// Yield the current time slice.
pub fn yield_thread() {
    unsafe { syscall0(SYS_NT_YIELD); }
}

/// Sleep for the given number of milliseconds.
pub fn sleep(ms: u64) {
    unsafe { syscall1(SYS_NT_SLEEP, ms); }
}

/// Get current process ID.
pub fn get_pid() -> u64 {
    unsafe { syscall0(SYS_NT_GET_PROCESS_ID) }
}

/// Get current thread ID.
pub fn get_tid() -> u64 {
    unsafe { syscall0(SYS_NT_GET_THREAD_ID) }
}

/// Terminate the current process with an exit code.
pub fn exit(code: i32) -> ! {
    let pid = get_pid();
    unsafe { syscall2(SYS_NT_TERMINATE_PROCESS, pid, code as u64); }
    // Should never return, but just in case:
    loop { unsafe { asm!("wfi"); } }
}

// -------------------------------------------------------------------------
// Safe wrappers — file I/O
// -------------------------------------------------------------------------

/// Write bytes to a file handle (1 = stdout).
pub fn write(handle: u32, buf: &[u8]) -> u64 {
    unsafe { syscall3(SYS_NT_WRITE_FILE, handle as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

/// Read bytes from a file handle (0 = stdin).
pub fn read(handle: u32, buf: &mut [u8]) -> u64 {
    unsafe { syscall3(SYS_NT_READ_FILE, handle as u64, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

/// Open/create a file by path. Returns a handle ID (0 on failure).
pub fn open(path: &str) -> u64 {
    unsafe { syscall2(SYS_NT_CREATE_FILE, path.as_ptr() as u64, path.len() as u64) }
}

/// Close a file handle.
pub fn close(handle: u32) {
    unsafe { syscall1(SYS_NT_CLOSE, handle as u64); }
}

/// Create a directory.
pub fn mkdir(path: &str) -> u64 {
    unsafe { syscall2(SYS_NT_CREATE_DIRECTORY, path.as_ptr() as u64, path.len() as u64) }
}

/// Delete a file.
pub fn delete(path: &str) -> u64 {
    unsafe { syscall2(SYS_NT_DELETE_FILE, path.as_ptr() as u64, path.len() as u64) }
}

// -------------------------------------------------------------------------
// Safe wrappers — memory
// -------------------------------------------------------------------------

/// Allocate virtual memory. Returns address (0 on failure).
pub fn alloc_memory(size: usize) -> u64 {
    unsafe { syscall2(SYS_NT_ALLOCATE_VIRTUAL_MEMORY, 0, size as u64) }
}

/// Free virtual memory.
pub fn free_memory(addr: u64, size: usize) {
    unsafe { syscall2(SYS_NT_FREE_VIRTUAL_MEMORY, addr, size as u64); }
}

// -------------------------------------------------------------------------
// Safe wrappers — IPC
// -------------------------------------------------------------------------

pub fn create_event(auto_reset: bool) -> u64 {
    unsafe { syscall1(SYS_NT_CREATE_EVENT, auto_reset as u64) }
}

pub fn set_event(id: u32) -> u64 {
    unsafe { syscall1(SYS_NT_SET_EVENT, id as u64) }
}

pub fn create_mutex() -> u64 {
    unsafe { syscall0(SYS_NT_CREATE_MUTEX) }
}

pub fn release_mutex(id: u32) -> u64 {
    unsafe { syscall1(SYS_NT_RELEASE_MUTEX, id as u64) }
}

pub fn create_semaphore(max_count: u32) -> u64 {
    unsafe { syscall1(SYS_NT_CREATE_SEMAPHORE, max_count as u64) }
}

pub fn release_semaphore(id: u32) -> u64 {
    unsafe { syscall1(SYS_NT_RELEASE_SEMAPHORE, id as u64) }
}

pub fn create_pipe() -> u64 {
    unsafe { syscall0(SYS_NT_CREATE_PIPE) }
}

// -------------------------------------------------------------------------
// Safe wrappers — process wait (waitpid)
// -------------------------------------------------------------------------

/// Wait for a process to exit. Returns the exit code.
pub fn waitpid(pid: u32) -> i32 {
    unsafe { syscall1(SYS_NT_WAIT_FOR_PROCESS, pid as u64) as i32 }
}

// -------------------------------------------------------------------------
// Safe wrappers — GUI
// -------------------------------------------------------------------------

/// Window event type received from the kernel.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WindowEvent {
    pub event_type: u32,  // 1=MouseMove, 2=MouseDown, 3=MouseUp, 4=KeyDown, 5=KeyUp
    pub x_or_code: u32,   // x position or keycode
    pub y: u32,            // y position (0 for key events)
    pub button: u32,       // mouse button (0 for key events)
}

/// Create a GUI window. Returns window ID (0 on failure).
pub fn gui_create_window(title: &str, width: u32, height: u32) -> u32 {
    let dimensions = ((width as u64) << 16) | (height as u64);
    unsafe {
        syscall3(SYS_GUI_CREATE_WINDOW, title.as_ptr() as u64, title.len() as u64, dimensions) as u32
    }
}

/// Destroy a GUI window.
pub fn gui_destroy_window(win_id: u32) {
    unsafe { syscall1(SYS_GUI_DESTROY_WINDOW, win_id as u64); }
}

/// Set a window's content text.
pub fn gui_set_content(win_id: u32, text: &str) {
    unsafe { syscall3(SYS_GUI_SET_CONTENT, win_id as u64, text.as_ptr() as u64, text.len() as u64); }
}

/// Poll for a window event. Returns Some(event) if available, None otherwise.
pub fn gui_poll_event(win_id: u32) -> Option<WindowEvent> {
    let mut event = WindowEvent { event_type: 0, x_or_code: 0, y: 0, button: 0 };
    let result = unsafe {
        syscall2(SYS_GUI_POLL_EVENT, win_id as u64, &mut event as *mut WindowEvent as u64)
    };
    if result == 0 {
        Some(event)
    } else {
        None
    }
}

// -------------------------------------------------------------------------
// Print macros — format into stack buffer, then debug_print
// -------------------------------------------------------------------------

/// Writer that writes into the kernel debug console via syscall.
struct DebugWriter;

impl Write for DebugWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        debug_print(s);
        Ok(())
    }
}

/// Internal function used by the print macros.
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    let _ = DebugWriter.write_fmt(args);
}

// -------------------------------------------------------------------------
// Safe wrappers — network / sockets
// -------------------------------------------------------------------------

/// Create a TCP socket. Returns FD.
pub fn net_socket() -> u32 {
    unsafe { syscall0(SYS_NET_SOCKET) as u32 }
}

/// Connect a socket to a remote address. ip is a 4-byte array pointer.
pub fn net_connect(fd: u32, ip: &[u8; 4], port: u16) -> u64 {
    unsafe { syscall3(SYS_NET_CONNECT, fd as u64, ip.as_ptr() as u64, port as u64) }
}

/// Send data on a connected socket.
pub fn net_send(fd: u32, data: &[u8]) -> u64 {
    unsafe { syscall3(SYS_NET_SEND, fd as u64, data.as_ptr() as u64, data.len() as u64) }
}

/// Receive data from a connected socket.
pub fn net_recv(fd: u32, buf: &mut [u8]) -> usize {
    unsafe { syscall3(SYS_NET_RECV, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) as usize }
}

/// Close a socket.
pub fn net_close(fd: u32) -> u64 {
    unsafe { syscall1(SYS_NET_CLOSE, fd as u64) }
}

/// Bind a socket to a port.
pub fn net_bind(fd: u32, port: u16) -> u64 {
    unsafe { syscall2(SYS_NET_BIND, fd as u64, port as u64) }
}

/// Listen on a socket.
pub fn net_listen(fd: u32) -> u64 {
    unsafe { syscall1(SYS_NET_LISTEN, fd as u64) }
}

/// Accept a connection. Returns new FD.
pub fn net_accept(fd: u32) -> u32 {
    unsafe { syscall1(SYS_NET_ACCEPT, fd as u64) as u32 }
}

// -------------------------------------------------------------------------
// Safe wrappers — GUI pixel buffer
// -------------------------------------------------------------------------

/// Set a pixel buffer as the window's content. buf is XRGB8888 pixel data.
pub fn gui_set_pixel_buffer(window_id: u32, buf: &[u32], width: u32, height: u32) -> u64 {
    unsafe { syscall4(SYS_GUI_SET_PIXEL_BUFFER, window_id as u64,
             buf.as_ptr() as u64, width as u64, height as u64) }
}

/// Get window dimensions (writes width/height to provided pointers).
pub fn gui_get_window_size(window_id: u32) -> (u32, u32) {
    let mut wh = [0u32; 2];
    unsafe { syscall2(SYS_GUI_GET_WINDOW_SIZE, window_id as u64, wh.as_mut_ptr() as u64); }
    (wh[0], wh[1])
}

/// Print to the kernel debug console (no newline).
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*))
    };
}

/// Print to the kernel debug console with a trailing newline.
#[macro_export]
macro_rules! println {
    () => { $crate::print!("\n") };
    ($($arg:tt)*) => {
        $crate::_print(format_args!($($arg)*));
        $crate::debug_print("\n");
    };
}
