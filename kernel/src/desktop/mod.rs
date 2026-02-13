// CantayaOS Desktop Environment
//
// A graphical desktop environment with a window manager, taskbar, start menu,
// and built-in applications. Inspired by Windows 95/98/2000 aesthetics.
//
// Architecture:
//   Desktop (background + icons)
//     └── Window Manager (manages windows, focus, z-order)
//           ├── Taskbar (always on top, bottom of screen)
//           │     ├── Start Button
//           │     ├── Running App Buttons
//           │     └── Clock
//           ├── Start Menu (overlay when Start is active)
//           └── Application Windows
//
// Navigation is fully keyboard-driven:
//   - Tab/Shift+Tab: cycle through windows/icons
//   - Arrow keys: move within app / navigate menus
//   - Enter: activate / select
//   - Alt+F4 / Escape: close window / exit desktop
//   - F1: open Start menu

pub mod wm;
pub mod taskbar;
pub mod apps;

extern crate alloc;

use alloc::vec::Vec;

use crate::graphics::framebuffer::{Color, FRAMEBUFFER};
use crate::graphics::font::{self, CHAR_WIDTH, CHAR_HEIGHT};
use crate::hal::keyboard::{self, KeyCode, KeyEvent};
use crate::hal::mouse;

// ============================================================================
// Color Palette (Windows 95/2000 inspired)
// ============================================================================

pub const DESKTOP_BG: Color = Color::rgb(0, 128, 128);        // Classic teal
pub const TASKBAR_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);   // Silver
pub const TASKBAR_BORDER: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const START_BTN_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const START_BTN_TEXT: Color = Color::rgb(0, 0, 0);
pub const WINDOW_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);    // Window chrome
pub const WINDOW_CLIENT: Color = Color::rgb(0xFF, 0xFF, 0xFF); // Client area
pub const TITLE_ACTIVE: Color = Color::rgb(0x00, 0x00, 0x80);  // Active title bar
pub const TITLE_INACTIVE: Color = Color::rgb(0x80, 0x80, 0x80);
pub const TITLE_TEXT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const MENU_BG: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const MENU_HIGHLIGHT: Color = Color::rgb(0x00, 0x00, 0x80);
pub const MENU_TEXT: Color = Color::rgb(0, 0, 0);
pub const MENU_TEXT_HI: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const SHADOW: Color = Color::rgb(0x80, 0x80, 0x80);
pub const HIGHLIGHT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const ICON_TEXT: Color = Color::rgb(0xFF, 0xFF, 0xFF);
pub const CLOCK_TEXT: Color = Color::rgb(0, 0, 0);
pub const BUTTON_FACE: Color = Color::rgb(0xC0, 0xC0, 0xC0);
pub const BUTTON_SHADOW: Color = Color::rgb(0x80, 0x80, 0x80);
pub const BUTTON_HIGHLIGHT: Color = Color::rgb(0xFF, 0xFF, 0xFF);

/// Taskbar height in pixels
pub const TASKBAR_HEIGHT: u32 = 28;

/// Title bar height in pixels
pub const TITLEBAR_HEIGHT: u32 = 20;

/// Window border width
pub const BORDER_WIDTH: u32 = 3;

// ============================================================================
// Drawing Helpers
// ============================================================================

/// Draw a string directly onto the framebuffer at pixel coordinates.
pub fn draw_text(x: u32, y: u32, text: &str, color: Color) {
    let mut fb = FRAMEBUFFER.lock();
    for (i, c) in text.chars().enumerate() {
        let cx = x + i as u32 * CHAR_WIDTH;
        let bitmap = font::get_char_bitmap(c);
        for (dy, &row_bits) in bitmap.iter().enumerate() {
            for dx in 0..8u32 {
                if (row_bits >> (7 - dx)) & 1 != 0 {
                    fb.put_pixel(cx + dx, y + dy as u32, color);
                }
            }
        }
    }
}

