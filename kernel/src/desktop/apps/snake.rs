// Snake Game Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::CHAR_HEIGHT;
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub const SNAKE_COLS: usize = 20;
pub const SNAKE_ROWS: usize = 18;
pub const SNAKE_CELL: u32 = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Up, Down, Left, Right,
}

pub struct SnakeState {
    body: Vec<(usize, usize)>, // (row, col)
    dir: Direction,
    food: (usize, usize),
    game_over: bool,
    score: u32,
    seed: u32,
}

impl SnakeState {
    pub fn new() -> Self {
        Self {
            body: Vec::new(),
            dir: Direction::Right,
            food: (0, 0),
            game_over: false,
            score: 0,
            seed: 0,
        }
    }
}

pub fn snake_init(state: &mut SnakeState) {
    state.body.clear();
    let mid_r = SNAKE_ROWS / 2;
    let mid_c = SNAKE_COLS / 2;
    state.body.push((mid_r, mid_c));
    state.body.push((mid_r, mid_c - 1));
    state.body.push((mid_r, mid_c - 2));
    state.dir = Direction::Right;
    state.game_over = false;
    state.score = 0;
    state.seed = crate::shell::ticks() as u32;
    snake_place_food(state);
}

fn snake_place_food(state: &mut SnakeState) {
    let total = SNAKE_COLS * SNAKE_ROWS;
    for _ in 0..total {
        state.seed = state.seed.wrapping_mul(1103515245).wrapping_add(12345);
        let r = ((state.seed >> 16) as usize) % SNAKE_ROWS;
        state.seed = state.seed.wrapping_mul(1103515245).wrapping_add(12345);
        let c = ((state.seed >> 16) as usize) % SNAKE_COLS;
        if !state.body.contains(&(r, c)) {
            state.food = (r, c);
            return;
        }
    }
}

fn snake_step(state: &mut SnakeState) {
    if state.game_over { return; }

    let (hr, hc) = state.body[0];
    let (nr, nc) = match state.dir {
        Direction::Up => (hr.wrapping_sub(1), hc),
        Direction::Down => (hr + 1, hc),
        Direction::Left => (hr, hc.wrapping_sub(1)),
        Direction::Right => (hr, hc + 1),
    };

    if nr >= SNAKE_ROWS || nc >= SNAKE_COLS {
        state.game_over = true;
        return;
    }

    if state.body.contains(&(nr, nc)) {
        state.game_over = true;
        return;
    }

    state.body.insert(0, (nr, nc));

    if (nr, nc) == state.food {
        state.score += 10;
        snake_place_food(state);
    } else {
        state.body.pop();
    }
}

pub fn snake_draw(win: &Window, state: &SnakeState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();

    // Header
    let mut header = String::new();
    if state.game_over {
        write!(header, "Game Over! Score: {}  R=restart", state.score).ok();
    } else {
        write!(header, "Score: {}  Arrows=move Space=step", state.score).ok();
    }
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 2, Color::rgb(0xE0, 0xE0, 0xE0));
    draw_text(cx + 4, cy + 1, &header, Color::BLACK);

    let grid_y = cy + CHAR_HEIGHT + 6;

    // Background grid
    for r in 0..SNAKE_ROWS {
        for c in 0..SNAKE_COLS {
            let px = cx + 4 + c as u32 * SNAKE_CELL;
            let py = grid_y + r as u32 * SNAKE_CELL;
            let bg = if (r + c) % 2 == 0 {
                Color::rgb(0xA0, 0xD0, 0xA0)
            } else {
                Color::rgb(0x90, 0xC0, 0x90)
            };
            fill_rect(px, py, SNAKE_CELL, SNAKE_CELL, bg);
        }
    }

    // Food
    let (fr, fc) = state.food;
    let fx = cx + 4 + fc as u32 * SNAKE_CELL + 2;
    let fy = grid_y + fr as u32 * SNAKE_CELL + 2;
    fill_rect(fx, fy, SNAKE_CELL - 4, SNAKE_CELL - 4, Color::rgb(0xFF, 0x00, 0x00));

    // Snake body
    for (i, &(r, c)) in state.body.iter().enumerate() {
        let px = cx + 4 + c as u32 * SNAKE_CELL + 1;
        let py = grid_y + r as u32 * SNAKE_CELL + 1;
        let color = if i == 0 {
            Color::rgb(0x00, 0x80, 0x00) // head
        } else {
            Color::rgb(0x00, 0xC0, 0x00) // body
        };
        fill_rect(px, py, SNAKE_CELL - 2, SNAKE_CELL - 2, color);
    }
}

pub fn snake_input(state: &mut SnakeState, event: &KeyEvent) -> InputResult {
    if state.game_over {
        if event.ascii == b'r' || event.ascii == b'R' {
            snake_init(state);
            return InputResult::Redraw;
        }
        return InputResult::Continue;
    }

    match event.key {
        KeyCode::Up    => { if state.dir != Direction::Down  { state.dir = Direction::Up; } }
        KeyCode::Down  => { if state.dir != Direction::Up    { state.dir = Direction::Down; } }
        KeyCode::Left  => { if state.dir != Direction::Right { state.dir = Direction::Left; } }
        KeyCode::Right => { if state.dir != Direction::Left  { state.dir = Direction::Right; } }
        KeyCode::Space => {
            snake_step(state);
            return InputResult::Redraw;
        }
        _ => return InputResult::Continue,
    }

    // Also step on each direction key
    snake_step(state);
    InputResult::Redraw
}
