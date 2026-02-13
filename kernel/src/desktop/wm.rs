// Window Manager for CantayaOS Desktop
//
// Manages overlapping windows with mouse-driven interaction.
// Each window has a title bar with minimize/maximize/close buttons,
// supports dragging, resizing, and proper z-order management.
// Inspired by the Windows 95/2000 window manager aesthetics.
//
// Mouse:
//   Drag title bar:  move window
//   Drag edges/corners: resize window
//   Click minimize [_]: minimize to taskbar
//   Click maximize [□]: toggle maximize/restore
//   Click close [X]: close window
//   Click window: bring to front
//   Right-click: context menu (handled by desktop)
//
// Keyboard:
//   F2: cycle focus between windows
//   F4 / Escape: close focused window
//   Arrow keys / typing: forwarded to focused window's app handler

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use super::{
    InputResult, BORDER_WIDTH, TITLEBAR_HEIGHT, TASKBAR_HEIGHT,
    WINDOW_BG, WINDOW_CLIENT, TITLE_ACTIVE, TITLE_INACTIVE, TITLE_TEXT,
    BUTTON_FACE, SHADOW,
    draw_text, draw_raised_rect, draw_sunken_rect, fill_rect,
};
use super::apps::{self, AppId, AppState};
use crate::hal::keyboard::{KeyCode, KeyEvent};
use crate::graphics::framebuffer::Color;

/// Maximum number of open windows
const MAX_WINDOWS: usize = 8;

/// Size of the resize grab zone at window edges (pixels).
const RESIZE_BORDER: u32 = 5;

/// Minimum window dimensions.
const MIN_WIN_WIDTH: u32 = 160;
const MIN_WIN_HEIGHT: u32 = 100;

/// Drag / resize interaction state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragState {
    /// No drag in progress.
    None,
    /// Moving a window. Offsets are mouse position minus window top-left.
    Moving { win_id: u32, offset_x: i32, offset_y: i32 },
    /// Resizing a window from an edge or corner.
    Resizing {
        win_id: u32,
        edge: ResizeEdge,
        start_mx: u32,
        start_my: u32,
        orig_x: u32,
        orig_y: u32,
        orig_w: u32,
        orig_h: u32,
    },
}