/// Draw a string with background fill.
pub fn draw_text_bg(x: u32, y: u32, text: &str, fg: Color, bg: Color) {
    let mut fb = FRAMEBUFFER.lock();
    for (i, c) in text.chars().enumerate() {
        let cx = x + i as u32 * CHAR_WIDTH;
        let bitmap = font::get_char_bitmap(c);
        for (dy, &row_bits) in bitmap.iter().enumerate() {
            for dx in 0..8u32 {
                let pixel_set = (row_bits >> (7 - dx)) & 1 != 0;
                let color = if pixel_set { fg } else { bg };
                fb.put_pixel(cx + dx, y + dy as u32, color);
            }
        }
    }
}

/// Draw a 3D raised button/panel (Windows 95 style bevel).
pub fn draw_raised_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut fb = FRAMEBUFFER.lock();
    // Top and left edges = highlight (white)
    fb.fill_rect(x, y, w, 1, HIGHLIGHT);
    fb.fill_rect(x, y, 1, h, HIGHLIGHT);
    // Bottom and right edges = shadow (dark gray)
    fb.fill_rect(x, y + h - 1, w, 1, SHADOW);
    fb.fill_rect(x + w - 1, y, 1, h, SHADOW);
    // Inner shadow
    fb.fill_rect(x + 1, y + h - 2, w - 2, 1, BUTTON_SHADOW);
    fb.fill_rect(x + w - 2, y + 1, 1, h - 2, BUTTON_SHADOW);
}

/// Draw a 3D sunken panel (Windows 95 style inset).
pub fn draw_sunken_rect(x: u32, y: u32, w: u32, h: u32) {
    let mut fb = FRAMEBUFFER.lock();
    // Top and left = shadow
    fb.fill_rect(x, y, w, 1, SHADOW);
    fb.fill_rect(x, y, 1, h, SHADOW);
    // Bottom and right = highlight
    fb.fill_rect(x, y + h - 1, w, 1, HIGHLIGHT);
    fb.fill_rect(x + w - 1, y, 1, h, HIGHLIGHT);
}

/// Fill a rectangle (convenience wrapper).
pub fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: Color) {
    let mut fb = FRAMEBUFFER.lock();
    fb.fill_rect(x, y, w, h, color);
}

/// Draw a simple icon (16x16) from a bitmap pattern.
pub fn draw_icon_16(x: u32, y: u32, icon: &[u16; 16], fg: Color, bg_opt: Option<Color>) {
    let mut fb = FRAMEBUFFER.lock();
    for (row, &bits) in icon.iter().enumerate() {
        for col in 0..16u32 {
            if (bits >> (15 - col)) & 1 != 0 {
                fb.put_pixel(x + col, y + row as u32, fg);
            } else if let Some(bg) = bg_opt {
                fb.put_pixel(x + col, y + row as u32, bg);
            }
        }
    }
}

/// Present the framebuffer.
pub fn present() {
    let mut fb = FRAMEBUFFER.lock();
    fb.present();
}

/// Get screen dimensions.
pub fn screen_size() -> (u32, u32) {
    let fb = FRAMEBUFFER.lock();
    (fb.width, fb.height)
}

// ============================================================================
// Desktop Icons (16x16 bitmaps)
// ============================================================================

/// Computer icon
pub const ICON_COMPUTER: [u16; 16] = [
    0b0111111111111110,
    0b0100000000000010,
    0b0101010101010010,
    0b0100000000000010,
    0b0101010101010010,
    0b0100000000000010,
    0b0111111111111110,
    0b0111111111111110,
    0b0000011111100000,
    0b0000010000100000,
    0b0000010000100000,
    0b0001111111111000,
    0b0001000000001000,
    0b0001111111111000,
    0b0000000000000000,
    0b0000000000000000,
];

