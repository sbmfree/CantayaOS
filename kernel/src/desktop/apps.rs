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
    Paint,
    Minesweeper,
    Snake,
    Settings,
    Clock,
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
    Paint(PaintState),
    Minesweeper(MinesweeperState),
    Snake(SnakeState),
    Settings(SettingsState),
    Clock(ClockState),
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
            AppId::Paint       => AppState::Paint(PaintState::new()),
            AppId::Minesweeper => AppState::Minesweeper(MinesweeperState::new()),
            AppId::Snake       => AppState::Snake(SnakeState::new()),
            AppId::Settings    => AppState::Settings(SettingsState::new()),
            AppId::Clock       => AppState::Clock(ClockState::new()),
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
        AppState::Paint(_) => {}
        AppState::Minesweeper(state) => minesweeper_init(state),
        AppState::Snake(state) => snake_init(state),
        AppState::Settings(state) => settings_init(state),
        AppState::Clock(_) => {}
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
        AppState::Paint(state) => paint_draw(win, state),
        AppState::Minesweeper(state) => minesweeper_draw(win, state),
        AppState::Snake(state) => snake_draw(win, state),
        AppState::Settings(state) => settings_draw(win, state),
        AppState::Clock(state) => clock_draw(win, state),
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
        AppState::Paint(state) => paint_input(state, event),
        AppState::Minesweeper(state) => minesweeper_input(state, event),
        AppState::Snake(state) => snake_input(state, event),
        AppState::Settings(state) => settings_input(state, event),
        AppState::Clock(_) => InputResult::Continue,
    }
}

/// Handle a mouse click inside a window's client area.
/// `local_x` and `local_y` are relative to the client area origin.
pub fn handle_app_click(win: &mut Window, local_x: u32, local_y: u32) -> InputResult {
    match &mut win.app_state {
        AppState::Calculator(state) => calc_click(state, local_x, local_y),
        AppState::FileBrowser(state) => filebrowser_click(state, local_y),
        AppState::TaskManager(state) => taskmgr_click(state, local_y),
        AppState::SystemInfo(state) => sysinfo_click(state, local_y),
        AppState::Paint(state) => paint_click(state, local_x, local_y),
        AppState::Minesweeper(state) => minesweeper_click(state, local_x, local_y),
        AppState::Settings(state) => settings_click(state, local_y),
        _ => InputResult::Redraw,
    }
}

/// Calculator: handle click on a button grid.
fn calc_click(state: &mut CalcState, local_x: u32, local_y: u32) -> InputResult {
    // Calculator layout: 4 columns of buttons starting below display area
    let display_h: u32 = CHAR_HEIGHT + 12;
    if local_y < display_h { return InputResult::Continue; }

    let btn_w: u32 = 40;
    let btn_h: u32 = 24;
    let gap: u32 = 4;
    let col = local_x / (btn_w + gap);
    let row = (local_y - display_h) / (btn_h + gap);

    // Map (row, col) to calculator key presses
    let (key, ascii): (KeyCode, u8) = match (row, col) {
        (0, 0) => (KeyCode::Key7, b'7'),
        (0, 1) => (KeyCode::Key8, b'8'),
        (0, 2) => (KeyCode::Key9, b'9'),
        (0, 3) => (KeyCode::Slash, b'/'),
        (1, 0) => (KeyCode::Key4, b'4'),
        (1, 1) => (KeyCode::Key5, b'5'),
        (1, 2) => (KeyCode::Key6, b'6'),
        (1, 3) => (KeyCode::Key8, b'*'),  // shift+8 = *
        (2, 0) => (KeyCode::Key1, b'1'),
        (2, 1) => (KeyCode::Key2, b'2'),
        (2, 2) => (KeyCode::Key3, b'3'),
        (2, 3) => (KeyCode::Minus, b'-'),
        (3, 0) => (KeyCode::Key0, b'0'),
        (3, 1) => (KeyCode::Period, b'.'),
        (3, 2) => (KeyCode::Enter, b'\r'),
        (3, 3) => (KeyCode::Equals, b'+'),
        (4, 0) => (KeyCode::C, b'c'),
        _ => return InputResult::Continue,
    };

    let event = KeyEvent { key, ascii, pressed: true };
    calc_input(state, &event)
}