/// Which edge(s) the user grabbed for resizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

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
    /// Whether the window is minimized (hidden, shown only in taskbar)
    pub minimized: bool,
    /// Whether the window is maximized (fills desktop area)
    pub maximized: bool,
    /// Saved geometry for restore-from-maximize
    pub restore_x: u32,
    pub restore_y: u32,
    pub restore_w: u32,
    pub restore_h: u32,
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
            AppId::Paint       => ("Paint", 500u32, 400u32),
            AppId::Minesweeper => ("Minesweeper", 320u32, 380u32),
            AppId::Snake       => ("Snake", 340u32, 380u32),
            AppId::Settings    => ("Settings", 400u32, 360u32),
            AppId::Clock       => ("Clock", 280u32, 240u32),
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
            minimized: false,
            maximized: false,
            restore_x: x,
            restore_y: y,
            restore_w: width,
            restore_h: height,
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

    /// Maximize this window to fill the desktop area.
    pub fn maximize(&mut self, screen_w: u32, screen_h: u32) {
        if !self.maximized {
            // Save current geometry for restore
            self.restore_x = self.x;
            self.restore_y = self.y;
            self.restore_w = self.width;
            self.restore_h = self.height;
            self.x = 0;
            self.y = 0;
            self.width = screen_w;
            self.height = screen_h - TASKBAR_HEIGHT;
            self.maximized = true;
        }
    }

    /// Restore from maximized state.
    pub fn restore_from_maximize(&mut self) {
        if self.maximized {
            self.x = self.restore_x;
            self.y = self.restore_y;
            self.width = self.restore_w;
            self.height = self.restore_h;
            self.maximized = false;
        }
    }

    /// Toggle maximize / restore.
    pub fn toggle_maximize(&mut self, screen_w: u32, screen_h: u32) {
        if self.maximized {
            self.restore_from_maximize();
        } else {
            self.maximize(screen_w, screen_h);
        }
    }

    /// Draw the window (frame + title bar + client area).
    pub fn draw(&self) {
        if self.minimized { return; }

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
        let max_chars = ((w.saturating_sub(BORDER_WIDTH * 2 + 58)) / 8) as usize; // room for 3 buttons
        let display_title: &str = if self.title.len() > max_chars {
            &self.title[..max_chars]
        } else {
            &self.title
        };
        draw_text(title_x, title_y, display_title, TITLE_TEXT);

        // --- Window control buttons (right side of title bar) ---
        let btn_y = y + BORDER_WIDTH + 2;
        let btn_h: u32 = 14;
        let btn_w: u32 = 16;

        // Close button [X] — rightmost
        let close_x = x + w - BORDER_WIDTH - btn_w - 2;
        fill_rect(close_x, btn_y, btn_w, btn_h, BUTTON_FACE);
        draw_raised_rect(close_x, btn_y, btn_w, btn_h);
        draw_text(close_x + 4, btn_y - 1, "x", Color::BLACK);

        // Maximize/Restore button [□] — second from right
        let max_x = close_x - btn_w - 2;
        fill_rect(max_x, btn_y, btn_w, btn_h, BUTTON_FACE);
        draw_raised_rect(max_x, btn_y, btn_w, btn_h);
        if self.maximized {
            // Restore icon: two overlapping squares
            draw_text(max_x + 3, btn_y - 1, "=", Color::BLACK);
        } else {
            // Maximize icon: single square outline
            {
                let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
                // Top edge
                fb.fill_rect(max_x + 3, btn_y + 2, 10, 1, Color::BLACK);
                fb.fill_rect(max_x + 3, btn_y + 3, 10, 1, Color::BLACK);
                // Bottom edge
                fb.fill_rect(max_x + 3, btn_y + 10, 10, 1, Color::BLACK);
                // Left edge
                fb.fill_rect(max_x + 3, btn_y + 2, 1, 9, Color::BLACK);
                // Right edge
                fb.fill_rect(max_x + 12, btn_y + 2, 1, 9, Color::BLACK);
            }
        }

        // Minimize button [_] — third from right
        let min_x = max_x - btn_w - 2;
        fill_rect(min_x, btn_y, btn_w, btn_h, BUTTON_FACE);
        draw_raised_rect(min_x, btn_y, btn_w, btn_h);
        {
            let mut fb = crate::graphics::framebuffer::FRAMEBUFFER.lock();
            fb.fill_rect(min_x + 3, btn_y + 10, 10, 2, Color::BLACK);
        }

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

    /// Hit-test the three control buttons. Returns which button was clicked.
    pub fn hit_test_buttons(&self, mx: u32, my: u32) -> Option<WindowButton> {
        let btn_y = self.y + BORDER_WIDTH + 2;
        let btn_h: u32 = 14;
        let btn_w: u32 = 16;

        // Must be in the button row vertically
        if my < btn_y || my >= btn_y + btn_h { return None; }

        let close_x = self.x + self.width - BORDER_WIDTH - btn_w - 2;
        if mx >= close_x && mx < close_x + btn_w { return Some(WindowButton::Close); }

        let max_x = close_x - btn_w - 2;
        if mx >= max_x && mx < max_x + btn_w { return Some(WindowButton::Maximize); }

        let min_x = max_x - btn_w - 2;
        if mx >= min_x && mx < min_x + btn_w { return Some(WindowButton::Minimize); }

        None
    }

    /// Test if a point is within the title bar (but not on a button).
    pub fn hit_test_titlebar(&self, mx: u32, my: u32) -> bool {
        let tb_x = self.x + BORDER_WIDTH;
        let tb_y = self.y + BORDER_WIDTH;
        let tb_w = self.width - BORDER_WIDTH * 2;

        mx >= tb_x && mx < tb_x + tb_w
            && my >= tb_y && my < tb_y + TITLEBAR_HEIGHT
            && self.hit_test_buttons(mx, my).is_none()
    }

    /// Test if a point is on the resize border. Returns the edge if so.
    pub fn hit_test_resize_edge(&self, mx: u32, my: u32) -> Option<ResizeEdge> {
        if self.maximized { return None; } // can't resize when maximized

        let x = self.x;
        let y = self.y;
        let w = self.width;
        let h = self.height;
        let r = RESIZE_BORDER;

        // Must be within the window bounds (with extended grab zone)
        if mx + r < x || mx > x + w + r || my + r < y || my > y + h + r {
            return None;
        }

        let on_left   = mx < x + r;
        let on_right  = mx >= x + w - r;
        let on_top    = my < y + r;
        let on_bottom = my >= y + h - r;

        match (on_left, on_right, on_top, on_bottom) {
            (true, _, true, _)    => Some(ResizeEdge::TopLeft),
            (true, _, _, true)    => Some(ResizeEdge::BottomLeft),
            (_, true, true, _)    => Some(ResizeEdge::TopRight),
            (_, true, _, true)    => Some(ResizeEdge::BottomRight),
            (true, _, _, _)       => Some(ResizeEdge::Left),
            (_, true, _, _)       => Some(ResizeEdge::Right),
            (_, _, true, _)       => Some(ResizeEdge::Top),
            (_, _, _, true)       => Some(ResizeEdge::Bottom),
            _                     => None,
        }
    }

    /// Test if a point is inside the client area.
    pub fn hit_test_client(&self, mx: u32, my: u32) -> bool {
        let cx = self.client_x();
        let cy = self.client_y();
        mx >= cx && mx < cx + self.client_width()
            && my >= cy && my < cy + self.client_height()
    }
}

