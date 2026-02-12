#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, exit};

#[unsafe(no_mangle)]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    // Skip argv[0] (program name), print the rest separated by spaces
    if argc <= 1 {
        println!();
        exit(0);
    }

    for i in 1..argc {
        let arg_ptr = unsafe { *argv.add(i as usize) };
        let arg = unsafe { cstr_to_str(arg_ptr) };
        if i > 1 {
            libcantaya::debug_print(" ");
        }
        libcantaya::debug_print(arg);
    }
    println!();
    exit(0);
}

unsafe fn cstr_to_str<'a>(ptr: *const u8) -> &'a str {
    let mut len = 0usize;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8_unchecked(slice)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[echo] PANIC!\n");
    loop { unsafe { asm!("wfi"); } }
}
