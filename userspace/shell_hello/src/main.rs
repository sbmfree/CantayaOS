//! Shell Hello — demonstrates multiple CantayaOS syscalls.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, get_pid, get_tid, sleep, yield_thread, exit};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=== CantayaOS Syscall Demo ===");
    println!("  PID: {}", get_pid());
    println!("  TID: {}", get_tid());

    println!("[demo] Yielding CPU...");
    yield_thread();
    println!("[demo] Returned from yield.");

    println!("[demo] Sleeping 1000 ms...");
    sleep(1000);
    println!("[demo] Awake!");

    println!("[demo] All syscalls exercised successfully.");
    println!("=== Demo complete ===");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[shell_hello] PANIC!\n");
    loop { unsafe { asm!("wfi"); } }
}
