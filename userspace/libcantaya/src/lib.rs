//! CantayaOS Userspace Syscall Library
//!
//! Provides safe Rust wrappers around CantayaOS system calls
//! for use by user-space applications.

#![no_std]

use core::arch::asm;

// -------------------------------------------------------------------------
// Raw syscall interface
// -------------------------------------------------------------------------

/// Perform a raw system call.
/// Convention: syscall number in x8, arguments in x0-x4, return in x0.
#[inline(always)]
pub unsafe fn syscall0(num: u64) -> u64 {
    let ret: u64;
    asm!(
        "svc #0",
        in("x8") num,
        lateout("x0") ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall1(num: u64, a0: u64) -> u64 {
    let ret: u64;
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(num: u64, a0: u64, a1: u64) -> u64 {
    let ret: u64;
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(num: u64, a0: u64, a1: u64, a2: u64) -> u64 {
    let ret: u64;
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall4(num: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
    let ret: u64;
    asm!(
        "svc #0",
        in("x8") num,
        inlateout("x0") a0 => ret,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
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

pub const SYS_NT_ALLOCATE_VIRTUAL_MEMORY: u64 = 0x0010;
pub const SYS_NT_FREE_VIRTUAL_MEMORY: u64     = 0x0011;

pub const SYS_NT_CREATE_FILE: u64        = 0x0020;
pub const SYS_NT_READ_FILE: u64          = 0x0021;
pub const SYS_NT_WRITE_FILE: u64         = 0x0022;
pub const SYS_NT_CLOSE: u64             = 0x0023;
pub const SYS_NT_CREATE_DIRECTORY: u64   = 0x0024;

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

pub const SYS_NT_DEBUG_PRINT: u64        = 0x00FF;

// -------------------------------------------------------------------------
// Safe wrappers
// -------------------------------------------------------------------------

pub fn debug_print(s: &str) {
    unsafe { syscall2(SYS_NT_DEBUG_PRINT, s.as_ptr() as u64, s.len() as u64); }
}

pub fn yield_thread() {
    unsafe { syscall0(SYS_NT_YIELD); }
}

pub fn sleep(ms: u64) {
    unsafe { syscall1(SYS_NT_SLEEP, ms); }
}

pub fn get_pid() -> u64 {
    unsafe { syscall0(SYS_NT_GET_PROCESS_ID) }
}

pub fn get_tid() -> u64 {
    unsafe { syscall0(SYS_NT_GET_THREAD_ID) }
}

pub fn exit_process(code: i32) {
    unsafe { syscall2(SYS_NT_TERMINATE_PROCESS, get_pid(), code as u64); }
}

pub fn write(handle: u32, buf: &[u8]) -> u64 {
    unsafe { syscall3(SYS_NT_WRITE_FILE, handle as u64, buf.as_ptr() as u64, buf.len() as u64) }
}

pub fn alloc_memory(size: usize) -> u64 {
    unsafe { syscall2(SYS_NT_ALLOCATE_VIRTUAL_MEMORY, 0, size as u64) }
}

pub fn free_memory(addr: u64, size: usize) {
    unsafe { syscall2(SYS_NT_FREE_VIRTUAL_MEMORY, addr, size as u64); }
}

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