/// Notepad/text icon
pub const ICON_NOTEPAD: [u16; 16] = [
    0b0011111111110000,
    0b0010000000011000,
    0b0010000000010100,
    0b0010000000011110,
    0b0010111011100010,
    0b0010000000000010,
    0b0010111011100010,
    0b0010000000000010,
    0b0010111011100010,
    0b0010000000000010,
    0b0010111000000010,
    0b0010000000000010,
    0b0010111011100010,
    0b0010000000000010,
    0b0011111111111110,
    0b0000000000000000,
];

/// Calculator icon
pub const ICON_CALC: [u16; 16] = [
    0b0011111111111100,
    0b0010000000000100,
    0b0010111111110100,
    0b0010111111110100,
    0b0010000000000100,
    0b0010110011001100,
    0b0010000000000100,
    0b0010110011001100,
    0b0010000000000100,
    0b0010110011001100,
    0b0010000000000100,
    0b0010110011111100,
    0b0010000000000100,
    0b0010110000000100,
    0b0010000000000100,
    0b0011111111111100,
];

/// Info/About icon
pub const ICON_INFO: [u16; 16] = [
    0b0000011111100000,
    0b0001100000011000,
    0b0010000110000100,
    0b0100000110000010,
    0b0100000000000010,
    0b0100001110000010,
    0b0100000110000010,
    0b0100000110000010,
    0b0100000110000010,
    0b0100000110000010,
    0b0100001111000010,
    0b0100000000000010,
    0b0010000000000100,
    0b0001100000011000,
    0b0000011111100000,
    0b0000000000000000,
];

/// Terminal icon
pub const ICON_TERMINAL: [u16; 16] = [
    0b0111111111111110,
    0b0111111111111110,
    0b0100000000000010,
    0b0100100000000010,
    0b0100010000000010,
    0b0100001000000010,
    0b0100010000000010,
    0b0100100000000010,
    0b0100000111100010,
    0b0100000000000010,
    0b0100000000000010,
    0b0100000000000010,
    0b0100000000000010,
    0b0100000000000010,
    0b0111111111111110,
    0b0000000000000000,
];

// ============================================================================
// Mouse Cursor (12x19 arrow pointer)
// ============================================================================

/// Arrow cursor bitmap (12 pixels wide, 19 pixels tall)
/// 0 = transparent, 1 = black outline, 2 = white fill
const CURSOR_WIDTH: u32 = 12;
const CURSOR_HEIGHT: u32 = 19;

static CURSOR_BITMAP: [[u8; 12]; 19] = [
    [1,0,0,0,0,0,0,0,0,0,0,0],
    [1,1,0,0,0,0,0,0,0,0,0,0],
    [1,2,1,0,0,0,0,0,0,0,0,0],
    [1,2,2,1,0,0,0,0,0,0,0,0],
    [1,2,2,2,1,0,0,0,0,0,0,0],
    [1,2,2,2,2,1,0,0,0,0,0,0],
    [1,2,2,2,2,2,1,0,0,0,0,0],
    [1,2,2,2,2,2,2,1,0,0,0,0],
    [1,2,2,2,2,2,2,2,1,0,0,0],
    [1,2,2,2,2,2,2,2,2,1,0,0],
    [1,2,2,2,2,2,2,2,2,2,1,0],
    [1,2,2,2,2,2,2,1,1,1,1,0],
    [1,2,2,2,1,2,2,1,0,0,0,0],
    [1,2,2,1,0,1,2,2,1,0,0,0],
    [1,2,1,0,0,1,2,2,1,0,0,0],
    [1,1,0,0,0,0,1,2,2,1,0,0],
    [1,0,0,0,0,0,1,2,2,1,0,0],
    [0,0,0,0,0,0,0,1,2,1,0,0],
    [0,0,0,0,0,0,0,1,1,0,0,0],
];

