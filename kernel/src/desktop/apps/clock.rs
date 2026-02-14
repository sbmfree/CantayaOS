// Clock — Analog/Digital Clock Application

extern crate alloc;

use alloc::string::String;
use core::fmt::Write;

use crate::desktop::{draw_text, draw_sunken_rect, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};

pub struct ClockState {
    _dummy: u8,
}

impl ClockState {
    pub fn new() -> Self {
        Self { _dummy: 0 }
    }
}

pub fn clock_draw(win: &Window, _state: &ClockState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    let dt = crate::hal::rtc::read_datetime();

    // Digital clock display
    let mut time_str = String::new();
    write!(time_str, "{:02}:{:02}:{:02}", dt.hour, dt.minute, dt.second).ok();

    let mut date_str = String::new();
    write!(date_str, "{:04}-{:02}-{:02}", dt.year, dt.month, dt.day).ok();

    // Background panel
    let panel_x = cx + 10;
    let panel_w = cw - 20;
    let panel_h = CHAR_HEIGHT * 3 + 50;
    fill_rect(panel_x, cy + 10, panel_w, panel_h, Color::rgb(0x10, 0x10, 0x30));
    draw_sunken_rect(panel_x, cy + 10, panel_w, panel_h);

    // 3× scale time digits
    let text_w = time_str.len() as u32 * CHAR_WIDTH * 3;
    let text_x = cx + (cw.saturating_sub(text_w)) / 2;
    let text_y = cy + 30;

    {
        let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
        for (ci, c) in time_str.chars().enumerate() {
            let bitmap = crate::graphics::font::get_char_bitmap(c);
            for (dy, &row_bits) in bitmap.iter().enumerate() {
                for dx in 0..8u32 {
                    if (row_bits >> (7 - dx)) & 1 != 0 {
                        for sy in 0..3u32 {
                            for sx in 0..3u32 {
                                let px = text_x + ci as u32 * CHAR_WIDTH * 3 + dx * 3 + sx;
                                let py = text_y + dy as u32 * 3 + sy;
                                fb.put_pixel(px, py, Color::rgb(0x00, 0xFF, 0x80));
                            }
                        }
                    }
                }
            }
        }
    }

    // Date below
    let date_w = date_str.len() as u32 * CHAR_WIDTH;
    let date_x = cx + (cw.saturating_sub(date_w)) / 2;
    let date_y = text_y + CHAR_HEIGHT * 3 + 12;
    draw_text(date_x, date_y, &date_str, Color::rgb(0x80, 0xC0, 0xFF));

    // Day of week
    let dow = day_of_week(dt.year as u32, dt.month as u32, dt.day as u32);
    let dow_name = match dow {
        0 => "Sunday", 1 => "Monday", 2 => "Tuesday", 3 => "Wednesday",
        4 => "Thursday", 5 => "Friday", 6 => "Saturday", _ => "???",
    };
    let dow_x = cx + (cw.saturating_sub(dow_name.len() as u32 * CHAR_WIDTH)) / 2;
    let dow_y = date_y + CHAR_HEIGHT + 4;
    draw_text(dow_x, dow_y, dow_name, Color::rgb(0xFF, 0xFF, 0x80));

    // Analog clock face
    let analog_y = cy + panel_h + 24;
    let available_h = ch.saturating_sub(panel_h + 34);
    let radius = available_h.min(cw) / 2 - 10;
    let center_x = cx + cw / 2;
    let center_y = analog_y + available_h / 2;

    if radius > 20 {
        let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
        // Clock face circle
        for angle_deg in 0..360 {
            let a = (angle_deg as f32) * 3.14159265 / 180.0;
            let px = center_x as f32 + radius as f32 * cos_approx(a);
            let py = center_y as f32 + radius as f32 * sin_approx(a);
            fb.put_pixel(px as u32, py as u32, Color::BLACK);
        }

        // Hour markers
        for h in 0..12 {
            let a = (h as f32) * 3.14159265 / 6.0 - 3.14159265 / 2.0;
            let inner = radius as f32 * 0.85;
            let outer = radius as f32 * 0.95;
            for t in 0..10 {
                let f = inner + (outer - inner) * t as f32 / 10.0;
                let px = center_x as f32 + f * cos_approx(a);
                let py = center_y as f32 + f * sin_approx(a);
                fb.put_pixel(px as u32, py as u32, Color::BLACK);
            }
        }

        // Hour hand
        let h_angle = ((dt.hour % 12) as f32 + dt.minute as f32 / 60.0) * 3.14159265 / 6.0 - 3.14159265 / 2.0;
        let h_len = radius as f32 * 0.5;
        draw_hand(&mut fb, center_x, center_y, h_angle, h_len, Color::BLACK);

        // Minute hand
        let m_angle = (dt.minute as f32 + dt.second as f32 / 60.0) * 3.14159265 / 30.0 - 3.14159265 / 2.0;
        let m_len = radius as f32 * 0.7;
        draw_hand(&mut fb, center_x, center_y, m_angle, m_len, Color::rgb(0x00, 0x00, 0x80));

        // Second hand
        let s_angle = dt.second as f32 * 3.14159265 / 30.0 - 3.14159265 / 2.0;
        let s_len = radius as f32 * 0.8;
        draw_hand(&mut fb, center_x, center_y, s_angle, s_len, Color::rgb(0xFF, 0x00, 0x00));

        // Centre dot
        for dy in 0..3u32 {
            for dx in 0..3u32 {
                fb.put_pixel(center_x - 1 + dx, center_y - 1 + dy, Color::BLACK);
            }
        }
    }
}

/// Simple sine approximation (Bhaskara I formula)
fn sin_approx(x: f32) -> f32 {
    let pi = 3.14159265f32;
    let mut a = x % (2.0 * pi);
    if a < 0.0 { a += 2.0 * pi; }
    let sign = if a > pi { a -= pi; -1.0 } else { 1.0 };
    sign * (16.0 * a * (pi - a)) / (5.0 * pi * pi - 4.0 * a * (pi - a))
}

fn cos_approx(x: f32) -> f32 {
    sin_approx(x + 3.14159265 / 2.0)
}

/// Draw a clock hand as a thick line from centre to endpoint.
fn draw_hand(fb: &mut crate::graphics::framebuffer::Framebuffer, cx: u32, cy: u32, angle: f32, length: f32, color: Color) {
    let steps = (length * 2.0) as u32;
    for t in 0..steps {
        let f = t as f32 / steps as f32;
        let px = cx as f32 + length * f * cos_approx(angle);
        let py = cy as f32 + length * f * sin_approx(angle);
        let px = px as u32;
        let py = py as u32;
        fb.put_pixel(px, py, color);
        fb.put_pixel(px + 1, py, color);
        fb.put_pixel(px, py + 1, color);
    }
}

/// Zeller-like day-of-week: 0=Sun, 1=Mon, ... 6=Sat
fn day_of_week(mut y: u32, mut m: u32, d: u32) -> u32 {
    if m < 3 {
        m += 12;
        y -= 1;
    }
    (d + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400 + 6) % 7
}
