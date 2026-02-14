// Terminal (Mini Shell) Application

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::desktop::{InputResult, draw_text, fill_rect};
use crate::desktop::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::KeyEvent;

const TERM_MAX_LINES: usize = 100;
const TERM_MAX_INPUT: usize = 120;

pub struct TerminalState {
    output_lines: Vec<String>,
    input_buf: String,
    scroll: usize,
}

impl TerminalState {
    pub fn new() -> Self {
        Self {
            output_lines: Vec::new(),
            input_buf: String::new(),
            scroll: 0,
        }
    }
}

pub(super) fn terminal_init(state: &mut TerminalState) {
    state.output_lines.push(String::from("CantayaOS Terminal"));
    state.output_lines.push(String::from("Type commands (help, mem, cpu, uptime, tasks, date, clear)"));
    state.output_lines.push(String::from(""));
    terminal_prompt(state);
}

fn terminal_prompt(state: &mut TerminalState) {
    state.output_lines.push(String::from("C:\\> "));
}

pub(super) fn terminal_draw(win: &Window, state: &TerminalState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Black background for terminal
    fill_rect(cx, cy, cw, ch, Color::BLACK);

    let visible_lines = (ch / CHAR_HEIGHT) as usize;
    let max_cols = (cw / CHAR_WIDTH) as usize;

    // We need to show lines plus the current input line
    let total_lines = state.output_lines.len();
    let start = if total_lines > visible_lines {
        total_lines - visible_lines
    } else {
        0
    };

    for (i, line) in state.output_lines.iter().skip(start).enumerate() {
        let y = cy + i as u32 * CHAR_HEIGHT;
        if y + CHAR_HEIGHT > cy + ch { break; }

        // If this is the last line (the prompt line), append the input buffer
        let is_prompt = i + start == total_lines - 1;
        if is_prompt {
            let mut display = line.clone();
            display.push_str(&state.input_buf);
            display.push('_'); // cursor
            let disp: &str = if display.len() > max_cols { &display[..max_cols] } else { &display };
            draw_text(cx, y, disp, Color::rgb(0xAA, 0xFF, 0xAA));
        } else {
            let display: &str = if line.len() > max_cols { &line[..max_cols] } else { line };
            draw_text(cx, y, display, Color::rgb(0xAA, 0xAA, 0xAA));
        }
    }
}

pub(super) fn terminal_input(state: &mut TerminalState, event: &KeyEvent) -> InputResult {
    match event.ascii {
        b'\n' => {
            // Execute the command
            let cmd = state.input_buf.clone();

            // Append input to the prompt line
            if let Some(last) = state.output_lines.last_mut() {
                last.push_str(&cmd);
            }

            state.input_buf.clear();

            // Execute and capture output
            terminal_execute(state, &cmd);

            // New prompt
            terminal_prompt(state);

            // Trim old lines
            while state.output_lines.len() > TERM_MAX_LINES {
                state.output_lines.remove(0);
            }

            InputResult::Redraw
        }
        0x08 => {
            // Backspace
            state.input_buf.pop();
            InputResult::Redraw
        }
        c if c >= 0x20 && c < 0x7F => {
            if state.input_buf.len() < TERM_MAX_INPUT {
                state.input_buf.push(c as char);
            }
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}

fn terminal_execute(state: &mut TerminalState, cmd: &str) {
    let cmd = cmd.trim();
    if cmd.is_empty() { return; }

    let (base_cmd, _args) = match cmd.find(' ') {
        Some(pos) => (&cmd[..pos], cmd[pos + 1..].trim()),
        None => (cmd, ""),
    };

    match base_cmd {
        "help" => {
            state.output_lines.push(String::from("Commands: help, mem, cpu, uptime, tasks, date, clear, ver"));
        }
        "mem" => {
            let free = crate::memory::frame_allocator::free_frame_count();
            let total = crate::memory::frame_allocator::total_frame_count();
            let used = total - free;
            let mut s = String::new();
            write!(s, "Memory: {} KiB used / {} KiB total ({} KiB free)", used * 4, total * 4, free * 4).ok();
            state.output_lines.push(s);
        }
        "cpu" => {
            state.output_lines.push(String::from("CPU: x86_64 (AMD64)"));
            let mut s = String::new();
            write!(s, "CR0={:#010X} CR4={:#010X}", crate::hal::cpu::read_cr0(), crate::hal::cpu::read_cr4()).ok();
            state.output_lines.push(s);
        }
        "uptime" => {
            let ticks = crate::shell::ticks();
            let ms = crate::hal::pit::ticks_to_ms(ticks);
            let secs = ms / 1000;
            let mut s = String::new();
            write!(s, "Uptime: {}m {}s ({} ticks)", secs / 60, secs % 60, ticks).ok();
            state.output_lines.push(s);
        }
        "tasks" => {
            let task_list = crate::core_kernel::scheduler::task_list();
            let mut s = String::new();
            write!(s, "{} tasks:", task_list.len()).ok();
            state.output_lines.push(s);
            for (id, ts, name, sw, pri, _cpu) in &task_list {
                let state_str = match ts {
                    crate::core_kernel::scheduler::TaskState::Empty => continue,
                    crate::core_kernel::scheduler::TaskState::Ready => "R",
                    crate::core_kernel::scheduler::TaskState::Running => "*",
                    crate::core_kernel::scheduler::TaskState::Blocked => "B",
                    crate::core_kernel::scheduler::TaskState::Exited => "X",
                };
                let mut s = String::new();
                write!(s, "  {:3} [{}] {:12} pri={} sw={}", id, state_str, name, pri.name(), sw).ok();
                state.output_lines.push(s);
            }
        }
        "date" => {
            let dt = crate::hal::rtc::read_datetime();
            let mut s = String::new();
            write!(s, "{:04}-{:02}-{:02} {:02}:{:02}:{:02}", dt.year, dt.month, dt.day, dt.hour, dt.minute, dt.second).ok();
            state.output_lines.push(s);
        }
        "ver" => {
            let mut s = String::new();
            write!(s, "CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
            state.output_lines.push(s);
        }
        "clear" => {
            state.output_lines.clear();
        }
        _ => {
            let mut s = String::new();
            write!(s, "Unknown command: '{}'", base_cmd).ok();
            state.output_lines.push(s);
        }
    }
    state.output_lines.push(String::new());
}
