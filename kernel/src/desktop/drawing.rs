// Drawing helper functions for the CantayaOS desktop environment.

use crate::graphics::framebuffer::{Color, FRAMEBUFFER};
use crate::graphics::font::{self, CHAR_WIDTH, CHAR_HEIGHT};
use super::{BUTTON_FACE, BUTTON_SHADOW, BUTTON_HIGHLIGHT, SHADOW, HIGHLIGHT};

/// Draw a string directly onto the framebuffer at pixel coordinates.
pub fn draw_text(x: u32, y: u32, text: &str, color: Color) {
    let mut fb = FRAMEBUFFER.lock();
    for (i, c) in text.chars().enumerate() {
        let cx = x + i as u32 * CHAR_WIDTH;
        let bitmap = font::get_char_bitmap(c);
        for (dy, &row_bits) in bitmap.iter().enumerate() {
            for dx in 0..8u32 {
                if (row_bits >> (7 - dx)) & 1 != 0 {
                    fb.put_pixel(cx + dx, y + dy as u32, color);
                }
            }
        }
    }
}

/// Draw a string with background fill.
pub fn draw_text_bg(x: u32, y: u32, text: &str, fg: Color, bg: Color) {
    let mut fb = FRAMEBUFFER.lock();
    for (i, c) in text.chars().enumerate() {
        let cx = x + i as u32 * CHAR_WIDTH;
        let bitmap = font::get_char_bitmap(c);
        for (dy, &row_bits) in bitmap.iter().enumerate() {
            for dx in 0..8u32 {
                let pixel_set = (row_bits >> (7 - dx)) & 1 != 0;
                let color = if pixel_set { fg } else { bg };
                fb.put_pixel(cx + dx, y + dy as u32, color);
            }
        }
    }
}

/// Draw a 3D raised button/panel (Windows 95 style bevel).
pub fn draw_raised_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut fb = FRAMEBUFFER.lock();
    // Top and left edges = highlight (white)
    fb.fill_rect(x, y, w, 1, HIGHLIGHT);
    fb.fill_rect(x, y, 1, h, HIGHLIGHT);
    // Bottom and right edges = shadow (dark gray)
    fb.fill_rect(x, y + h - 1, w, 1, SHADOW);
    fb.fill_rect(x + w - 1, y, 1, h, SHADOW);
    // Inner shadow
    fb.fill_rect(x + 1, y + h - 2, w - 2, 1, BUTTON_SHADOW);
    fb.fill_rect(x + w - 2, y + 1, 1, h - 2, BUTTON_SHADOW);
}

/// Draw a 3D sunken panel (Windows 95 style inset).
pub fn draw_sunken_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut fb = FRAMEBUFFER.lock();
    // Top and left = shadow
    fb.fill_rect(x, y, w, 1, SHADOW);
    fb.fill_rect(x, y, 1, h, SHADOW);
    // Bottom and right = highlight
    fb.fill_rect(x, y + h - 1, w, 1, HIGHLIGHT);
    fb.fill_rect(x + w - 1, y, 1, h, HIGHLIGHT);
}

/// Fill a rectangle (convenience wrapper).
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: Color) {
    let mut fb = FRAMEBUFFER.lock();
    fb.fill_rect(x, y, w, h, color);
}

/// Draw a simple icon (16x16) from a bitmap pattern.
pub fn draw_icon_16(x: u32, y: u32, icon: &[u16; 16], fg: Color, bg_opt: Option<Color>) {
    let mut fb = FRAMEBUFFER.lock();
    for (row, &bits) in icon.iter().enumerate() {
        for col in 0..16u32 {
            if (bits >> (15 - col)) & 1 != 0 {
                fb.put_pixel(x + col, y + row as u32, fg);
            } else if let Some(bg) = bg_opt {
                fb.put_pixel(x + col, y + row as u32, bg);
            }
        }
    }
}

/// Present the framebuffer.
pub fn present() {
    let mut fb = FRAMEBUFFER.lock();
    fb.present();
}

/// Get screen dimensions.
pub fn screen_size() -> (u32, u32) {
    let fb = FRAMEBUFFER.lock();
    (fb.width, fb.height)
}