/// Draw the mouse cursor at the given position.
fn draw_cursor(mx: u32, my: u32) {
    let mut fb = FRAMEBUFFER.lock();
    for row in 0..CURSOR_HEIGHT {
        for col in 0..CURSOR_WIDTH {
            let px = mx + col;
            let py = my + row;
            if px < fb.width && py < fb.height {
                match CURSOR_BITMAP[row as usize][col as usize] {
                    1 => fb.put_pixel(px, py, Color::BLACK),
                    2 => fb.put_pixel(px, py, Color::WHITE),
                    _ => {} // transparent
                }
            }
        }
    }
}

// ============================================================================
// Mouse State for Desktop
// ============================================================================

/// Mouse click event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseClick {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
}

/// Track previous mouse button state to detect edges (press/release)
struct MouseTracker {
    prev_left: bool,
    prev_right: bool,
    cursor_x: i32,
    cursor_y: i32,
}

impl MouseTracker {
    fn new(screen_w: u32, screen_h: u32) -> Self {
        Self {
            prev_left: false,
            prev_right: false,
            cursor_x: (screen_w / 2) as i32,
            cursor_y: (screen_h / 2) as i32,
        }
    }

    /// Process all pending mouse events and return the latest click (if any).
    fn process_events(&mut self, screen_w: u32, screen_h: u32) -> (bool, Option<MouseClick>) {
        let mut moved = false;
        let mut click = None;

        while let Some(event) = mouse::try_read_event() {
            // Apply deltas
            self.cursor_x += event.dx as i32;
            self.cursor_y += event.dy as i32;

            // Clamp to screen bounds
            if self.cursor_x < 0 { self.cursor_x = 0; }
            if self.cursor_y < 0 { self.cursor_y = 0; }
            if self.cursor_x >= screen_w as i32 { self.cursor_x = screen_w as i32 - 1; }
            if self.cursor_y >= screen_h as i32 { self.cursor_y = screen_h as i32 - 1; }

            moved = true;

            // Detect button edges (press/release transitions)
            if event.buttons.left && !self.prev_left {
                click = Some(MouseClick::LeftDown);
            }
            if !event.buttons.left && self.prev_left {
                click = Some(MouseClick::LeftUp);
            }
            if event.buttons.right && !self.prev_right {
                click = Some(MouseClick::RightDown);
            }

            self.prev_left = event.buttons.left;
            self.prev_right = event.buttons.right;
        }

        // Update the global mouse position
        mouse::set_position(self.cursor_x, self.cursor_y);

        (moved, click)
    }

    fn x(&self) -> u32 { self.cursor_x as u32 }
    fn y(&self) -> u32 { self.cursor_y as u32 }
}

// ============================================================================
// Desktop Entry Point
// ============================================================================

/// Desktop icon descriptor
struct DesktopIcon {
    name: &'static str,
    icon: &'static [u16; 16],
    app_id: apps::AppId,
    x: u32,
    y: u32,
}

/// Desktop state
struct DesktopState {
    icons: Vec<DesktopIcon>,
    selected_icon: Option<usize>,
    start_menu_open: bool,
    start_menu_selection: usize,
    screen_w: u32,
    screen_h: u32,
}

impl DesktopState {
    fn new(screen_w: u32, screen_h: u32) -> Self {
        let icon_spacing_y = 70;
        let icon_x = 20;
        let icon_start_y = 20;

        let icons = alloc::vec![
            DesktopIcon {
                name: "System Info",
                icon: &ICON_COMPUTER,
                app_id: apps::AppId::SystemInfo,
                x: icon_x,
                y: icon_start_y,
            },
            DesktopIcon {
                name: "Task Manager",
                icon: &ICON_COMPUTER,
                app_id: apps::AppId::TaskManager,
                x: icon_x,
                y: icon_start_y + icon_spacing_y,
            },
            DesktopIcon {
                name: "Notepad",
                icon: &ICON_NOTEPAD,
                app_id: apps::AppId::Notepad,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 2,
            },
            DesktopIcon {
                name: "Calculator",
                icon: &ICON_CALC,
                app_id: apps::AppId::Calculator,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 3,
            },
            DesktopIcon {
                name: "About",
                icon: &ICON_INFO,
                app_id: apps::AppId::About,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 4,
            },
            DesktopIcon {
                name: "Terminal",
                icon: &ICON_TERMINAL,
                app_id: apps::AppId::Terminal,
                x: icon_x,
                y: icon_start_y + icon_spacing_y * 5,
            },
        ];

        Self {
            icons,
            selected_icon: None,
            start_menu_open: false,
            start_menu_selection: 0,
            screen_w,
            screen_h,
        }
    }
}

