// Task Manager Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, TITLE_ACTIVE, draw_text, draw_text_bg, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub struct TaskMgrState {
    pub(super) tasks: Vec<(u32, &'static str, String, u64, &'static str, u64)>,
    pub(super) selected: usize,
    pub(super) scroll: usize,
}

impl TaskMgrState {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }
}

pub(super) fn taskmgr_init(state: &mut TaskMgrState) {
    taskmgr_refresh(state);
}

pub(super) fn taskmgr_refresh(state: &mut TaskMgrState) {
    use crate::core_kernel::scheduler;
    let task_list = scheduler::task_list();
    state.tasks.clear();

    for (id, task_state, name, switches, priority, cpu_ticks) in &task_list {
        let state_str = match task_state {
            scheduler::TaskState::Empty => continue,
            scheduler::TaskState::Ready => "Ready",
            scheduler::TaskState::Running => "Running",
            scheduler::TaskState::Blocked => "Blocked",
            scheduler::TaskState::Exited => "Exited",
        };
        let pri_str = priority.name();
        state.tasks.push((*id, state_str, name.clone(), *switches, pri_str, *cpu_ticks));
    }
}

pub(super) fn taskmgr_draw(win: &Window, state: &TaskMgrState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Header
    draw_text_bg(cx, cy, "  ID  State    Name                  Sw     Pri     ", Color::WHITE, TITLE_ACTIVE);

    let visible_lines = ((ch - CHAR_HEIGHT - 4) / CHAR_HEIGHT) as usize;

    for (i, task) in state.tasks.iter().skip(state.scroll).take(visible_lines).enumerate() {
        let y = cy + (i as u32 + 1) * CHAR_HEIGHT + 2;
        let row_idx = state.scroll + i;
        let selected = row_idx == state.selected;

        let mut line = String::new();
        write!(line, "  {:3} {:8} {:20} {:6} {:8}",
            task.0, task.1, task.2, task.3, task.4
        ).ok();

        let max_chars = (cw / CHAR_WIDTH) as usize;
        let display: &str = if line.len() > max_chars { &line[..max_chars] } else { &line };

        if selected {
            let fill_w = cw.min(display.len() as u32 * CHAR_WIDTH + 4);
            fill_rect(cx, y, fill_w, CHAR_HEIGHT, TITLE_ACTIVE);
            draw_text(cx, y, display, Color::WHITE);
        } else {
            draw_text(cx, y, display, Color::BLACK);
        }
    }

    // Footer
    let footer_y = cy + ch - CHAR_HEIGHT;
    draw_text(cx, footer_y, "[R]efresh [K]ill  [Up/Down] Navigate", Color::rgb(0x80, 0x80, 0x80));
}

pub(super) fn taskmgr_input(state: &mut TaskMgrState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
                if state.selected < state.scroll {
                    state.scroll = state.selected;
                }
            }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.selected + 1 < state.tasks.len() {
                state.selected += 1;
            }
            InputResult::Redraw
        }
        _ => {
            // Check ASCII for R and K
            match event.ascii {
                b'r' | b'R' => {
                    taskmgr_refresh(state);
                    InputResult::Redraw
                }
                b'k' | b'K' => {
                    if let Some(task) = state.tasks.get(state.selected) {
                        let task_id = task.0;
                        if task_id > 0 {
                            crate::core_kernel::scheduler::kill(task_id);
                            taskmgr_refresh(state);
                        }
                    }
                    InputResult::Redraw
                }
                _ => InputResult::Continue,
            }
        }
    }
}
