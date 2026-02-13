// Taskbar for CantayaOS Desktop
//
// A bottom-of-screen taskbar with:
//   - Start button (F1 to activate)
//   - Running application buttons
//   - System clock (from RTC)
//
// Inspired by the Windows 95/98/2000 taskbar.

extern crate alloc;

use alloc::string::String;
use core::fmt::Write;

use super::{
    TASKBAR_HEIGHT, TASKBAR_BG,
    START_BTN_TEXT,
    HIGHLIGHT, CLOCK_TEXT,
    BUTTON_FACE,
    draw_text, draw_raised_rect, draw_sunken_rect, fill_rect,
};
use super::wm::WindowManager;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::CHAR_WIDTH;
use crate::hal::rtc;

/// Draw the taskbar at the bottom of the screen.
pub fn draw(wm: &WindowManager, screen_w: u32, screen_h: u32) {
    let tb_y = screen_h - TASKBAR_HEIGHT;

    // Taskbar background
    fill_rect(0, tb_y, screen_w, TASKBAR_HEIGHT, TASKBAR_BG);

    // Top edge highlight
    {
        let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
        fb.fill_rect(0, tb_y, screen_w, 1, HIGHLIGHT);
    }

    // Start button
    let start_x: u32 = 2;
    let start_y = tb_y + 2;
    let start_w: u32 = 60;
    let start_h = TASKBAR_HEIGHT - 4;

    fill_rect(start_x, start_y, start_w, start_h, BUTTON_FACE);
    draw_raised_rect(start_x, start_y, start_w, start_h);
    draw_text(start_x + 6, start_y + 4, "Start", START_BTN_TEXT);

    // F1 hint
    draw_text(start_x + start_w + 4, start_y + 4, "[F1]", Color::rgb(0x80, 0x80, 0x80));

    // Running window buttons
    let btn_area_x = start_x + start_w + 40;
    let btn_area_w = screen_w - btn_area_x - 100; // Leave space for clock
    let titles = wm.window_titles();
    let btn_count = titles.len();

    if btn_count > 0 {
        let btn_w = (btn_area_w / btn_count as u32).min(160);

        for (i, (_, title, focused)) in titles.iter().enumerate() {
            let bx = btn_area_x + i as u32 * (btn_w + 2);
            let by = tb_y + 2;
            let bh = TASKBAR_HEIGHT - 4;

            if *focused {
                // Sunken style for active window
                fill_rect(bx, by, btn_w, bh, BUTTON_FACE);
                draw_sunken_rect(bx, by, btn_w, bh);
            } else {
                // Raised style for inactive windows
                fill_rect(bx, by, btn_w, bh, BUTTON_FACE);
                draw_raised_rect(bx, by, btn_w, bh);
            }

            // Truncate title to fit
            let max_chars = ((btn_w - 8) / CHAR_WIDTH) as usize;
            let display: &str = if title.len() > max_chars {
                &title[..max_chars]
            } else {
                title
            };
            draw_text(bx + 4, by + 4, display, Color::BLACK);
        }
    }

    // Clock area (sunken panel on the right)
    let clock_w: u32 = 80;
    let clock_x = screen_w - clock_w - 4;
    let clock_y = tb_y + 2;
    let clock_h = TASKBAR_HEIGHT - 4;

    fill_rect(clock_x, clock_y, clock_w, clock_h, BUTTON_FACE);
    draw_sunken_rect(clock_x, clock_y, clock_w, clock_h);

    // Read RTC time
    let dt = rtc::read_datetime();
    let mut time_buf = String::new();
    write!(time_buf, "{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second).ok();
    draw_text(clock_x + 8, clock_y + 4, &time_buf, CLOCK_TEXT);
}
