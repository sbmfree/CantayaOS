// About CantayaOS Application

extern crate alloc;

use alloc::string::String;
use core::fmt::Write;

use crate::desktop::{InputResult, TITLE_ACTIVE, draw_text, draw_raised_rect, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};

pub struct AboutState;

impl AboutState {
    pub fn new() -> Self { Self }
}

pub(super) fn about_draw(win: &Window, _state: &AboutState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();

    // CantayaOS logo area
    let logo_y = cy + 16;
    let center_x = cx + cw / 2;

    // Draw a simple "logo" — a box with OS name
    let logo_w: u32 = 200;
    let logo_x = center_x.saturating_sub(logo_w / 2);
    fill_rect(logo_x, logo_y, logo_w, 40, TITLE_ACTIVE);
    draw_raised_rect(logo_x, logo_y, logo_w, 40);

    let text = "CantayaOS";
    let text_x = center_x.saturating_sub(text.len() as u32 * CHAR_WIDTH / 2);
    draw_text(text_x, logo_y + 12, text, Color::WHITE);

    // Version info
    let mut s = String::new();
    write!(s, "Version {}", env!("CARGO_PKG_VERSION")).ok();
    let text_x = center_x.saturating_sub(s.len() as u32 * CHAR_WIDTH / 2);
    draw_text(text_x, logo_y + 52, &s, Color::BLACK);

    // Description
    let info_y = logo_y + 80;
    let lines = [
        "A hobby operating system written in Rust.",
        "Inspired by Windows NT architecture.",
        "",
        "Architecture: x86_64 (AMD64)",
        "Kernel Type:  Hybrid Monolithic",
        "Graphics:     1920x1080 Framebuffer",
        "Scheduler:    Preemptive Round-Robin",
        "Boot:         UEFI via custom bootloader",
        "",
        "Built with love and Rust nightly.",
    ];

    for (i, line) in lines.iter().enumerate() {
        let x = cx + 20;
        let y = info_y + i as u32 * (CHAR_HEIGHT + 2);
        draw_text(x, y, line, Color::BLACK);
    }
}