/// Enter the desktop environment. Returns when the user exits (Escape at desktop).
pub fn run() {
    let (sw, sh) = screen_size();
    let mut desktop = DesktopState::new(sw, sh);
    let mut window_mgr = wm::WindowManager::new(sw, sh);
    let mut mouse_tracker = MouseTracker::new(sw, sh);

    // Initial draw
    draw_desktop(&desktop);
    taskbar::draw(&window_mgr, sw, sh);
    draw_cursor(mouse_tracker.x(), mouse_tracker.y());
    present();

    loop {
        // Periodically refresh clock (~1 second)
        let now = crate::shell::ticks();
        static mut LAST_CLOCK_TICK: u64 = 0;
        let should_refresh_clock = unsafe {
            if now.wrapping_sub(LAST_CLOCK_TICK) >= 1000 {
                LAST_CLOCK_TICK = now;
                true
            } else {
                false
            }
        };

        if should_refresh_clock {
            taskbar::draw(&window_mgr, sw, sh);
            draw_cursor(mouse_tracker.x(), mouse_tracker.y());
            present();
        }

        // --- Process mouse events ---
        let (mouse_moved, mouse_click) = mouse_tracker.process_events(sw, sh);
        let mut needs_redraw = false;

        // Handle mouse click
        if let Some(click) = mouse_click {
            if click == MouseClick::LeftDown {
                let mx = mouse_tracker.x();
                let my = mouse_tracker.y();

                let result = handle_mouse_click(&mut desktop, &mut window_mgr, mx, my, sw, sh);
                match result {
                    InputResult::ExitDesktop => return,
                    InputResult::Redraw => { needs_redraw = true; }
                    InputResult::Continue => {}
                }
            }
        }

        if mouse_moved || needs_redraw {
            // Full redraw on click, optimized redraw on just movement
            draw_desktop(&desktop);
            if desktop.start_menu_open {
                draw_start_menu(&desktop);
            }
            window_mgr.draw_all();
            taskbar::draw(&window_mgr, sw, sh);
            draw_cursor(mouse_tracker.x(), mouse_tracker.y());
            present();
        }

        // --- Process keyboard events ---
        let event = match keyboard::try_read_char() {
            Some(e) => e,
            None => {
                if !mouse_moved && !needs_redraw {
                    unsafe { core::arch::asm!("hlt"); }
                }
                continue;
            }
        };

        // Route input based on what's active
        if desktop.start_menu_open {
            match handle_start_menu_input(&mut desktop, &mut window_mgr, &event) {
                InputResult::Continue => {}
                InputResult::ExitDesktop => return,
                InputResult::Redraw => {
                    draw_desktop(&desktop);
                    if desktop.start_menu_open {
                        draw_start_menu(&desktop);
                    }
                    window_mgr.draw_all();
                    taskbar::draw(&window_mgr, sw, sh);
                    draw_cursor(mouse_tracker.x(), mouse_tracker.y());
                    present();
                }
            }
        } else if window_mgr.has_focused_window() {
            match window_mgr.handle_input(&event) {
                InputResult::Continue => {}
                InputResult::ExitDesktop => return,
                InputResult::Redraw => {
                    draw_desktop(&desktop);
                    window_mgr.draw_all();
                    taskbar::draw(&window_mgr, sw, sh);
                    draw_cursor(mouse_tracker.x(), mouse_tracker.y());
                    present();
                }
            }
        } else {
            // Desktop has focus
            match handle_desktop_input(&mut desktop, &mut window_mgr, &event) {
                InputResult::Continue => {}
                InputResult::ExitDesktop => return,
                InputResult::Redraw => {
                    draw_desktop(&desktop);
                    if desktop.start_menu_open {
                        draw_start_menu(&desktop);
                    }
                    window_mgr.draw_all();
                    taskbar::draw(&window_mgr, sw, sh);
                    draw_cursor(mouse_tracker.x(), mouse_tracker.y());
                    present();
                }
            }
        }
    }
}