/// Window control button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

// ============================================================================
// Window Manager
// ============================================================================

pub struct WindowManager {
    windows: Vec<Window>,
    next_id: u32,
    screen_w: u32,
    screen_h: u32,
    /// Current drag/resize interaction state.
    pub drag: DragState,
}

impl WindowManager {
    pub fn new(screen_w: u32, screen_h: u32) -> Self {
        Self {
            windows: Vec::new(),
            next_id: 1,
            screen_w,
            screen_h,
            drag: DragState::None,
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

        // Focus the new window (bring to front)
        self.focus_last();
    }

    /// Close the focused window.
    pub fn close_focused(&mut self) {
        if let Some(idx) = self.focused_index() {
            self.windows.remove(idx);
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
        // Cancel any drag referencing this window
        match self.drag {
            DragState::Moving { win_id, .. } | DragState::Resizing { win_id, .. } if win_id == id => {
                self.drag = DragState::None;
            }
            _ => {}
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

    /// Focus the last window (most recently opened / top of z-stack).
    fn focus_last(&mut self) {
        let len = self.windows.len();
        if len > 0 {
            self.set_focus(len - 1);
        }
    }

    /// Bring a window to the front of the z-order (end of Vec) and focus it.
    pub fn bring_to_front(&mut self, id: u32) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            // Already at front?
            if idx == self.windows.len() - 1 {
                self.set_focus(idx);
                return;
            }
            let win = self.windows.remove(idx);
            self.windows.push(win);
            self.focus_last();
        }
    }

    /// Cycle focus to the next window (F2 key).
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
        // F4 or Escape = close focused window
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

    /// Draw all visible windows (back to front for proper z-ordering).
    pub fn draw_all(&self) {
        for win in &self.windows {
            if !win.minimized {
                win.draw();
            }
        }
    }

    /// Get list of window titles for the taskbar.
    pub fn window_titles(&self) -> Vec<(u32, String, bool, bool)> {
        self.windows
            .iter()
            .map(|w| (w.id, w.title.clone(), w.focused, w.minimized))
            .collect()
    }

    /// Get the number of open windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Focus a specific window by its ID.
    pub fn focus_window_by_id(&mut self, id: u32) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            // Un-minimize if needed
            self.windows[idx].minimized = false;
            self.set_focus(idx);
        }
    }

