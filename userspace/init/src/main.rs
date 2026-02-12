//! CantayaOS Init Process
//!
//! First user-space process launched by the kernel.
//! Prints a banner, reports its PID, then enters a heartbeat loop.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, get_pid, sleep};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("=========================================");
    println!("  CantayaOS Init Process");
    println!("  PID: {}", get_pid());
    println!("=========================================");
    println!("[init] System services starting...");
    println!("[init] System ready.");

    // Idle loop — keep init alive
    loop {
        sleep(1000);
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    debug_print("[init] PANIC: ");
    if let Some(loc) = info.location() {
        // Can't use println here because we might be in a bad state
        debug_print(loc.file());
        debug_print("\n");
    } else {
        debug_print("unknown location\n");
    }
    loop { unsafe { asm!("wfi"); } }
}
