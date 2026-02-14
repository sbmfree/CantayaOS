// Input handling for the CantayaOS desktop environment.

use crate::hal::keyboard::{KeyCode, KeyEvent};
use crate::hal::mouse;
use crate::graphics::font::{CHAR_WIDTH, CHAR_HEIGHT};

use super::{DesktopState, TASKBAR_HEIGHT};
use super::wm::WindowManager;
use super::apps;

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
pub(super) struct MouseTracker {
    prev_left: bool,
    prev_right: bool,
    cursor_x: i32,
    cursor_y: i32,
}

impl MouseTracker {
    pub(super) fn new(screen_w: u32, screen_h: u32) -> Self {
        Self {
            prev_left: false,
            prev_right: false,
            cursor_x: (screen_w / 2) as i32,
            cursor_y: (screen_h / 2) as i32,
        }
    }

    /// Process all pending mouse events and return the latest click (if any).
    pub(super) fn process_events(&mut self, screen_w: u32, screen_h: u32) -> (bool, Option<MouseClick>) {
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

    pub(super) fn x(&self) -> u32 { self.cursor_x as u32 }
    pub(super) fn y(&self) -> u32 { self.cursor_y as u32 }
}

// ============================================================================
// Desktop Input Handling
// ============================================================================

pub(super) fn handle_desktop_input(
    desktop: &mut DesktopState,
    wm: &mut WindowManager,
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

pub(super) fn handle_start_menu_input(
    desktop: &mut DesktopState,
    wm: &mut WindowManager,
    event: &KeyEvent,
) -> InputResult {
    let menu_items = super::start_menu_items();
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
pub(super) fn handle_mouse_click(
    desktop: &mut DesktopState,
    wm: &mut WindowManager,
    mx: u32,
    my: u32,
    screen_w: u32,
    screen_h: u32,
) -> InputResult {
    let tb_y = screen_h - TASKBAR_HEIGHT;

    // --- Start menu click handling ---
    if desktop.start_menu_open {
        let items = super::start_menu_items();
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

        // Taskbar window buttons — click to focus/minimize
        let btn_area_x = start_x + start_w + 40;
        let clock_w: u32 = 80;
        let btn_area_w = screen_w - btn_area_x - clock_w - 8;
        let titles = wm.window_titles();
        let btn_count = titles.len();

        if btn_count > 0 && mx >= btn_area_x {
            let btn_w = (btn_area_w / btn_count as u32).min(160);
            for (i, (id, _, _, _)) in titles.iter().enumerate() {
                let bx = btn_area_x + i as u32 * (btn_w + 2);
                if mx >= bx && mx < bx + btn_w {
                    wm.toggle_minimize_by_id(*id);
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
