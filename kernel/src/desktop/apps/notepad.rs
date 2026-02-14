// Notepad (Simple Text Editor) Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct NotepadState {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll: usize,
    filename: String,
    modified: bool,
    status_msg: String,
}

impl NotepadState {
    pub fn new() -> Self {
        Self {
            lines: alloc::vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll: 0,
            filename: String::from("/notepad.txt"),
            modified: false,
            status_msg: String::from("Ctrl+S:Save  Ctrl+O:Open  Ctrl+N:New"),
        }
    }
}

pub(super) fn notepad_draw(win: &Window, state: &NotepadState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Status bar at top
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 2, Color::rgb(0xE0, 0xE0, 0xE0));
    let mut status = String::new();
    write!(status, " {} {} L:{} C:{}", state.filename,
        if state.modified { "*" } else { "" },
        state.cursor_row + 1, state.cursor_col + 1
    ).ok();
    draw_text(cx + 2, cy + 1, &status, Color::BLACK);

    // Status message at bottom
    let status_y = cy + ch - CHAR_HEIGHT - 2;
    fill_rect(cx, status_y, cw, CHAR_HEIGHT + 2, Color::rgb(0xE0, 0xE0, 0xE0));
    draw_text(cx + 2, status_y + 1, &state.status_msg, Color::rgb(0x40, 0x40, 0x40));

    let content_y = cy + CHAR_HEIGHT + 4;
    let content_h = ch.saturating_sub(CHAR_HEIGHT * 2 + 8);
    let visible_lines = (content_h / CHAR_HEIGHT) as usize;
    let max_cols = (cw / CHAR_WIDTH) as usize;

    for (i, line) in state.lines.iter().skip(state.scroll).take(visible_lines).enumerate() {
        let y = content_y + i as u32 * CHAR_HEIGHT;
        let display: &str = if line.len() > max_cols { &line[..max_cols] } else { line };
        draw_text(cx, y, display, Color::BLACK);
    }

    // Draw cursor
    let cursor_screen_row = state.cursor_row.saturating_sub(state.scroll);
    if cursor_screen_row < visible_lines {
        let cursor_x = cx + state.cursor_col as u32 * CHAR_WIDTH;
        let cursor_y = content_y + cursor_screen_row as u32 * CHAR_HEIGHT;
        fill_rect(cursor_x, cursor_y, CHAR_WIDTH, CHAR_HEIGHT, Color::BLACK);

        if let Some(line) = state.lines.get(state.cursor_row) {
            if state.cursor_col < line.len() {
                let ch = &line[state.cursor_col..state.cursor_col + 1];
                draw_text(cursor_x, cursor_y, ch, Color::WHITE);
            }
        }
    }
}