/// FileBrowser: handle click on a file row.
fn filebrowser_click(state: &mut FileBrowserState, local_y: u32) -> InputResult {
    // Each row is CHAR_HEIGHT tall, starts after the header area
    let header_h: u32 = CHAR_HEIGHT + 4;
    if local_y < header_h { return InputResult::Continue; }
    let row = ((local_y - header_h) / CHAR_HEIGHT) as usize + state.scroll;
    if row < state.entries.len() {
        if state.selected == row {
            // Already selected — simulate Enter to navigate
            let event = KeyEvent { key: KeyCode::Enter, ascii: b'\r', pressed: true };
            filebrowser_input(state, &event)
        } else {
            state.selected = row;
            InputResult::Redraw
        }
    } else {
        InputResult::Continue
    }
}

/// TaskManager: handle click on a task row.
fn taskmgr_click(state: &mut TaskMgrState, local_y: u32) -> InputResult {
    let header_h: u32 = CHAR_HEIGHT + 4;
    if local_y < header_h { return InputResult::Continue; }
    let row = ((local_y - header_h) / CHAR_HEIGHT) as usize + state.scroll;
    if row < state.tasks.len() {
        state.selected = row;
        InputResult::Redraw
    } else {
        InputResult::Continue
    }
}

/// SysInfo: handle click for scrolling.
fn sysinfo_click(state: &mut SysInfoState, local_y: u32) -> InputResult {
    // Click top half = scroll up, bottom half = scroll down
    let mid = 100u32;
    if local_y < mid {
        if state.scroll > 0 { state.scroll -= 1; }
    } else {
        state.scroll += 1;
    }
    InputResult::Redraw
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

// ============================================================================
// 8. Paint — Simple Pixel Drawing App
// ============================================================================

pub struct PaintState {
    /// Canvas pixels stored as flat array (palette indices).
    /// Canvas is PAINT_COLS x PAINT_ROWS, each "pixel" is a 4×4 block.
    canvas: Vec<u8>,
    /// Current drawing colour (palette index).
    current_color: usize,
    /// Whether eraser mode is on.
    eraser: bool,
}

const PAINT_COLS: usize = 100;
const PAINT_ROWS: usize = 70;
const PAINT_SCALE: u32 = 4;

const PAINT_PALETTE: [Color; 16] = [
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

// ============================================================================
// 9. Minesweeper
// ============================================================================

const MINE_ROWS: usize = 12;
const MINE_COLS: usize = 12;
const MINE_COUNT: usize = 20;
const CELL_SIZE: u32 = 22;

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
                    fill_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1, super::BUTTON_FACE);
                    super::draw_raised_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1);
                }
                CellState::Flagged => {
                    fill_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1, super::BUTTON_FACE);
                    super::draw_raised_rect(px, py, CELL_SIZE - 1, CELL_SIZE - 1);
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

// ============================================================================
// 10. Snake
// ============================================================================

const SNAKE_COLS: usize = 20;
const SNAKE_ROWS: usize = 18;
const SNAKE_CELL: u32 = 16;

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

// ============================================================================
// 11. Settings
// ============================================================================

pub struct SettingsState {
    selected: usize,
    items: Vec<SettingsItem>,
}

struct SettingsItem {
    label: String,
    kind: SettingsKind,
}

enum SettingsKind {
    Toggle(bool),
    Value(String),
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            items: Vec::new(),
        }
    }
}

