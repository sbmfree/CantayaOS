//! Window manager — stacking windows with title bars, dragging, close button.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::collections::VecDeque;

use crate::drivers::framebuffer::{self, SCREEN_WIDTH, SCREEN_HEIGHT, FONT_HEIGHT};
use crate::gui::event::GuiEvent;
use crate::sync::IrqMutex;

// ---------------------------------------------------------------------------
// Colours (XRGB8888)
// ---------------------------------------------------------------------------
const TITLE_BAR_BG: u32    = 0xFF2266AA; // blue title bar
const TITLE_BAR_FG: u32    = 0xFFFFFFFF; // white text
const TITLE_BAR_INACTIVE: u32 = 0xFF555577;
const CLOSE_BTN_BG: u32    = 0xFFDD3333;
#[allow(dead_code)]
const CLOSE_BTN_HOVER: u32 = 0xFFFF5555;
const WIN_BG: u32          = 0xFFE8E8E8; // light grey client area
const WIN_BORDER: u32      = 0xFF333333;

pub const TITLE_BAR_H: u32 = 24;
const CLOSE_BTN_SIZE: u32  = 20;
const BORDER_W: u32        = 1;

// ---------------------------------------------------------------------------
// Window
// ---------------------------------------------------------------------------
pub struct Window {
    pub id: u32,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32, // client area height (excluding title bar)
    pub visible: bool,
    pub focused: bool,
    /// Optional text content to display in the client area
    pub content: String,
    /// Optional XRGB8888 pixel buffer for the client area
    pub pixel_buffer: Option<Vec<u32>>,
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// PID of the process that owns this window (0 = kernel)
    pub owner_pid: u32,
    /// Event queue for userspace windows
    pub event_queue: VecDeque<GuiEvent>,
}

impl Window {
    /// Total height including title bar and borders.
    pub fn total_height(&self) -> u32 {
        TITLE_BAR_H + self.height + BORDER_W * 2
    }

    /// Total width including borders.
    pub fn total_width(&self) -> u32 {
        self.width + BORDER_W * 2
    }

    /// Check if a point is inside the window (including title bar).
    pub fn hit_test(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.total_width() as i32
            && py >= self.y && py < self.y + self.total_height() as i32
    }

    /// Check if a point is on the title bar.
    pub fn hit_title_bar(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.total_width() as i32
            && py >= self.y && py < self.y + TITLE_BAR_H as i32
    }

    /// Check if a point is on the close button.
    pub fn hit_close(&self, px: i32, py: i32) -> bool {
        let cx = self.x + self.total_width() as i32 - CLOSE_BTN_SIZE as i32 - 2;
        let cy = self.y + 2;
        px >= cx && px < cx + CLOSE_BTN_SIZE as i32
            && py >= cy && py < cy + CLOSE_BTN_SIZE as i32
    }