/// Result of processing an input event.
pub enum InputResult {
    /// Nothing changed, no redraw needed.
    Continue,
    /// Screen needs redrawing.
    Redraw,
    /// Exit the desktop and return to shell.
    ExitDesktop,
}

// ============================================================================
// Desktop Input Handling
// ============================================================================

fn handle_desktop_input(
    desktop: &mut DesktopState,
    wm: &mut wm::WindowManager,
    event: &KeyEvent,
) -> InputResult {
    match event.key {
        // Escape at desktop = exit to shell
        KeyCode::Escape => InputResult::ExitDesktop,

        // F1 or Enter on nothing = open start menu
        KeyCode::F1 => {
            desktop.start_menu_open = true;
            desktop.start_menu_selection = 0;
            InputResult::Redraw
        }

        // Tab = cycle icons
        KeyCode::Tab => {
            let n = desktop.icons.len();
            if n > 0 {
                desktop.selected_icon = Some(match desktop.selected_icon {
                    Some(i) => (i + 1) % n,
                    None => 0,
                });
            }
            InputResult::Redraw
        }

        // Up/Down = navigate icons
        KeyCode::Up => {
            if let Some(i) = desktop.selected_icon {
                if i > 0 {
                    desktop.selected_icon = Some(i - 1);
                }
            } else if !desktop.icons.is_empty() {
                desktop.selected_icon = Some(0);
            }
            InputResult::Redraw
        }

        KeyCode::Down => {
            let n = desktop.icons.len();
            if let Some(i) = desktop.selected_icon {
                if i + 1 < n {
                    desktop.selected_icon = Some(i + 1);
                }
            } else if n > 0 {
                desktop.selected_icon = Some(0);
            }
            InputResult::Redraw
        }

        // Enter = launch selected icon
        KeyCode::Enter => {
            if let Some(idx) = desktop.selected_icon {
                let app_id = desktop.icons[idx].app_id;
                wm.open_app(app_id);
            }
            InputResult::Redraw
        }

        _ => InputResult::Continue,
    }
}

fn handle_start_menu_input(
    desktop: &mut DesktopState,
    wm: &mut wm::WindowManager,
    event: &KeyEvent,
) -> InputResult {
    let menu_items = start_menu_items();
    let count = menu_items.len();

    match event.key {
        KeyCode::Escape | KeyCode::F1 => {
            desktop.start_menu_open = false;
            InputResult::Redraw
        }

        KeyCode::Up => {
            if desktop.start_menu_selection > 0 {
                desktop.start_menu_selection -= 1;
            }
            InputResult::Redraw
        }

        KeyCode::Down => {
            if desktop.start_menu_selection + 1 < count {
                desktop.start_menu_selection += 1;
            }
            InputResult::Redraw
        }

        KeyCode::Enter => {
            let sel = desktop.start_menu_selection;
            desktop.start_menu_open = false;

            if sel < count {
                let (_, app_id) = menu_items[sel];
                match app_id {
                    Some(id) => wm.open_app(id),
                    None => return InputResult::ExitDesktop, // "Exit to Shell"
                }
            }
            InputResult::Redraw
        }

        _ => InputResult::Continue,
    }
}

// ============================================================================
// Mouse Click Handling
// ============================================================================