pub fn settings_init(state: &mut SettingsState) {
    state.items.clear();

    // Read current theme from disk
    let theme = if let Some(data) = crate::storage::vfs::read_file("/system/theme.cfg") {
        String::from(core::str::from_utf8(&data).unwrap_or("default").trim())
    } else {
        String::from("default")
    };
    state.items.push(SettingsItem {
        label: String::from("Console Theme"),
        kind: SettingsKind::Value(String::from(theme)),
    });

    // Memory info
    let free = crate::memory::frame_allocator::free_frame_count();
    let total = crate::memory::frame_allocator::total_frame_count();
    let mut mem_str = String::new();
    write!(mem_str, "{}/{} KiB", (total - free) * 4, total * 4).ok();
    state.items.push(SettingsItem {
        label: String::from("Memory Usage"),
        kind: SettingsKind::Value(mem_str),
    });

    // Uptime
    let ticks = crate::shell::ticks();
    let ms = crate::hal::pit::ticks_to_ms(ticks);
    let secs = ms / 1000;
    let mut up_str = String::new();
    write!(up_str, "{}s", secs).ok();
    state.items.push(SettingsItem {
        label: String::from("Uptime"),
        kind: SettingsKind::Value(up_str),
    });

    // Storage
    let disk_ready = crate::storage::vfs::is_ready();
    state.items.push(SettingsItem {
        label: String::from("Disk Mounted"),
        kind: SettingsKind::Toggle(disk_ready),
    });

    state.items.push(SettingsItem {
        label: String::from("Screen Resolution"),
        kind: SettingsKind::Value(String::from("1920x1080")),
    });

    state.items.push(SettingsItem {
        label: String::from("OS Version"),
        kind: SettingsKind::Value(String::from(env!("CARGO_PKG_VERSION"))),
    });
}

pub fn settings_draw(win: &Window, state: &SettingsState) {
    let cx = win.client_x();
    let cy = win.client_y();
    let cw = win.client_width();
    let ch = win.client_height();

    // Title bar
    fill_rect(cx, cy, cw, CHAR_HEIGHT + 4, super::TITLE_ACTIVE);
    draw_text(cx + 8, cy + 2, "  System Settings", Color::WHITE);

    let row_h = CHAR_HEIGHT + 8;
    let start_y = cy + CHAR_HEIGHT + 8;

    for (i, item) in state.items.iter().enumerate() {
        let y = start_y + i as u32 * row_h;
        if y + row_h > cy + ch { break; }

        let selected = i == state.selected;
        if selected {
            fill_rect(cx, y, cw, row_h, Color::rgb(0xD0, 0xD0, 0xFF));
        }

        draw_text(cx + 8, y + 4, &item.label, Color::BLACK);

        let val_str = match &item.kind {
            SettingsKind::Toggle(v) => if *v { "[ON]" } else { "[OFF]" },
            SettingsKind::Value(s) => s.as_str(),
        };

        // Right-align value
        let val_x = cx + cw - 8 - val_str.len() as u32 * CHAR_WIDTH;
        draw_text(val_x, y + 4, val_str, Color::rgb(0x00, 0x00, 0x80));
    }

    // Footer
    let footer_y = cy + ch - CHAR_HEIGHT - 4;
    draw_text(cx + 4, footer_y, "[Up/Down] Navigate  [R] Refresh", Color::rgb(0x80, 0x80, 0x80));
}

pub fn settings_input(state: &mut SettingsState, event: &KeyEvent) -> InputResult {
    match event.key {
        KeyCode::Up => {
            if state.selected > 0 { state.selected -= 1; }
            InputResult::Redraw
        }
        KeyCode::Down => {
            if state.selected + 1 < state.items.len() { state.selected += 1; }
            InputResult::Redraw
        }
        _ => {
            if event.ascii == b'r' || event.ascii == b'R' {
                settings_init(state);
                InputResult::Redraw
            } else {
                InputResult::Continue
            }
        }
    }
}

pub fn settings_click(state: &mut SettingsState, local_y: u32) -> InputResult {
    let header_h = CHAR_HEIGHT + 8;
    if local_y < header_h { return InputResult::Continue; }
    let row_h = CHAR_HEIGHT + 8;
    let row = ((local_y - header_h) / row_h) as usize;
    if row < state.items.len() {
        state.selected = row;
        InputResult::Redraw
    } else {
        InputResult::Continue
    }
}

// ============================================================================
// 12. Clock — Analog/Digital Clock
// ============================================================================

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
