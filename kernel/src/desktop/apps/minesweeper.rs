// Minesweeper Application

extern crate alloc;

use core::fmt::Write;

use crate::desktop::{
    InputResult, BUTTON_FACE,
    draw_text, draw_sunken_rect, draw_raised_rect, fill_rect,
};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub const MINE_ROWS: usize = 12;
pub const MINE_COLS: usize = 12;
pub const MINE_COUNT: usize = 20;
pub const CELL_SIZE: u32 = 22;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CellState {
    Hidden,
    Revealed,
    Flagged,
}

pub struct MinesweeperState {
    mines: [bool; MINE_ROWS * MINE_COLS],
    states: [CellState; MINE_ROWS * MINE_COLS],
    adjacent: [u8; MINE_ROWS * MINE_COLS],
    cursor_row: usize,
    cursor_col: usize,
    game_over: bool,
    won: bool,
}

impl MinesweeperState {
    pub fn new() -> Self {
        Self {
            mines: [false; MINE_ROWS * MINE_COLS],
            states: [CellState::Hidden; MINE_ROWS * MINE_COLS],
            adjacent: [0; MINE_ROWS * MINE_COLS],
            cursor_row: 0,
            cursor_col: 0,
            game_over: false,
            won: false,
        }
    }
}

pub fn minesweeper_init(state: &mut MinesweeperState) {
    // Place mines using a simple LCG seeded from PIT ticks
    let mut seed = crate::shell::ticks() as u32;
    let total = MINE_ROWS * MINE_COLS;

    for m in state.mines.iter_mut() { *m = false; }
    for s in state.states.iter_mut() { *s = CellState::Hidden; }
    state.game_over = false;
    state.won = false;
    state.cursor_row = 0;
    state.cursor_col = 0;

    let mut placed = 0usize;
    while placed < MINE_COUNT {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let idx = ((seed >> 16) as usize) % total;
        if !state.mines[idx] {
            state.mines[idx] = true;
            placed += 1;
        }
    }

    // Compute adjacency
    for r in 0..MINE_ROWS {
        for c in 0..MINE_COLS {
            let mut count = 0u8;
            for dr in [-1i32, 0, 1] {
                for dc in [-1i32, 0, 1] {
                    if dr == 0 && dc == 0 { continue; }
                    let nr = r as i32 + dr;
                    let nc = c as i32 + dc;
                    if nr >= 0 && nr < MINE_ROWS as i32 && nc >= 0 && nc < MINE_COLS as i32 {
                        if state.mines[nr as usize * MINE_COLS + nc as usize] {
                            count += 1;
                        }
                    }
                }
            }
            state.adjacent[r * MINE_COLS + c] = count;
        }
    }
}

fn minesweeper_reveal(state: &mut MinesweeperState, r: usize, c: usize) {
    if r >= MINE_ROWS || c >= MINE_COLS { return; }
    let idx = r * MINE_COLS + c;
    if state.states[idx] != CellState::Hidden { return; }

    state.states[idx] = CellState::Revealed;

    if state.mines[idx] {
        state.game_over = true;
        for i in 0..MINE_ROWS * MINE_COLS {
            if state.mines[i] { state.states[i] = CellState::Revealed; }
        }
        return;
    }

    if state.adjacent[idx] == 0 {
        for dr in [-1i32, 0, 1] {
            for dc in [-1i32, 0, 1] {
                if dr == 0 && dc == 0 { continue; }
                let nr = r as i32 + dr;
                let nc = c as i32 + dc;
                if nr >= 0 && nr < MINE_ROWS as i32 && nc >= 0 && nc < MINE_COLS as i32 {
                    minesweeper_reveal(state, nr as usize, nc as usize);
                }
            }
        }
    }
}

fn minesweeper_check_win(state: &mut MinesweeperState) {
    let mut all_revealed = true;
    for i in 0..MINE_ROWS * MINE_COLS {
        if !state.mines[i] && state.states[i] != CellState::Revealed {
            all_revealed = false;
            break;
        }
    }
    if all_revealed {
        state.won = true;
        state.game_over = true;
    }
}

