#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, exit};

#[unsafe(no_mangle)]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    if argc < 2 {
        println!("Usage: cat <file> [file2 ...]");
        exit(1);
    }

    for i in 1..argc {
        let arg_ptr = unsafe { *argv.add(i as usize) };
        let path = unsafe { cstr_to_str(arg_ptr) };

        let fd = libcantaya::open(path);
        if fd == 0 || fd == u64::MAX {
            println!("cat: {}: No such file", path);
            continue;
        }

        let handle = fd as u32;
        let mut buf = [0u8; 512];
        loop {
            let n = libcantaya::read(handle, &mut buf);
            if n == 0 || n == u64::MAX {
                break;
            }
            // Print the bytes we read
            if let Ok(s) = core::str::from_utf8(&buf[..n as usize]) {
                debug_print(s);
            }
        }
        libcantaya::close(handle);
    }

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
    debug_print("[cat] PANIC!\n");
    loop { unsafe { { #[cfg(target_arch = "aarch64")] asm!("wfi"); #[cfg(target_arch = "x86_64")] asm!("hlt"); }; } }
}
