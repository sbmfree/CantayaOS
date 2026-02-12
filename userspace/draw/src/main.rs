#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, exit};

/// Simple pixel-buffer GUI demo.
/// Creates a window, draws a colourful gradient, then animates it.
#[unsafe(no_mangle)]
pub extern "C" fn _start(_argc: u64, _argv: *const *const u8) -> ! {
    println!("[draw] Pixel buffer demo starting...");

    // Create a 200x150 window
    let win = libcantaya::gui_create_window("Draw Demo", 200, 150);
    if win == 0 || win == u32::MAX {
        println!("[draw] Failed to create window");
        exit(1);
    }

    let (w, h) = libcantaya::gui_get_window_size(win);
    let width = if w > 0 { w } else { 200 };
    let height = if h > 0 { h } else { 150 };
    let pixels = (width * height) as usize;

    // Allocate pixel buffer in userspace
    let buf_size = pixels * 4; // u32 per pixel
    let buf_addr = libcantaya::alloc_memory(buf_size);
    if buf_addr == 0 {
        println!("[draw] Failed to allocate pixel buffer");
        exit(1);
    }

    let buf = unsafe { core::slice::from_raw_parts_mut(buf_addr as *mut u32, pixels) };

    // Animate a few frames
    for frame in 0u32..60 {
        // Render gradient with animated offset
        for y in 0..height {
            for x in 0..width {
                let r = ((x + frame * 3) % 256) as u8;
                let g = ((y + frame * 2) % 256) as u8;
                let b = (((x + y) / 2 + frame * 4) % 256) as u8;
                buf[(y * width + x) as usize] = 0xFF000000 | (r as u32) << 16 | (g as u32) << 8 | (b as u32);
            }
        }

        libcantaya::gui_set_pixel_buffer(win, buf, width, height);

        // Check for window close event
        if let Some(ev) = libcantaya::gui_poll_event(win) {
            // event_type 0 = close (or we just break on any event for simplicity)
            if ev.event_type == 0 {
                break;
            }
        }

        libcantaya::sleep(50); // ~20 fps
    }

    // Clean up
    libcantaya::gui_destroy_window(win);
    libcantaya::free_memory(buf_addr, buf_size);

    println!("[draw] Demo complete.");
    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[draw] PANIC!\n");
    loop { unsafe { { #[cfg(target_arch = "aarch64")] asm!("wfi"); #[cfg(target_arch = "x86_64")] asm!("hlt"); }; } }
}
