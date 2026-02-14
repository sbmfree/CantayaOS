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

use super::{
    InputResult,
    fill_rect,
};
use super::wm::Window;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{KeyCode, KeyEvent};

pub mod sysinfo;
pub mod taskmgr;
pub mod notepad;
pub mod calculator;
pub mod about;
pub mod terminal;
pub mod filebrowser;
pub mod paint;
pub mod minesweeper;
pub mod snake;
pub mod settings;
pub mod clock;

pub use sysinfo::SysInfoState;
pub use taskmgr::TaskMgrState;
pub use notepad::NotepadState;
pub use calculator::CalcState;
pub use about::AboutState;
pub use terminal::TerminalState;
pub use filebrowser::FileBrowserState;
pub use paint::PaintState;
pub use minesweeper::MinesweeperState;
pub use snake::SnakeState;
pub use settings::SettingsState;
pub use clock::ClockState;

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
        AppState::SystemInfo(state) => sysinfo::sysinfo_init(state),
        AppState::TaskManager(state) => taskmgr::taskmgr_init(state),
        AppState::Notepad(_) => {}
        AppState::Calculator(_) => {}
        AppState::About(_) => {}
        AppState::Terminal(state) => terminal::terminal_init(state),
        AppState::FileBrowser(state) => filebrowser::filebrowser_init(state),
        AppState::Paint(_) => {}
        AppState::Minesweeper(state) => minesweeper::minesweeper_init(state),
        AppState::Snake(state) => snake::snake_init(state),
        AppState::Settings(state) => settings::settings_init(state),
        AppState::Clock(_) => {}
    }
}

/// Draw the application content in the window's client area.
pub fn draw_app(win: &Window) {
    match &win.app_state {
        AppState::SystemInfo(state) => sysinfo::sysinfo_draw(win, state),
        AppState::TaskManager(state) => taskmgr::taskmgr_draw(win, state),
        AppState::Notepad(state) => notepad::notepad_draw(win, state),
        AppState::Calculator(state) => calculator::calc_draw(win, state),
        AppState::About(state) => about::about_draw(win, state),
        AppState::Terminal(state) => terminal::terminal_draw(win, state),
        AppState::FileBrowser(state) => filebrowser::filebrowser_draw(win, state),
        AppState::Paint(state) => paint::paint_draw(win, state),
        AppState::Minesweeper(state) => minesweeper::minesweeper_draw(win, state),
        AppState::Snake(state) => snake::snake_draw(win, state),
        AppState::Settings(state) => settings::settings_draw(win, state),
        AppState::Clock(state) => clock::clock_draw(win, state),
    }
}

/// Handle keyboard input for the focused application.
pub fn handle_app_input(win: &mut Window, event: &KeyEvent) -> InputResult {
    match &mut win.app_state {
        AppState::SystemInfo(state) => sysinfo::sysinfo_input(state, event),
        AppState::TaskManager(state) => taskmgr::taskmgr_input(state, event),
        AppState::Notepad(state) => notepad::notepad_input(state, event),
        AppState::Calculator(state) => calculator::calc_input(state, event),
        AppState::About(_) => InputResult::Continue,
        AppState::Terminal(state) => terminal::terminal_input(state, event),
        AppState::FileBrowser(state) => filebrowser::filebrowser_input(state, event),
        AppState::Paint(state) => paint::paint_input(state, event),
        AppState::Minesweeper(state) => minesweeper::minesweeper_input(state, event),
        AppState::Snake(state) => snake::snake_input(state, event),
        AppState::Settings(state) => settings::settings_input(state, event),
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
        AppState::Paint(state) => paint::paint_click(state, local_x, local_y),
        AppState::Minesweeper(state) => minesweeper::minesweeper_click(state, local_x, local_y),
        AppState::Settings(state) => settings::settings_click(state, local_y),
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
    calculator::calc_input(state, &event)
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
            filebrowser::filebrowser_input(state, &event)
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