    /// Draw this window into the framebuffer.
    pub fn draw(&self, fb: &mut framebuffer::Framebuffer) {
        if !self.visible { return; }

        let x = self.x;
        let y = self.y;
        let tw = self.total_width();
        let th = self.total_height();

        // Border
        fb.fill_rect(x, y, tw, th, WIN_BORDER);

        // Title bar
        let tb_bg = if self.focused { TITLE_BAR_BG } else { TITLE_BAR_INACTIVE };
        fb.fill_rect(x + BORDER_W as i32, y + BORDER_W as i32,
                      self.width, TITLE_BAR_H - BORDER_W, tb_bg);

        // Title text
        let text_x = x + BORDER_W as i32 + 6;
        let text_y = y + BORDER_W as i32 + (TITLE_BAR_H as i32 - FONT_HEIGHT as i32) / 2;
        fb.draw_string_transparent(text_x, text_y, &self.title, TITLE_BAR_FG);

        // Close button
        let cx = x + tw as i32 - CLOSE_BTN_SIZE as i32 - 2;
        let cy = y + 2;
        fb.fill_rect(cx, cy, CLOSE_BTN_SIZE, CLOSE_BTN_SIZE, CLOSE_BTN_BG);
        // Draw X
        let xc = cx + CLOSE_BTN_SIZE as i32 / 2;
        let yc = cy + CLOSE_BTN_SIZE as i32 / 2;
        for d in -4..=4i32 {
            fb.put_pixel((xc + d) as u32, (yc + d) as u32, 0xFFFFFFFF);
            fb.put_pixel((xc + d) as u32, (yc - d) as u32, 0xFFFFFFFF);
            // Thicker
            fb.put_pixel((xc + d) as u32, (yc + d + 1) as u32, 0xFFFFFFFF);
            fb.put_pixel((xc + d) as u32, (yc - d + 1) as u32, 0xFFFFFFFF);
        }

        // Client area
        let client_x = x + BORDER_W as i32;
        let client_y = y + TITLE_BAR_H as i32;
        fb.fill_rect(client_x, client_y, self.width, self.height, WIN_BG);

        // Draw pixel buffer if present, otherwise draw text content
        if let Some(ref pixels) = self.pixel_buffer {
            let pw = self.pixel_width;
            let ph = self.pixel_height;
            let draw_w = pw.min(self.width);
            let draw_h = ph.min(self.height);
            for row in 0..draw_h {
                for col in 0..draw_w {
                    let idx = (row * pw + col) as usize;
                    if idx < pixels.len() {
                        fb.put_pixel((client_x + col as i32) as u32,
                                     (client_y + row as i32) as u32, pixels[idx]);
                    }
                }
            }
        } else if !self.content.is_empty() {
            let tx = client_x + 8;
            let mut ty = client_y + 8;
            for line in self.content.split('\n') {
                if ty + FONT_HEIGHT as i32 > client_y + self.height as i32 { break; }
                fb.draw_string_transparent(tx, ty, line, 0xFF222222);
                ty += FONT_HEIGHT as i32 + 2;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Window Manager
// ---------------------------------------------------------------------------
pub struct WindowManager {
    pub windows: Vec<Window>,
    next_id: u32,
    drag: Option<DragState>,
}

struct DragState {
    win_id: u32,
    offset_x: i32,
    offset_y: i32,
}

/// Global window manager instance.
pub static WM: IrqMutex<WindowManager> = IrqMutex::new(WindowManager::new());

impl WindowManager {
    pub const fn new() -> Self {
        WindowManager {
            windows: Vec::new(),
            next_id: 1,
            drag: None,
        }
    }

    /// Create a new window. Returns its id.
    pub fn create_window(&mut self, title: &str, x: i32, y: i32, w: u32, h: u32) -> u32 {
        self.create_window_owned(title, x, y, w, h, 0)
    }

    /// Create a window owned by a specific process.
    pub fn create_window_owned(&mut self, title: &str, x: i32, y: i32, w: u32, h: u32, owner_pid: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.windows.push(Window {
            id,
            title: String::from(title),
            x, y,
            width: w,
            height: h,
            visible: true,
            focused: true,
            content: String::new(),
            pixel_buffer: None,
            pixel_width: 0,
            pixel_height: 0,
            owner_pid,
            event_queue: VecDeque::new(),
        });
        // Focus this window
        self.focus(id);
        id
    }

    /// Set content text for a window.
    pub fn set_content(&mut self, id: u32, text: &str) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.content = String::from(text);
        }
    }

    /// Set a pixel buffer for a window's client area.
    pub fn set_pixel_buffer(&mut self, id: u32, pixels: Vec<u32>, width: u32, height: u32) {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == id) {
            win.pixel_buffer = Some(pixels);
            win.pixel_width = width;
            win.pixel_height = height;
        }
    }

    /// Close (remove) a window by id.
    pub fn close_window(&mut self, id: u32) {
        self.windows.retain(|w| w.id != id);
        if let Some(drag) = &self.drag {
            if drag.win_id == id { self.drag = None; }
        }
    }

    /// Bring window to front and focus it.
    pub fn focus(&mut self, id: u32) {
        for w in self.windows.iter_mut() {
            w.focused = w.id == id;
        }
        // Move to end (top of z-order)
        if let Some(pos) = self.windows.iter().position(|w| w.id == id) {
            let win = self.windows.remove(pos);
            self.windows.push(win);
        }
    }

    /// Process a GUI event, returns true if the event was consumed by a window.
    pub fn handle_event(&mut self, ev: &GuiEvent) -> bool {
        match ev {
            GuiEvent::MouseDown { x, y, button: 0 } => {
                // Find topmost window hit (reverse order = top of z-order)
                let mut hit_id = None;
                let mut close_id = None;
                for win in self.windows.iter().rev() {
                    if !win.visible { continue; }
                    if win.hit_test(*x, *y) {
                        hit_id = Some(win.id);
                        if win.hit_close(*x, *y) {
                            close_id = Some(win.id);
                        }
                        break;
                    }
                }
                if let Some(cid) = close_id {
                    self.close_window(cid);
                    return true;
                }
                if let Some(wid) = hit_id {
                    self.focus(wid);
                    // Check if on title bar → start drag
                    if let Some(win) = self.windows.iter().find(|w| w.id == wid) {
                        if win.hit_title_bar(*x, *y) {
                            self.drag = Some(DragState {
                                win_id: wid,
                                offset_x: *x - win.x,
                                offset_y: *y - win.y,
                            });
                        }
                    }
                    return true;
                }
                false
            }
            GuiEvent::MouseUp { button: 0, .. } => {
                if self.drag.is_some() {
                    self.drag = None;
                    return true;
                }
                false
            }
            GuiEvent::MouseMove { x, y } => {
                if let Some(drag) = &self.drag {
                    let wid = drag.win_id;
                    let ox = drag.offset_x;
                    let oy = drag.offset_y;
                    if let Some(win) = self.windows.iter_mut().find(|w| w.id == wid) {
                        win.x = *x - ox;
                        win.y = *y - oy;
                        // Clamp so title bar stays on screen
                        win.x = win.x.max(-(win.total_width() as i32) + 40);
                        win.y = win.y.max(0);
                        win.x = win.x.min(SCREEN_WIDTH as i32 - 40);
                        win.y = win.y.min(SCREEN_HEIGHT as i32 - TITLE_BAR_H as i32);
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Draw all windows (bottom to top).
    pub fn draw_all(&self, fb: &mut framebuffer::Framebuffer) {
        for win in &self.windows {
            win.draw(fb);
        }
    }

    /// Number of open windows.
    pub fn count(&self) -> usize {
        self.windows.len()
    }

    /// Close all windows owned by a specific process.
    pub fn close_windows_for_pid(&mut self, pid: u32) {
        self.windows.retain(|w| w.owner_pid != pid);
    }

    /// Poll an event from a window's event queue.
    pub fn poll_event(&mut self, win_id: u32) -> Option<GuiEvent> {
        if let Some(win) = self.windows.iter_mut().find(|w| w.id == win_id) {
            win.event_queue.pop_front()
        } else {
            None
        }
    }
}
