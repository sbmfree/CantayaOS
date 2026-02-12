//! CantayaOS Init Process
//!
//! This is the first user-space process launched by the kernel.
//! It is responsible for starting system services and the user shell.
//!
//! NOTE: This is a template. Actual user-space execution requires
//! the kernel's EL0 transition and user-space ABI to be fully wired.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;

// -------------------------------------------------------------------------
// Syscall wrappers (CantayaOS NT-like ABI)
// -------------------------------------------------------------------------

/// Raw system call: number in x8, args in x0-x4, return in x0
#[inline(always)]
unsafe fn syscall(num: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
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

// Syscall numbers
const SYS_NT_CREATE_PROCESS: u64 = 0x0001;
const SYS_NT_YIELD: u64 = 0x0007;
const SYS_NT_SLEEP: u64 = 0x0008;
const SYS_NT_WRITE_FILE: u64 = 0x0022;
const SYS_NT_DEBUG_PRINT: u64 = 0x00FF;

/// Print a string to the kernel console
fn debug_print(s: &str) {
    unsafe {
        syscall(SYS_NT_DEBUG_PRINT, s.as_ptr() as u64, s.len() as u64, 0, 0);
    }
}

/// Write to stdout (handle 1)
fn write_stdout(s: &str) {
    unsafe {
        syscall(SYS_NT_WRITE_FILE, 1, s.as_ptr() as u64, s.len() as u64, 0);
    }
}

/// Yield the current time slice
fn yield_cpu() {
    unsafe {
        syscall(SYS_NT_YIELD, 0, 0, 0, 0);
    }
}

/// Sleep for the given number of milliseconds
fn sleep_ms(ms: u64) {
    unsafe {
        syscall(SYS_NT_SLEEP, ms, 0, 0, 0);
    }
}

// -------------------------------------------------------------------------
// Init entry point
// -------------------------------------------------------------------------

#[no_mangle]
pub extern "C" fn _start() -> ! {
    debug_print("[init] CantayaOS init process started\n");
    debug_print("[init] Starting system services...\n");

    // TODO: Start system services
    // - Session Manager (smss)
    // - Service Control Manager (services)
    // - Local Security Authority (lsass)

    debug_print("[init] System ready.\n");

    // Idle loop
    loop {
        yield_cpu();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[init] PANIC!\n");
    loop {
        unsafe { asm!("wfi"); }
    }
}