pub fn minesweeper_draw(win: &Window, state: &MinesweeperState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();

    // Header
    let header_y = cy;
    let status = if state.won {
        "YOU WIN! Press R to restart"
    } else if state.game_over {
        "BOOM! Game Over. Press R"
    } else {
        "Arrows:move Space:reveal F:flag"
    };
    fill_rect(cx, header_y, cw, CHAR_HEIGHT + 4, Color::rgb(0xE0, 0xE0, 0xE0));
    draw_text(cx + 4, header_y + 2, status, Color::BLACK);

    let grid_y = cy + CHAR_HEIGHT + 8;

    for r in 0..MINE_ROWS {
        for c in 0..MINE_COLS {
            let idx = r * MINE_COLS + c;
            let px = cx + 4 + c as u32 * CELL_SIZE;
            let py = grid_y + r as u32 * CELL_SIZE;

            let is_cursor = r == state.cursor_row && c == state.cursor_col;

            match state.states[idx] {
                CellState::Hidden => {
                    fill_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1, BUTTON_FACE);
                    draw_raised_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1);
                }
                CellState::Flagged => {
                    fill_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1, BUTTON_FACE);
                    draw_raised_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1);
                    draw_text(px + 5, py + 3, "F", Color::rgb(0xFF, 0x00, 0x00));
                }
                CellState::Revealed => {
                    fill_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1, Color::rgb(0xE0, 0xE0, 0xE0));
                    draw_sunken_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1);

                    if state.mines[idx] {
                        draw_text(px + 5, py + 3, "*", Color::rgb(0xFF, 0x00, 0x00));
                    } else if state.adjacent[idx] > 0 {
                        let n = state.adjacent[idx];
                        let color = match n {
                            1 => Color::rgb(0x00, 0x00, 0xFF),
                            2 => Color::rgb(0x00, 0x80, 0x00),
                            3 => Color::rgb(0xFF, 0x00, 0x00),
                            4 => Color::rgb(0x00, 0x00, 0x80),
                            _ => Color::rgb(0x80, 0x00, 0x00),
                        };
                        let mut buf = [0u8; 1];
                        buf[0] = b'0' + n;
                        let s = core::str::from_utf8(&buf).unwrap_or("?");
                        draw_text(px + 7, py + 3, s, color);
                    }
                }
            }

            // Cursor highlight
            if is_cursor && !state.game_over {
                let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
                let cs = CELL_SIZE - 1;
                for d in 0..cs {
                    fb.put_pixel(px + d, py, Color::rgb(0xFF, 0xFF, 0x00));
                    fb.put_pixel(px + d, py + cs - 1, Color::rgb(0xFF, 0xFF, 0x00));
                    fb.put_pixel(px, py + d, Color::rgb(0xFF, 0xFF, 0x00));
                    fb.put_pixel(px + cs - 1, py + d, Color::rgb(0xFF, 0xFF, 0x00));
                }
            }
        }
    }
}

pub fn minesweeper_input(state: &mut MinesweeperState, event: &KeyEvent) -> InputResult {
    if state.game_over {
        if event.ascii == b'r' || event.ascii == b'R' {
            minesweeper_init(state);
            return InputResult::Redraw;
        }
        return InputResult::Continue;
    }

    match event.key {
        KeyCode::Up => {
            if state.cursor_row > 0 { state.cursor_row -= 1; }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.cursor_row + 1 < MINE_ROWS { state.cursor_row += 1; }
            InputResult::Redraw
        }
        KeyCode::Left => {
            if state.cursor_col > 0 { state.cursor_col -= 1; }
            InputResult::Redraw
        }
        KeyCode::Right => {
            if state.cursor_col + 1 < MINE_COLS { state.cursor_col += 1; }
            InputResult::Redraw
        }
        KeyCode::Space | KeyCode::Enter => {
            let idx = state.cursor_row * MINE_COLS + state.cursor_col;
            if state.states[idx] == CellState::Hidden {
                minesweeper_reveal(state, state.cursor_row, state.cursor_col);
                if !state.game_over {
                    minesweeper_check_win(state);
                }
            }
            InputResult::Redraw
        }
        _ => {
            if event.ascii == b'f' || event.ascii == b'F' {
                let idx = state.cursor_row * MINE_COLS + state.cursor_col;
                match state.states[idx] {
                    CellState::Hidden => state.states[idx] = CellState::Flagged,
                    CellState::Flagged => state.states[idx] = CellState::Hidden,
                    _ => {}
                }
                InputResult::Redraw
            } else {
                InputResult::Continue
            }
        }
    }
}

pub fn minesweeper_click(state: &mut MinesweeperState, local_x: u32, local_y: u32) -> InputResult {
    if state.game_over { return InputResult::Continue; }

    let header_h = CHAR_HEIGHT + 8;
    if local_y < header_h { return InputResult::Continue; }

    let grid_y = local_y - header_h;
    let col = ((local_x.saturating_sub(4)) / CELL_SIZE) as usize;
    let row = (grid_y / CELL_SIZE) as usize;

    if row < MINE_ROWS && col < MINE_COLS {
        state.cursor_row = row;
        state.cursor_col = col;
        let idx = row * MINE_COLS + col;
        if state.states[idx] == CellState::Hidden {
            minesweeper_reveal(state, row, col);
            if !state.game_over {
                minesweeper_check_win(state);
            }
        }
        InputResult::Redraw
    } else {
        InputResult::Continue
    }
}