/// Handle a mouse left-click at screen coordinates (mx, my).
fn handle_mouse_click(
    desktop: &mut DesktopState,
    wm: &mut wm::WindowManager,
    mx: u32,
    my: u32,
    screen_w: u32,
    screen_h: u32,
) -> InputResult {
    let tb_y = screen_h - TASKBAR_HEIGHT;

    // --- Start menu click handling ---
    if desktop.start_menu_open {
        let items = start_menu_items();
        let item_h = CHAR_HEIGHT + 4;
        let menu_w: u32 = 200;
        let menu_h = items.len() as u32 * item_h + 4;
        let menu_x: u32 = 2;
        let menu_y = screen_h - TASKBAR_HEIGHT - menu_h;

        // Click inside start menu?
        if mx >= menu_x && mx < menu_x + menu_w && my >= menu_y && my < menu_y + menu_h {
            let item_idx = ((my - menu_y - 2) / item_h) as usize;
            if item_idx < items.len() {
                let (label, app_id) = &items[item_idx];
                if *label != "  ─────────────" {
                    desktop.start_menu_open = false;
                    match app_id {
                        Some(id) => wm.open_app(*id),
                        None => return InputResult::ExitDesktop,
                    }
                    return InputResult::Redraw;
                }
            }
            return InputResult::Continue;
        }

        // Click outside menu — close it
        desktop.start_menu_open = false;
        // Fall through to check other click targets
    }

    // --- Taskbar click handling ---
    if my >= tb_y {
        // Start button area (2, tb_y+2, 60, TASKBAR_HEIGHT-4)
        let start_x: u32 = 2;
        let start_w: u32 = 60;
        if mx >= start_x && mx < start_x + start_w && my >= tb_y + 2 {
            desktop.start_menu_open = !desktop.start_menu_open;
            desktop.start_menu_selection = 0;
            return InputResult::Redraw;
        }

        // Taskbar window buttons — click to focus
        let btn_area_x = start_x + start_w + 40;
        let clock_w: u32 = 80;
        let btn_area_w = screen_w - btn_area_x - clock_w - 8;
        let titles = wm.window_titles();
        let btn_count = titles.len();

        if btn_count > 0 && mx >= btn_area_x {
            let btn_w = (btn_area_w / btn_count as u32).min(160);
            for (i, (id, _, _)) in titles.iter().enumerate() {
                let bx = btn_area_x + i as u32 * (btn_w + 2);
                if mx >= bx && mx < bx + btn_w {
                    wm.focus_window_by_id(*id);
                    return InputResult::Redraw;
                }
            }
        }

        return InputResult::Redraw;
    }

    // --- Window click handling (front-to-back for proper z-order) ---
    if let Some(result) = wm.handle_click(mx, my) {
        return result;
    }

    // --- Desktop icon click handling ---
    for (i, icon) in desktop.icons.iter().enumerate() {
        // Icon clickable area: icon (16x16) + label area below
        let icon_area_x = icon.x.saturating_sub(2);
        let icon_area_y = icon.y.saturating_sub(2);
        let label_w = icon.name.len() as u32 * CHAR_WIDTH + 8;
        let icon_area_w = label_w.max(20);
        let icon_area_h = 20 + CHAR_HEIGHT + 4;

        if mx >= icon_area_x && mx < icon_area_x + icon_area_w
            && my >= icon_area_y && my < icon_area_y + icon_area_h
        {
            // Double-click simulation: if already selected, launch
            if desktop.selected_icon == Some(i) {
                wm.open_app(icon.app_id);
            } else {
                desktop.selected_icon = Some(i);
            }
            return InputResult::Redraw;
        }
    }

    // Click on empty desktop — deselect icons, unfocus windows
    desktop.selected_icon = None;
    wm.unfocus_all();
    InputResult::Redraw
}

// ============================================================================
// Drawing
// ============================================================================

/// Draw the desktop background and icons.
fn draw_desktop(desktop: &DesktopState) {
    let (sw, sh) = screen_size();

    // Fill desktop background
    fill_rect(0, 0, sw, sh - TASKBAR_HEIGHT, DESKTOP_BG);

    // Draw desktop icons
    for (i, icon) in desktop.icons.iter().enumerate() {
        let selected = desktop.selected_icon == Some(i);
        draw_desktop_icon(icon, selected);
    }
}

