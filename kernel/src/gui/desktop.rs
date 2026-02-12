//! Desktop rendering — gradient background and 32px taskbar at the bottom.

use crate::drivers::framebuffer::{self, SCREEN_WIDTH, SCREEN_HEIGHT, FONT_HEIGHT};

// Colours
const TASKBAR_BG: u32   = 0xFF1A1A2E; // dark blue-black
const TASKBAR_FG: u32   = 0xFFCCCCDD;
const TASKBAR_H: u32    = 32;

/// Draw the desktop background (a vertical gradient from sky blue to darker blue).
pub fn draw_background(fb: &mut framebuffer::Framebuffer) {
    for y in 0..SCREEN_HEIGHT {
        // Gradient from (40, 100, 180) at top to (20, 40, 80) at bottom
        let t = y as f32 / SCREEN_HEIGHT as f32;
        let r = lerp(40, 20, t) as u32;
        let g = lerp(100, 40, t) as u32;
        let b = lerp(180, 80, t) as u32;
        let color = 0xFF000000 | (r << 16) | (g << 8) | b;
        fb.draw_hline(0, y as i32, SCREEN_WIDTH as u32, color);
    }
}

/// Draw the taskbar at the bottom of the screen.
pub fn draw_taskbar(fb: &mut framebuffer::Framebuffer, window_count: usize) {
    let taskbar_y = SCREEN_HEIGHT as u32 - TASKBAR_H;

    // Background
    fb.fill_rect(0, taskbar_y as i32, SCREEN_WIDTH as u32, TASKBAR_H, TASKBAR_BG);

    // Top border line
    fb.draw_hline(0, taskbar_y as i32, SCREEN_WIDTH as u32, 0xFF333355);

    // "CantayaOS" button on the left
    fb.fill_rect(4, taskbar_y as i32 + 4, 90, TASKBAR_H - 8, 0xFF2244AA);
    let text_y = taskbar_y as i32 + (TASKBAR_H as i32 - FONT_HEIGHT as i32) / 2;
    fb.draw_string_transparent(12, text_y, "CantayaOS", 0xFFFFFFFF);

    // Show window count
    if window_count > 0 {
        let mut buf = [0u8; 20];
        let s = format_usize(window_count, &mut buf);
        let status_text_width = (s.len() + 10) * 8; // rough estimate
        let _ = status_text_width;

        // Simple: show "N window(s)" near the right side
        let right_x = SCREEN_WIDTH as i32 - 120;
        fb.draw_string_transparent(right_x, text_y, s, TASKBAR_FG);
        fb.draw_string_transparent(right_x + (s.len() as i32 * 8) + 4, text_y,
                                    if window_count == 1 { "window" } else { "windows" },
                                    TASKBAR_FG);
    }
}

/// Area above the taskbar usable for windows.
pub const fn usable_height() -> u32 {
    SCREEN_HEIGHT as u32 - TASKBAR_H
}

fn lerp(a: u32, b: u32, t: f32) -> u32 {
    ((a as f32) * (1.0 - t) + (b as f32) * t) as u32
}

fn format_usize(val: usize, buf: &mut [u8; 20]) -> &str {
    if val == 0 {
        buf[0] = b'0';
        return unsafe { core::str::from_utf8_unchecked(&buf[..1]) };
    }
    let mut n = val;
    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    unsafe { core::str::from_utf8_unchecked(&buf[i..]) }
}
