// Built-in Desktop Applications for CantayaOS
//
// Each application runs inside a Window and receives keyboard input.
// Applications are simple — they render text/graphics in their client area
// and handle keyboard events.
//
// Applications:
//   1. System Info  — displays CPU, memory, uptime, PCI devices
//   2. Task Manager — live task list with priority and CPU info
//   3. Notepad      — simple text editor (type + basic editing)
//   4. Calculator   — basic arithmetic calculator
//   5. About        — version info and credits
//   6. Terminal     — mini embedded terminal (subset of shell commands)

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use super::{
    InputResult, TITLE_ACTIVE,
    draw_text, draw_text_bg, draw_sunken_rect, fill_rect,
};
use super::wm::Window;
use crate::graphics::framebuffer::Color;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

// ============================================================================
// App Registry
// ============================================================================

/// Application identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppId {
    SystemInfo,
    TaskManager,
    Notepad,
    Calculator,
    About,
    Terminal,
    FileBrowser,
}

/// Application state — each app stores its own data here.
pub enum AppState {
    SystemInfo(SysInfoState),
    TaskManager(TaskMgrState),
    Notepad(NotepadState),
    Calculator(CalcState),
    About(AboutState),
    Terminal(TerminalState),
    FileBrowser(FileBrowserState),
}

impl AppState {
    pub fn new(app_id: AppId) -> Self {
        match app_id {
            AppId::SystemInfo  => AppState::SystemInfo(SysInfoState::new()),
            AppId::TaskManager => AppState::TaskManager(TaskMgrState::new()),
            AppId::Notepad     => AppState::Notepad(NotepadState::new()),
            AppId::Calculator  => AppState::Calculator(CalcState::new()),
            AppId::About       => AppState::About(AboutState::new()),
            AppId::Terminal    => AppState::Terminal(TerminalState::new()),
            AppId::FileBrowser => AppState::FileBrowser(FileBrowserState::new()),
        }
    }
}

// ============================================================================
// App Dispatch
// ============================================================================

/// Initialize an application (called once when the window opens).
pub fn init_app(win: &mut Window) {
    match &mut win.app_state {
        AppState::SystemInfo(state) => sysinfo_init(state),
        AppState::TaskManager(state) => taskmgr_init(state),
        AppState::Notepad(_) => {}
        AppState::Calculator(_) => {}
        AppState::About(_) => {}
        AppState::Terminal(state) => terminal_init(state),
        AppState::FileBrowser(state) => filebrowser_init(state),
    }
}

/// Draw the application content in the window's client area.
pub fn draw_app(win: &Window) {
    match &win.app_state {
        AppState::SystemInfo(state) => sysinfo_draw(win, state),
        AppState::TaskManager(state) => taskmgr_draw(win, state),
        AppState::Notepad(state) => notepad_draw(win, state),
        AppState::Calculator(state) => calc_draw(win, state),
        AppState::About(state) => about_draw(win, state),
        AppState::Terminal(state) => terminal_draw(win, state),
        AppState::FileBrowser(state) => filebrowser_draw(win, state),
    }
}

/// Handle keyboard input for the focused application.
pub fn handle_app_input(win: &mut Window, event: &KeyEvent) -> InputResult {
    match &mut win.app_state {
        AppState::SystemInfo(state) => sysinfo_input(state, event),
        AppState::TaskManager(state) => taskmgr_input(state, event),
        AppState::Notepad(state) => notepad_input(state, event),
        AppState::Calculator(state) => calc_input(state, event),
        AppState::About(_) => InputResult::Continue,
        AppState::Terminal(state) => terminal_input(state, event),
        AppState::FileBrowser(state) => filebrowser_input(state, event),
    }
}

// ============================================================================
// 1. System Information
// ============================================================================

pub struct SysInfoState {
    lines: Vec<String>,
    scroll: usize,
}

impl SysInfoState {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            scroll: 0,
        }
    }
}