/// Draw a single desktop icon with label.
fn draw_desktop_icon(icon: &DesktopIcon, selected: bool) {
    let ix = icon.x;
    let iy = icon.y;

    // Icon background highlight if selected
    if selected {
        fill_rect(ix - 2, iy - 2, 20, 20, TITLE_ACTIVE);
    }

    // Draw the 16x16 icon
    draw_icon_16(ix, iy, icon.icon, Color::WHITE, Some(if selected { TITLE_ACTIVE } else { DESKTOP_BG }));

    // Label (centered under icon)
    let label_x = if icon.name.len() > 6 {
        ix.saturating_sub(((icon.name.len() - 2) as u32 * CHAR_WIDTH) / 2)
    } else {
        ix
    };
    let label_y = iy + 20;

    if selected {
        // Selected: highlight background
        let label_w = icon.name.len() as u32 * CHAR_WIDTH + 4;
        fill_rect(label_x.saturating_sub(2), label_y, label_w, CHAR_HEIGHT, TITLE_ACTIVE);
        draw_text(label_x, label_y, icon.name, Color::WHITE);
    } else {
        draw_text(label_x, label_y, icon.name, ICON_TEXT);
    }
}

/// Start menu items: (label, app_id or None for "Exit")
fn start_menu_items() -> Vec<(&'static str, Option<apps::AppId>)> {
    alloc::vec![
        ("  System Info",   Some(apps::AppId::SystemInfo)),
        ("  Task Manager",  Some(apps::AppId::TaskManager)),
        ("  Notepad",       Some(apps::AppId::Notepad)),
        ("  Calculator",    Some(apps::AppId::Calculator)),
        ("  About CantayaOS", Some(apps::AppId::About)),
        ("  Terminal",      Some(apps::AppId::Terminal)),
        ("  ─────────────", None), // separator (will skip in input)
        ("  Exit to Shell", None),
    ]
}

/// Draw the start menu popup.
fn draw_start_menu(desktop: &DesktopState) {
    let items = start_menu_items();
    let item_h = CHAR_HEIGHT + 4;
    let menu_w: u32 = 200;
    let menu_h = items.len() as u32 * item_h + 4;
    let (_, sh) = screen_size();
    let menu_x: u32 = 2;
    let menu_y = sh - TASKBAR_HEIGHT - menu_h;

    // Menu background with 3D border
    fill_rect(menu_x, menu_y, menu_w, menu_h, MENU_BG);
    draw_raised_rect(menu_x, menu_y, menu_w, menu_h);

    // Side banner (Windows 95 style blue/gray strip on the left)
    fill_rect(menu_x + 2, menu_y + 2, 22, menu_h - 4, TITLE_ACTIVE);
    // Vertical "CantayaOS" text would go here, but let's just put a colored strip

    // Menu items
    for (i, (label, _)) in items.iter().enumerate() {
        let iy = menu_y + 2 + i as u32 * item_h;
        let ix = menu_x + 26;

        if *label == "  ─────────────" {
            // Separator line
            {
                let mut fb = FRAMEBUFFER.lock();
                fb.fill_rect(menu_x + 26, iy + item_h / 2, menu_w - 30, 1, SHADOW);
                fb.fill_rect(menu_x + 26, iy + item_h / 2 + 1, menu_w - 30, 1, HIGHLIGHT);
            }
            continue;
        }

        let selected = desktop.start_menu_selection == i;
        if selected {
            fill_rect(ix - 2, iy, menu_w - 28, item_h, MENU_HIGHLIGHT);
            draw_text(ix, iy + 2, label, MENU_TEXT_HI);
        } else {
            draw_text(ix, iy + 2, label, MENU_TEXT);
        }
    }
}
