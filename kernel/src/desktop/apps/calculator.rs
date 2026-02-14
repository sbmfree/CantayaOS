// Calculator Application

extern crate alloc;

use alloc::string::String;
use core::fmt::Write;

use crate::desktop::{
    InputResult, BUTTON_FACE,
    draw_text, draw_sunken_rect, fill_rect, draw_raised_rect,
};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct CalcState {
    display: String,
    accumulator: f64,
    current_input: String,
    operator: Option<char>,
    just_computed: bool,
}

impl CalcState {
    pub fn new() -> Self {
        Self {
            display: String::from("0"),
            accumulator: 0.0,
            current_input: String::new(),
            operator: None,
            just_computed: false,
        }
    }
}

pub(super) fn calc_draw(win: &Window, state: &CalcState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();

    // Display area (sunken panel)
    let disp_x = cx + 8;
    let disp_y = cy + 8;
    let disp_w = cw - 16;
    let disp_h = CHAR_HEIGHT + 12;

    fill_rect(disp_x, disp_y, disp_w, disp_h, Color::WHITE);
    draw_sunken_rect(disp_x, disp_y, disp_w, disp_h);

    // Display text (right-aligned)
    let max_chars = ((disp_w - 8) / CHAR_WIDTH) as usize;
    let text: &str = if state.display.len() > max_chars {
        &state.display[state.display.len() - max_chars..]
    } else {
        &state.display
    };
    // Right-align
    let text_x = disp_x + disp_w - 4 - text.len() as u32 * CHAR_WIDTH;
    draw_text(text_x, disp_y + 6, text, Color::BLACK);

    // Buttons layout
    let btn_w: u32 = 40;
    let btn_h: u32 = 28;
    let gap: u32 = 4;
    let start_x = cx + 8;
    let start_y = disp_y + disp_h + 12;

    let buttons: [[&str; 4]; 5] = [
        ["C", "/", "*", "-"],
        ["7", "8", "9", "+"],
        ["4", "5", "6", " "],
        ["1", "2", "3", "="],
        ["0", " ", ".", " "],
    ];

    for (row, row_buttons) in buttons.iter().enumerate() {
        for (col, &label) in row_buttons.iter().enumerate() {
            if label == " " { continue; }

            let bx = start_x + col as u32 * (btn_w + gap);
            let by = start_y + row as u32 * (btn_h + gap);

            // Special sizing for = button (tall) and 0 button (wide)
            let (bw, bh) = if label == "=" {
                (btn_w, btn_h * 2 + gap) // tall
            } else if label == "0" {
                (btn_w * 2 + gap, btn_h) // wide
            } else {
                (btn_w, btn_h)
            };

            // Skip duplicates caused by special sizing
            if label == "=" && row != 3 { continue; }
            if label == "0" && row != 4 { continue; }

            fill_rect(bx, by, bw, bh, BUTTON_FACE);
            draw_raised_rect(bx, by, bw, bh);

            let text_x = bx + (bw - CHAR_WIDTH) / 2;
            let text_y = by + (bh - CHAR_HEIGHT) / 2;
            draw_text(text_x, text_y, label, Color::BLACK);
        }
    }

    // Footer
    let footer_y = start_y + 5 * (btn_h + gap) + 4;
    draw_text(cx + 8, footer_y, "Type digits, +-*/, Enter=", Color::rgb(0x80, 0x80, 0x80));
}

pub(super) fn calc_input(state: &mut CalcState, event: &KeyEvent) -> InputResult {
    match event.ascii {
        b'0'..=b'9' => {
            if state.just_computed {
                state.current_input.clear();
                state.just_computed = false;
            }
            state.current_input.push(event.ascii as char);
            state.display = state.current_input.clone();
            InputResult::Redraw
        }
        b'.' => {
            if !state.current_input.contains('.') {
                if state.current_input.is_empty() {
                    state.current_input.push('0');
                }
                state.current_input.push('.');
                state.display = state.current_input.clone();
            }
            InputResult::Redraw
        }
        b'+' | b'-' | b'*' | b'/' => {
            if !state.current_input.is_empty() {
                let val = parse_f64(&state.current_input);
                if state.operator.is_some() {
                    state.accumulator = calc_compute(state.accumulator, val, state.operator.unwrap());
                } else {
                    state.accumulator = val;
                }
                state.current_input.clear();
            }
            state.operator = Some(event.ascii as char);
            let mut s = String::new();
            write!(s, "{}", format_number(state.accumulator)).ok();
            state.display = s;
            state.just_computed = false;
            InputResult::Redraw
        }
        b'\n' | b'=' => {
            if !state.current_input.is_empty() {
                let val = parse_f64(&state.current_input);
                if let Some(op) = state.operator {
                    state.accumulator = calc_compute(state.accumulator, val, op);
                } else {
                    state.accumulator = val;
                }
                state.current_input.clear();
                state.operator = None;
            }
            let mut s = String::new();
            write!(s, "{}", format_number(state.accumulator)).ok();
            state.display = s;
            state.just_computed = true;
            InputResult::Redraw
        }
        b'c' | b'C' => {
            state.accumulator = 0.0;
            state.current_input.clear();
            state.operator = None;
            state.display = String::from("0");
            state.just_computed = false;
            InputResult::Redraw
        }
        0x08 => {
            // Backspace — delete last digit
            if !state.current_input.is_empty() {
                state.current_input.pop();
                if state.current_input.is_empty() {
                    state.display = String::from("0");
                } else {
                    state.display = state.current_input.clone();
                }
            }
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}

fn calc_compute(a: f64, b: f64, op: char) -> f64 {
    match op {
        '+' => a + b,
        '-' => a - b,
        '*' => a * b,
        '/' => if b != 0.0 { a / b } else { 0.0 },
        _ => b,
    }
}

/// Simple f64 parser for no_std (handles integers and simple decimals).
fn parse_f64(s: &str) -> f64 {
    let bytes = s.as_bytes();
    let mut result: f64 = 0.0;
    let mut decimal_part = false;
    let mut decimal_divisor: f64 = 10.0;
    let mut negative = false;
    let mut start = 0;

    if !bytes.is_empty() && bytes[0] == b'-' {
        negative = true;
        start = 1;
    }

    for &b in &bytes[start..] {
        if b == b'.' {
            decimal_part = true;
            continue;
        }
        if b >= b'0' && b <= b'9' {
            let digit = (b - b'0') as f64;
            if decimal_part {
                result += digit / decimal_divisor;
                decimal_divisor *= 10.0;
            } else {
                result = result * 10.0 + digit;
            }
        }
    }

    if negative { -result } else { result }
}

/// Format a number for display — integer if whole, else with decimals.
fn format_number(n: f64) -> String {
    let mut s = String::new();
    // Check if it's a whole number
    let rounded = n as i64;
    if (n - rounded as f64).abs() < 0.0000001 {
        write!(s, "{}", rounded).ok();
    } else {
        // Show up to 6 decimal places
        write!(s, "{}", n as i64).ok();
        let frac = ((n - (n as i64) as f64).abs() * 1000000.0) as u64;
        // Trim trailing zeros
        let mut frac_str = String::new();
        write!(frac_str, "{:06}", frac).ok();
        let trimmed = frac_str.trim_end_matches('0');
        if !trimmed.is_empty() {
            s.push('.');
            s.push_str(trimmed);
        }
    }
    s
}
