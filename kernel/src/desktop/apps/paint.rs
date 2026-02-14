// Paint — Simple Pixel Drawing App

extern crate alloc;

use alloc::vec::Vec;

use crate::desktop::{InputResult, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::CHAR_HEIGHT;
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub const PAINT_COLS: usize = 100;
pub const PAINT_ROWS: usize = 70;
pub const PAINT_SCALE: u32 = 4;

pub const PAINT_PALETTE: [Color; 16] = [
    Color::BLACK,
    Color::rgb(0xFF, 0xFF, 0xFF),
    Color::rgb(0xFF, 0x00, 0x00),
    Color::rgb(0x00, 0xFF, 0x00),
    Color::rgb(0x00, 0x00, 0xFF),
    Color::rgb(0xFF, 0xFF, 0x00),
    Color::rgb(0xFF, 0x00, 0xFF),
    Color::rgb(0x00, 0xFF, 0xFF),
    Color::rgb(0x80, 0x00, 0x00),
    Color::rgb(0x00, 0x80, 0x00),
    Color::rgb(0x00, 0x00, 0x80),
    Color::rgb(0x80, 0x80, 0x00),
    Color::rgb(0x80, 0x00, 0x80),
    Color::rgb(0x00, 0x80, 0x80),
    Color::rgb(0x80, 0x80, 0x80),
    Color::rgb(0xC0, 0xC0, 0xC0),
];

pub struct PaintState {
    /// Canvas pixels stored as flat array (palette indices).
    /// Canvas is PAINT_COLS x PAINT_ROWS, each "pixel" is a 4×4 block.
    canvas: Vec<u8>,
    /// Current drawing colour (palette index).
    current_color: usize,
    /// Whether eraser mode is on.
    eraser: bool,
}

impl PaintState {
    pub fn new() -> Self {
        Self {
            canvas: alloc::vec![1u8; PAINT_COLS * PAINT_ROWS], // white
            current_color: 0,
            eraser: false,
        }
    }
}

pub fn paint_draw(win: &Window, state: &PaintState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();

    // Toolbar: palette row + status
    let toolbar_h = 20u32;
    fill_rect(cx, cy, cw, toolbar_h, Color::rgb(0xE0, 0xE0, 0xE0));

    let pal_size = 14u32;
    for (i, &color) in PAINT_PALETTE.iter().enumerate() {
        let px = cx + 2 + i as u32 * (pal_size + 2);
        let py = cy + 3;
        fill_rect(px, py, pal_size, pal_size, color);
        if i == state.current_color {
            // Selection border
            let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
            for dx in 0..pal_size {
                fb.put_pixel(px + dx, py.wrapping_sub(1), Color::rgb(0xFF, 0x00, 0x00));
                fb.put_pixel(px + dx, py + pal_size, Color::rgb(0xFF, 0x00, 0x00));
            }
            for dy in 0..pal_size {
                fb.put_pixel(px.wrapping_sub(1), py + dy, Color::rgb(0xFF, 0x00, 0x00));
                fb.put_pixel(px + pal_size, py + dy, Color::rgb(0xFF, 0x00, 0x00));
            }
        }
    }

    let mode = if state.eraser { "ERZ" } else { "PEN" };
    draw_text(cx + cw - 40, cy + 3, mode, Color::BLACK);

    // Canvas
    let canvas_y = cy + toolbar_h + 2;
    for row in 0..PAINT_ROWS {
        for col in 0..PAINT_COLS {
            let idx = row * PAINT_COLS + col;
            let color = PAINT_PALETTE[state.canvas[idx] as usize % 16];
            let px = cx + col as u32 * PAINT_SCALE;
            let py = canvas_y + row as u32 * PAINT_SCALE;
            fill_rect(px, py, PAINT_SCALE, PAINT_SCALE, color);
        }
    }
}

pub fn paint_input(state: &mut PaintState, event: &KeyEvent) -> InputResult {
    match event.ascii {
        b'c' | b'C' => {
            for p in state.canvas.iter_mut() { *p = 1; }
            InputResult::Redraw
        }
        b'e' | b'E' => {
            state.eraser = !state.eraser;
            InputResult::Redraw
        }
        b'1'..=b'9' => {
            let idx = (event.ascii - b'1') as usize;
            if idx < PAINT_PALETTE.len() {
                state.current_color = idx;
            }
            InputResult::Redraw
        }
        b'0' => {
            state.current_color = 0;
            InputResult::Redraw
        }
        _ => {
            match event.key {
                KeyCode::Left => {
                    if state.current_color > 0 { state.current_color -= 1; }
                    InputResult::Redraw
                }
                KeyCode::Right => {
                    if state.current_color + 1 < PAINT_PALETTE.len() { state.current_color += 1; }
                    InputResult::Redraw
                }
                _ => InputResult::Continue,
            }
        }
    }
}

pub fn paint_click(state: &mut PaintState, local_x: u32, local_y: u32) -> InputResult {
    let toolbar_h = 22u32;

    // Palette click
    if local_y < 20 {
        let pal_size = 14u32;
        let idx = local_x / (pal_size + 2);
        if (idx as usize) < PAINT_PALETTE.len() {
            state.current_color = idx as usize;
        }
        return InputResult::Redraw;
    }

    // Canvas click
    if local_y >= toolbar_h {
        let canvas_y = local_y - toolbar_h;
        let col = (local_x / PAINT_SCALE) as usize;
        let row = (canvas_y / PAINT_SCALE) as usize;
        if col < PAINT_COLS && row < PAINT_ROWS {
            let idx = row * PAINT_COLS + col;
            if state.eraser {
                state.canvas[idx] = 1; // white
            } else {
                state.canvas[idx] = state.current_color as u8;
            }
        }
        return InputResult::Redraw;
    }

    InputResult::Continue
}