pub(super) fn notepad_input(state: &mut NotepadState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Left => {
            if state.cursor_col > 0 {
                state.cursor_col -= 1;
            } else if state.cursor_row > 0 {
                state.cursor_row -= 1;
                state.cursor_col = state.lines[state.cursor_row].len();
            }
            return InputResult::Redraw;
        }
        KeyCode::Right => {
            let line_len = state.lines.get(state.cursor_row).map_or(0, |l| l.len());
            if state.cursor_col < line_len {
                state.cursor_col += 1;
            } else if state.cursor_row + 1 < state.lines.len() {
                state.cursor_row += 1;
                state.cursor_col = 0;
            }
            return InputResult::Redraw;
        }
        KeyCode::Up => {
            if state.cursor_row > 0 {
                state.cursor_row -= 1;
                let line_len = state.lines[state.cursor_row].len();
                if state.cursor_col > line_len {
                    state.cursor_col = line_len;
                }
                if state.cursor_row < state.scroll {
                    state.scroll = state.cursor_row;
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Down => {
            if state.cursor_row + 1 < state.lines.len() {
                state.cursor_row += 1;
                let line_len = state.lines[state.cursor_row].len();
                if state.cursor_col > line_len {
                    state.cursor_col = line_len;
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Home => {
            state.cursor_col = 0;
            return InputResult::Redraw;
        }
        KeyCode::End => {
            let line_len = state.lines.get(state.cursor_row).map_or(0, |l| l.len());
            state.cursor_col = line_len;
            return InputResult::Redraw;
        }
        KeyCode::Delete => {
            if let Some(line) = state.lines.get_mut(state.cursor_row) {
                if state.cursor_col < line.len() {
                    line.remove(state.cursor_col);
                    state.modified = true;
                } else if state.cursor_row + 1 < state.lines.len() {
                    let next = state.lines.remove(state.cursor_row + 1);
                    state.lines[state.cursor_row].push_str(&next);
                    state.modified = true;
                }
            }
            return InputResult::Redraw;
        }
        _ => {}
    }

    // Check ASCII
    match event.ascii {
        // Ctrl+S — save to VFS
        0x13 => {
            use crate::storage::vfs;
            if vfs::is_ready() {
                let mut content = String::new();
                for (i, line) in state.lines.iter().enumerate() {
                    content.push_str(line);
                    if i < state.lines.len() - 1 {
                        content.push('\n');
                    }
                }
                if vfs::write_file(&state.filename, content.as_bytes()) {
                    state.modified = false;
                    state.status_msg.clear();
                    write!(state.status_msg, "Saved {} bytes to {}", content.len(), state.filename).ok();
                } else {
                    state.status_msg.clear();
                    write!(state.status_msg, "ERROR: Failed to save!").ok();
                }
            } else {
                state.status_msg.clear();
                write!(state.status_msg, "No filesystem mounted!").ok();
            }
            InputResult::Redraw
        }
        // Ctrl+O — open from VFS
        0x0F => {
            use crate::storage::vfs;
            if vfs::is_ready() {
                if let Some(data) = vfs::read_file(&state.filename) {
                    let text = core::str::from_utf8(&data).unwrap_or("");
                    state.lines = if text.is_empty() {
                        alloc::vec![String::new()]
                    } else {
                        text.lines().map(|l| String::from(l)).collect()
                    };
                    if state.lines.is_empty() { state.lines.push(String::new()); }
                    state.cursor_row = 0;
                    state.cursor_col = 0;
                    state.scroll = 0;
                    state.modified = false;
                    state.status_msg.clear();
                    write!(state.status_msg, "Opened {}", state.filename).ok();
                } else {
                    state.status_msg.clear();
                    write!(state.status_msg, "File not found: {}", state.filename).ok();
                }
            } else {
                state.status_msg.clear();
                write!(state.status_msg, "No filesystem mounted!").ok();
            }
            InputResult::Redraw
        }
        // Ctrl+N — new file
        0x0E => {
            state.lines = alloc::vec![String::new()];
            state.cursor_row = 0;
            state.cursor_col = 0;
            state.scroll = 0;
            state.modified = false;
            state.filename = String::from("/notepad.txt");
            state.status_msg.clear();
            write!(state.status_msg, "New file").ok();
            InputResult::Redraw
        }
        b'\n' => {
            // Enter — split line at cursor
            let current_line = state.lines.get(state.cursor_row).cloned().unwrap_or_default();
            let (left, right) = current_line.split_at(state.cursor_col.min(current_line.len()));
            state.lines[state.cursor_row] = String::from(left);
            state.lines.insert(state.cursor_row + 1, String::from(right));
            state.cursor_row += 1;
            state.cursor_col = 0;
            state.modified = true;
            InputResult::Redraw
        }
        0x08 => {
            // Backspace
            if state.cursor_col > 0 {
                if let Some(line) = state.lines.get_mut(state.cursor_row) {
                    state.cursor_col -= 1;
                    if state.cursor_col < line.len() {
                        line.remove(state.cursor_col);
                    }
                }
            } else if state.cursor_row > 0 {
                // Join with previous line
                let current = state.lines.remove(state.cursor_row);
                state.cursor_row -= 1;
                state.cursor_col = state.lines[state.cursor_row].len();
                state.lines[state.cursor_row].push_str(&current);
            }
            state.modified = true;
            InputResult::Redraw
        }
        c if c >= 0x20 && c < 0x7F => {
            // Printable character
            if state.lines.len() <= state.cursor_row {
                state.lines.push(String::new());
            }
            let line = &mut state.lines[state.cursor_row];
            if state.cursor_col >= line.len() {
                line.push(c as char);
            } else {
                line.insert(state.cursor_col, c as char);
            }
            state.cursor_col += 1;
            state.modified = true;
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}
