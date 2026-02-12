//! Hello World — simplest CantayaOS userspace program.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, exit};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    println!("Hello from CantayaOS userspace!");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[hello] PANIC!\n");
    loop { unsafe { { #[cfg(target_arch = "aarch64")] asm!("wfi"); #[cfg(target_arch = "x86_64")] asm!("hlt"); }; } }
}