    /// Toggle minimize for a window (taskbar click).
    pub fn toggle_minimize_by_id(&mut self, id: u32) {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            let win = &mut self.windows[idx];
            if win.minimized {
                // Restore from taskbar
                win.minimized = false;
                self.set_focus(idx);
            } else if win.focused {
                // Already focused — minimize it
                win.minimized = true;
                win.focused = false;
                // Focus next visible window
                for w in self.windows.iter_mut().rev() {
                    if !w.minimized {
                        w.focused = true;
                        break;
                    }
                }
            } else {
                // Not focused — bring to front and focus
                self.set_focus(idx);
            }
        }
    }

    /// Unfocus all windows (desktop gets focus).
    pub fn unfocus_all(&mut self) {
        for w in self.windows.iter_mut() {
            w.focused = false;
        }
    }

    /// Handle a mouse left-button-down at screen position (mx, my).
    /// Returns Some(InputResult) if a window was hit, None otherwise.
    pub fn handle_click(&mut self, mx: u32, my: u32) -> Option<InputResult> {
        // Check windows in reverse order (front-to-back since last = top)
        let mut hit_idx = None;
        for (i, win) in self.windows.iter().enumerate().rev() {
            if win.minimized { continue; }
            if mx >= win.x.saturating_sub(RESIZE_BORDER)
                && mx < win.x + win.width + RESIZE_BORDER
                && my >= win.y.saturating_sub(RESIZE_BORDER)
                && my < win.y + win.height + RESIZE_BORDER
            {
                hit_idx = Some(i);
                break;
            }
        }

        let idx = hit_idx?;
        let win_id = self.windows[idx].id;

        // Bring to front and focus
        self.bring_to_front(win_id);

        // Re-find the window (it moved to the end)
        let idx = self.windows.len() - 1;
        let win = &self.windows[idx];

        // Check control buttons first
        if let Some(btn) = win.hit_test_buttons(mx, my) {
            match btn {
                WindowButton::Close => {
                    self.close_window(win_id);
                    return Some(InputResult::Redraw);
                }
                WindowButton::Maximize => {
                    let sw = self.screen_w;
                    let sh = self.screen_h;
                    self.windows[idx].toggle_maximize(sw, sh);
                    return Some(InputResult::Redraw);
                }
                WindowButton::Minimize => {
                    self.windows[idx].minimized = true;
                    self.windows[idx].focused = false;
                    // Focus next visible window
                    for w in self.windows.iter_mut().rev() {
                        if !w.minimized {
                            w.focused = true;
                            break;
                        }
                    }
                    return Some(InputResult::Redraw);
                }
            }
        }

        // Check resize edges
        if let Some(edge) = win.hit_test_resize_edge(mx, my) {
            self.drag = DragState::Resizing {
                win_id,
                edge,
                start_mx: mx,
                start_my: my,
                orig_x: win.x,
                orig_y: win.y,
                orig_w: win.width,
                orig_h: win.height,
            };
            return Some(InputResult::Redraw);
        }

        // Check title bar (initiate drag-move)
        if win.hit_test_titlebar(mx, my) {
            // If maximized, restore first so user can drag
            if win.maximized {
                self.windows[idx].restore_from_maximize();
                // Re-position so the cursor stays on the title bar
                let new_w = self.windows[idx].width;
                self.windows[idx].x = mx.saturating_sub(new_w / 2);
            }
            let win = &self.windows[idx];
            self.drag = DragState::Moving {
                win_id,
                offset_x: mx as i32 - win.x as i32,
                offset_y: my as i32 - win.y as i32,
            };
            return Some(InputResult::Redraw);
        }

        // Click in client area — forward to app
        if win.hit_test_client(mx, my) {
            let cx = win.client_x();
            let cy = win.client_y();
            let local_x = mx - cx;
            let local_y = my - cy;
            let result = apps::handle_app_click(&mut self.windows[idx], local_x, local_y);
            return Some(result);
        }

        Some(InputResult::Redraw)
    }

    /// Handle mouse movement during drag/resize.
    /// Returns true if a redraw is needed.
    pub fn handle_mouse_move(&mut self, mx: u32, my: u32) -> bool {
        match self.drag {
            DragState::None => false,
            DragState::Moving { win_id, offset_x, offset_y } => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    let new_x = (mx as i32 - offset_x).max(0) as u32;
                    let new_y = (my as i32 - offset_y).max(0) as u32;
                    // Clamp: keep at least 30px of title bar visible
                    win.x = new_x.min(self.screen_w.saturating_sub(30));
                    win.y = new_y.min(self.screen_h.saturating_sub(TASKBAR_HEIGHT + 10));
                    true
                } else {
                    self.drag = DragState::None;
                    false
                }
            }
            DragState::Resizing { win_id, edge, start_mx, start_my, orig_x, orig_y, orig_w, orig_h } => {
                if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
                    let dx = mx as i32 - start_mx as i32;
                    let dy = my as i32 - start_my as i32;

                    // Apply resize based on edge
                    match edge {
                        ResizeEdge::Right | ResizeEdge::TopRight | ResizeEdge::BottomRight => {
                            win.width = (orig_w as i32 + dx).max(MIN_WIN_WIDTH as i32) as u32;
                        }
                        ResizeEdge::Left | ResizeEdge::TopLeft | ResizeEdge::BottomLeft => {
                            let new_w = (orig_w as i32 - dx).max(MIN_WIN_WIDTH as i32) as u32;
                            win.x = orig_x as i32 as u32 + orig_w - new_w;
                            win.width = new_w;
                        }
                        _ => {}
                    }

                    match edge {
                        ResizeEdge::Bottom | ResizeEdge::BottomLeft | ResizeEdge::BottomRight => {
                            win.height = (orig_h as i32 + dy).max(MIN_WIN_HEIGHT as i32) as u32;
                        }
                        ResizeEdge::Top | ResizeEdge::TopLeft | ResizeEdge::TopRight => {
                            let new_h = (orig_h as i32 - dy).max(MIN_WIN_HEIGHT as i32) as u32;
                            win.y = orig_y as i32 as u32 + orig_h - new_h;
                            win.height = new_h;
                        }
                        _ => {}
                    }

                    true
                } else {
                    self.drag = DragState::None;
                    false
                }
            }
        }
    }

    /// Handle mouse button release — end any active drag.
    pub fn handle_mouse_up(&mut self) {
        self.drag = DragState::None;
    }

    /// Whether a drag or resize is in progress.
    pub fn is_dragging(&self) -> bool {
        self.drag != DragState::None
    }
}