fn sysinfo_init(state: &mut SysInfoState) {
    let mut lines = Vec::new();

    // Header
    lines.push(String::from("  === CantayaOS System Information ==="));
    lines.push(String::new());

    // Kernel version
    let mut s = String::new();
    write!(s, "  Kernel:     CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
    lines.push(s);
    lines.push(String::from("  Arch:       x86_64 (AMD64)"));
    lines.push(String::from("  Build:      Rust nightly, no_std"));
    lines.push(String::new());

    // Memory
    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;

    let mut s = String::new();
    write!(s, "  Total RAM:  {} KiB ({} frames)", total_frames * 4, total_frames).ok();
    lines.push(s);
    let mut s = String::new();
    write!(s, "  Used:       {} KiB ({} frames)", used_frames * 4, used_frames).ok();
    lines.push(s);
    let mut s = String::new();
    write!(s, "  Free:       {} KiB ({} frames)", free_frames * 4, free_frames).ok();
    lines.push(s);
    lines.push(String::new());

    // Uptime
    let ticks = crate::shell::ticks();
    let ms = crate::hal::pit::ticks_to_ms(ticks);
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    let mut s = String::new();
    write!(s, "  Uptime:     {}h {}m {}s", hours, mins % 60, secs % 60).ok();
    lines.push(s);
    lines.push(String::new());

    // CPU info
    lines.push(String::from("  CPU:        x86_64 (AMD64)"));
    let mut s = String::new();
    let cr0 = crate::hal::cpu::read_cr0();
    write!(s, "  CR0:        {:#010X}", cr0).ok();
    lines.push(s);
    let mut s = String::new();
    let cr4 = crate::hal::cpu::read_cr4();
    write!(s, "  CR4:        {:#010X}", cr4).ok();
    lines.push(s);
    lines.push(String::new());

    // PCI Devices
    let pci_count = crate::hal::pci::device_count();
    let mut s = String::new();
    write!(s, "  PCI Devices: {}", pci_count).ok();
    lines.push(s);

    let devices = crate::hal::pci::device_list();
    for dev in devices.iter().take(10) {
        let mut s = String::new();
        write!(s, "    {:02X}:{:02X}.{} {:04X}:{:04X} class={:02X}.{:02X}",
            dev.bus, dev.device, dev.function,
            dev.vendor_id, dev.device_id,
            dev.class_code, dev.subclass
        ).ok();
        lines.push(s);
    }

    lines.push(String::new());
    lines.push(String::from("  [Up/Down to scroll]"));

    state.lines = lines;
}

fn sysinfo_draw(win: &Window, state: &SysInfoState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    let visible_lines = (ch / CHAR_HEIGHT) as usize;

    for (i, line) in state.lines.iter().skip(state.scroll).take(visible_lines).enumerate() {
        let y = cy + i as u32 * CHAR_HEIGHT;
        let max_chars = (cw / CHAR_WIDTH) as usize;
        let display: &str = if line.len() > max_chars { &line[..max_chars] } else { line };
        draw_text(cx, y, display, Color::BLACK);
    }
}

fn sysinfo_input(state: &mut SysInfoState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.scroll > 0 {
                state.scroll -= 1;
            }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.scroll + 1 < state.lines.len() {
                state.scroll += 1;
            }
            InputResult::Redraw
        }
        KeyCode::Home => {
            state.scroll = 0;
            InputResult::Redraw
        }
        KeyCode::End => {
            if state.lines.len() > 5 {
                state.scroll = state.lines.len() - 5;
            }
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}

// ============================================================================
// 2. Task Manager
// ============================================================================

pub struct TaskMgrState {
    tasks: Vec<(u32, &'static str, String, u64, &'static str, u64)>,
    selected: usize,
    scroll: usize,
}

impl TaskMgrState {
    fn new() -> Self {
        Self {
            tasks: Vec::new(),
            selected: 0,
            scroll: 0,
        }
    }
}

fn taskmgr_init(state: &mut TaskMgrState) {
    taskmgr_refresh(state);
}

fn taskmgr_refresh(state: &mut TaskMgrState) {
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

fn taskmgr_draw(win: &Window, state: &TaskMgrState) {
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

fn taskmgr_input(state: &mut TaskMgrState, event: &KeyEvent) -> InputResult {
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

// ============================================================================
// 3. Notepad (Simple Text Editor)
// ============================================================================

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
    fn new() -> Self {
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

fn notepad_draw(win: &Window, state: &NotepadState) {
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

fn notepad_input(state: &mut NotepadState, event: &KeyEvent) -> InputResult {
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

// ============================================================================
// 4. Calculator
// ============================================================================

pub struct CalcState {
    display: String,
    accumulator: f64,
    current_input: String,
    operator: Option<char>,
    just_computed: bool,
}

impl CalcState {
    fn new() -> Self {
        Self {
            display: String::from("0"),
            accumulator: 0.0,
            current_input: String::new(),
            operator: None,
            just_computed: false,
        }
    }
}

fn calc_draw(win: &Window, state: &CalcState) {
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

            fill_rect(bx, by, bw, bh, super::BUTTON_FACE);
            super::draw_raised_rect(bx, by, bw, bh);

            let text_x = bx + (bw - CHAR_WIDTH) / 2;
            let text_y = by + (bh - CHAR_HEIGHT) / 2;
            draw_text(text_x, text_y, label, Color::BLACK);
        }
    }

    // Footer
    let footer_y = start_y + 5 * (btn_h + gap) + 4;
    draw_text(cx + 8, footer_y, "Type digits, +-*/, Enter=", Color::rgb(0x80, 0x80, 0x80));
}

fn calc_input(state: &mut CalcState, event: &KeyEvent) -> InputResult {
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

// ============================================================================
// 5. About CantayaOS
// ============================================================================

pub struct AboutState;

impl AboutState {
    fn new() -> Self { Self }
}

fn about_draw(win: &Window, _state: &AboutState) {
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
    super::draw_raised_rect(logo_x, logo_y, logo_w, 40);

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

// ============================================================================
// 6. Terminal (Mini Shell)
// ============================================================================

const TERM_MAX_LINES: usize = 100;
const TERM_MAX_INPUT: usize = 120;

pub struct TerminalState {
    output_lines: Vec<String>,
    input_buf: String,
    scroll: usize,
}

impl TerminalState {
    fn new() -> Self {
        Self {
            output_lines: Vec::new(),
            input_buf: String::new(),
            scroll: 0,
        }
    }
}

fn terminal_init(state: &mut TerminalState) {
    state.output_lines.push(String::from("CantayaOS Terminal"));
    state.output_lines.push(String::from("Type commands (help, mem, cpu, uptime, tasks, date, clear)"));
    state.output_lines.push(String::from(""));
    terminal_prompt(state);
}

fn terminal_prompt(state: &mut TerminalState) {
    state.output_lines.push(String::from("C:\\> "));
}

fn terminal_draw(win: &Window, state: &TerminalState) {
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

fn terminal_input(state: &mut TerminalState, event: &KeyEvent) -> InputResult {
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

// ============================================================================
// 7. File Browser
// ============================================================================

pub struct FileBrowserState {
    current_path: String,
    entries: Vec<FileBrowserEntry>,
    selected: usize,
    scroll: usize,
    status_msg: String,
    preview: Vec<String>,
}

struct FileBrowserEntry {
    name: String,
    is_dir: bool,
    size: usize,
}

impl FileBrowserState {
    fn new() -> Self {
        Self {
            current_path: String::from("/"),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            status_msg: String::from("Enter:Open  Backspace:Up  D:Delete  R:Refresh"),
            preview: Vec::new(),
        }
    }
}

fn filebrowser_init(state: &mut FileBrowserState) {
    filebrowser_refresh(state);
}

fn filebrowser_refresh(state: &mut FileBrowserState) {
    use crate::storage::vfs;
    state.entries.clear();
    state.preview.clear();

    if !vfs::is_ready() {
        state.status_msg = String::from("No filesystem mounted!");
        return;
    }

    // Add parent directory entry if not root
    if state.current_path != "/" {
        state.entries.push(FileBrowserEntry {
            name: String::from(".."),
            is_dir: true,
            size: 0,
        });
    }

    if let Some(dir_entries) = vfs::list_dir(&state.current_path) {
        // Sort: directories first, then files
        let mut dirs: Vec<_> = dir_entries.iter().filter(|e| e.is_dir).collect();
        let mut files: Vec<_> = dir_entries.iter().filter(|e| !e.is_dir).collect();
        dirs.sort_by(|a, b| a.name.cmp(&b.name));
        files.sort_by(|a, b| a.name.cmp(&b.name));

        for d in dirs {
            state.entries.push(FileBrowserEntry {
                name: d.name.clone(),
                is_dir: true,
                size: 0,
            });
        }
        for f in files {
            let full = if state.current_path == "/" {
                alloc::format!("/{}", f.name)
            } else {
                alloc::format!("{}/{}", state.current_path, f.name)
            };
            let size = vfs::read_file(&full).map(|d| d.len()).unwrap_or(0);
            state.entries.push(FileBrowserEntry {
                name: f.name.clone(),
                is_dir: false,
                size,
            });
        }
    }

    state.selected = 0;
    state.scroll = 0;
    state.status_msg.clear();
    write!(state.status_msg, "{} - {} items", state.current_path, state.entries.len()).ok();
}

fn filebrowser_draw(win: &Window, state: &FileBrowserState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Header — current path
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 2, Color::rgb(0x00, 0x00, 0x80));
    let mut header = String::new();
    write!(header, " {}", state.current_path).ok();
    draw_text(cx + 2, cy + 1, &header, Color::WHITE);

    // File list
    let list_y = cy + CHAR_HEIGHT + 4;
    let max_cols = (cw / CHAR_WIDTH) as usize;
    
    // Reserve space for preview and status
    let list_height = ch.saturating_sub(CHAR_HEIGHT * 2 + 10);
    let visible = (list_height / CHAR_HEIGHT) as usize;

    for (i, entry) in state.entries.iter().skip(state.scroll).take(visible).enumerate() {
        let y = list_y + i as u32 * CHAR_HEIGHT;
        let actual_idx = state.scroll + i;
        let is_selected = actual_idx == state.selected;

        if is_selected {
            fill_rect(cx, y, cw, CHAR_HEIGHT, Color::rgb(0x00, 0x00, 0x80));
        }

        let mut line = String::new();
        if entry.is_dir {
            write!(line, " [DIR]  {}/", entry.name).ok();
        } else {
            if entry.size < 1024 {
                write!(line, " {:>5}B  {}", entry.size, entry.name).ok();
            } else {
                write!(line, " {:>4}KB  {}", entry.size / 1024, entry.name).ok();
            }
        }

        if line.len() > max_cols { line.truncate(max_cols); }

        let fg = if is_selected {
            Color::WHITE
        } else if entry.is_dir {
            Color::rgb(0x00, 0x00, 0xAA)
        } else {
            Color::BLACK
        };

        draw_text(cx + 2, y, &line, fg);
    }

    // Status bar at bottom
    let status_y = cy + ch - CHAR_HEIGHT - 2;
    fill_rect(cx, status_y, cw, CHAR_HEIGHT + 2, Color::rgb(0xE0, 0xE0, 0xE0));
    draw_text(cx + 2, status_y + 1, &state.status_msg, Color::rgb(0x40, 0x40, 0x40));
}

fn filebrowser_input(state: &mut FileBrowserState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.selected > 0 {
                state.selected -= 1;
                if state.selected < state.scroll {
                    state.scroll = state.selected;
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Down => {
            if state.selected + 1 < state.entries.len() {
                state.selected += 1;
            }
            return InputResult::Redraw;
        }
        KeyCode::Enter => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.name == ".." {
                    // Go up
                    if let Some(pos) = state.current_path.rfind('/') {
                        if pos == 0 {
                            state.current_path = String::from("/");
                        } else {
                            state.current_path.truncate(pos);
                        }
                    }
                    filebrowser_refresh(state);
                } else if entry.is_dir {
                    // Enter directory
                    if state.current_path == "/" {
                        state.current_path = alloc::format!("/{}", entry.name);
                    } else {
                        let new_path = alloc::format!("{}/{}", state.current_path, entry.name);
                        state.current_path = new_path;
                    }
                    filebrowser_refresh(state);
                } else {
                    // Preview file content
                    use crate::storage::vfs;
                    let full = if state.current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", state.current_path, entry.name)
                    };
                    state.status_msg.clear();
                    if let Some(data) = vfs::read_file(&full) {
                        if let Ok(text) = core::str::from_utf8(&data) {
                            write!(state.status_msg, "{} - {} bytes (text)", entry.name, data.len()).ok();
                        } else {
                            write!(state.status_msg, "{} - {} bytes (binary)", entry.name, data.len()).ok();
                        }
                    }
                }
            }
            return InputResult::Redraw;
        }
        KeyCode::Backspace => {
            // Go up one directory
            if state.current_path != "/" {
                if let Some(pos) = state.current_path.rfind('/') {
                    if pos == 0 {
                        state.current_path = String::from("/");
                    } else {
                        state.current_path.truncate(pos);
                    }
                }
                filebrowser_refresh(state);
            }
            return InputResult::Redraw;
        }
        _ => {}
    }

    match event.ascii {
        // 'D' or 'd' — delete selected file
        b'D' | b'd' => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.name != ".." {
                    use crate::storage::vfs;
                    let full = if state.current_path == "/" {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", state.current_path, entry.name)
                    };
                    if vfs::delete(&full) {
                        state.status_msg.clear();
                        write!(state.status_msg, "Deleted: {}", entry.name).ok();
                        filebrowser_refresh(state);
                        if state.selected >= state.entries.len() && state.selected > 0 {
                            state.selected -= 1;
                        }
                    } else {
                        state.status_msg.clear();
                        write!(state.status_msg, "Failed to delete: {}", entry.name).ok();
                    }
                }
            }
            InputResult::Redraw
        }
        // 'R' or 'r' — refresh
        b'R' | b'r' => {
            filebrowser_refresh(state);
            InputResult::Redraw
        }
        _ => InputResult::Continue,
    }
}
