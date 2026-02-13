// Text Console
//
// Provides a simple text console on top of the framebuffer.
// This is CantayaOS's equivalent of the boot video driver (Bootvid.dll in Windows)
// that displays text during early boot before the full GUI is available.
//
// Features:
//   - Fixed-width character grid using the bitmap font
//   - Automatic line wrapping and scrolling
//   - Foreground and background colors
//   - Works without memory allocation (uses a fixed-size screen buffer)
//
// The console divides the framebuffer into a grid of character cells.
// For a 1024x768 framebuffer with 8x16 font: 128 columns × 48 rows.

use super::font::{self, CHAR_HEIGHT, CHAR_WIDTH};
use super::framebuffer::{Color, FRAMEBUFFER};
use spin::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};

extern crate alloc;
use alloc::string::String;

/// Maximum console dimensions (columns × rows)
/// These are generous limits; actual dimensions depend on framebuffer resolution.
const MAX_COLS: usize = 256;
const MAX_ROWS: usize = 128;

/// Console state
struct Console {
    /// Current cursor column (0-based)
    col: usize,
    /// Current cursor row (0-based)
    row: usize,
    /// Number of character columns that fit on screen
    cols: usize,
    /// Number of character rows available for text (excludes status bar)
    rows: usize,
    /// Total rows on screen (including status bar row)
    total_rows: usize,
    /// Text foreground color
    fg_color: Color,
    /// Text background color
    bg_color: Color,
    /// Whether the console is initialized
    initialized: bool,
}

impl Console {
    const fn new() -> Self {
        Self {
            col: 0,
            row: 0,
            cols: 0,
            rows: 0,
            total_rows: 0,
            fg_color: Color::WHITE,
            bg_color: Color::TEAL,
            initialized: false,
        }
    }
}

static CONSOLE: Mutex<Console> = Mutex::new(Console::new());

/// Output capture mode — when active, print/println append to a buffer
/// instead of (or in addition to) writing to the framebuffer.
static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_BUFFER: Mutex<String> = Mutex::new(String::new());

/// Start capturing console output into a string buffer.
/// While capture is active, output still goes to screen AND to the buffer.
pub fn start_capture() {
    CAPTURE_BUFFER.lock().clear();
    CAPTURE_ACTIVE.store(true, Ordering::Release);
}

/// Stop capturing and return the captured output.
pub fn stop_capture() -> String {
    CAPTURE_ACTIVE.store(false, Ordering::Release);
    let mut buf = CAPTURE_BUFFER.lock();
    let result = buf.clone();
    buf.clear();
    result
}

/// Check if capture mode is active.
pub fn is_capturing() -> bool {
    CAPTURE_ACTIVE.load(Ordering::Acquire)
}

/// Initialize the text console based on the current framebuffer dimensions.
pub fn init() {
    let fb = FRAMEBUFFER.lock();
    let mut console = CONSOLE.lock();

    console.cols = (fb.width / CHAR_WIDTH).min(MAX_COLS as u32) as usize;
    let total = (fb.height / CHAR_HEIGHT).min(MAX_ROWS as u32) as usize;
    console.total_rows = total;
    // Reserve the last row for the status bar
    console.rows = if total > 1 { total - 1 } else { total };
    console.col = 0;
    console.row = 0;
    console.initialized = true;

    log::info!("Console: {}x{} characters ({} text + 1 status bar)", console.cols, total, console.rows);
}

/// Draw a single character at the given grid position.
fn draw_char(col: usize, row: usize, c: char, fg: Color, bg: Color) {
    let mut fb = FRAMEBUFFER.lock();
    let bitmap = font::get_char_bitmap(c);

    let x_start = col as u32 * CHAR_WIDTH;
    let y_start = row as u32 * CHAR_HEIGHT;

    for (dy, &row_bits) in bitmap.iter().enumerate() {
        for dx in 0..8 {
            let pixel_set = (row_bits >> (7 - dx)) & 1 != 0;
            let color = if pixel_set { fg } else { bg };
            fb.put_pixel(x_start + dx as u32, y_start + dy as u32, color);
        }
    }
}

/// Scroll the console up by one line.
///
/// Copies all pixel rows up by one character height (CHAR_HEIGHT pixels),
/// then clears the bottom row. Uses raw pointer memcpy for performance.
/// Works on the draw target (back buffer if available).
fn scroll_up() {
    let mut fb = FRAMEBUFFER.lock();

    // Use the draw target (back buffer or hardware FB)
    let target = fb.draw_target();

    // Copy each scanline up by CHAR_HEIGHT pixels
    let stride_bytes = fb.stride as usize * 4; // bytes per scanline
    let copy_height = fb.height - CHAR_HEIGHT; // number of scanlines to copy

    for y in 0..copy_height {
        let src_y = y + CHAR_HEIGHT;
        let src = (target as usize + src_y as usize * stride_bytes) as *const u8;
        let dst = (target as usize + y as usize * stride_bytes) as *mut u8;
        let copy_bytes = fb.width as usize * 4; // only copy visible pixels

        unsafe {
            core::ptr::copy(src, dst, copy_bytes);
        }
    }

    // Clear the last character row
    let console = CONSOLE.lock();
    let last_row_y = (console.rows - 1) as u32 * CHAR_HEIGHT;
    let w = fb.width;
    fb.fill_rect(0, last_row_y, w, CHAR_HEIGHT, console.bg_color);
    fb.present();
}

/// Write a single character to the console at the current cursor position.
fn write_char(c: char) {
    // Phase 1: Handle the character (update cursor, draw if printable)
    let draw_info: Option<(usize, usize, char, Color, Color)>;

    {
        let mut console = CONSOLE.lock();
        if !console.initialized {
            return;
        }

        match c {
            '\n' => {
                console.col = 0;
                console.row += 1;
                draw_info = None;
            }
            '\r' => {
                console.col = 0;
                draw_info = None;
            }
            '\t' => {
                let tab_stop = (console.col + 4) & !3;
                console.col = tab_stop.min(console.cols - 1);
                draw_info = None;
            }
            ch => {
                let col = console.col;
                let row = console.row;
                let fg = console.fg_color;
                let bg = console.bg_color;
                let cols = console.cols;

                // Advance cursor now (before drawing)
                console.col += 1;
                if console.col >= cols {
                    console.col = 0;
                    console.row += 1;
                }

                draw_info = Some((col, row, ch, fg, bg));
            }
        }
    } // CONSOLE lock dropped for ALL cases

    // Phase 2: Draw character if printable (requires FRAMEBUFFER lock)
    if let Some((col, row, ch, fg, bg)) = draw_info {
        draw_char(col, row, ch, fg, bg);
    }

    // Phase 3: Handle scrolling if cursor moved past the last row
    let mut console = CONSOLE.lock();
    if console.row >= console.rows {
        console.row = console.rows - 1;
        drop(console);

        // Scroll the entire framebuffer up by one line
        scroll_up();
    }
}

/// Print a string to the console.
pub fn print(s: &str) {
    // If capturing, append to capture buffer
    if CAPTURE_ACTIVE.load(Ordering::Acquire) {
        CAPTURE_BUFFER.lock().push_str(s);
    }
    for c in s.chars() {
        write_char(c);
    }
    // Present immediately so every print is visible
    let mut fb = FRAMEBUFFER.lock();
    fb.present();
}

/// Print a string followed by a newline.
pub fn println(s: &str) {
    // If capturing, append to capture buffer with newline
    if CAPTURE_ACTIVE.load(Ordering::Acquire) {
        let mut buf = CAPTURE_BUFFER.lock();
        buf.push_str(s);
        buf.push('\n');
    }
    for c in s.chars() {
        write_char(c);
    }
    write_char('\n');
    let mut fb = FRAMEBUFFER.lock();
    fb.present();
}

/// Handle a backspace: erase the character behind the cursor.
pub fn backspace() {
    let mut console = CONSOLE.lock();
    if !console.initialized {
        return;
    }

    if console.col > 0 {
        console.col -= 1;
        let col = console.col;
        let row = console.row;
        let bg = console.bg_color;
        drop(console);
        draw_char(col, row, ' ', bg, bg);
        // Present immediately so the backspace is visible
        let mut fb = FRAMEBUFFER.lock();
        fb.present();
    }
}

/// Clear the entire screen and reset cursor to top-left.
pub fn clear() {
    {
        let mut fb = FRAMEBUFFER.lock();
        let console = CONSOLE.lock();
        fb.clear(console.bg_color);
        fb.present();
    }
    let mut console = CONSOLE.lock();
    console.col = 0;
    console.row = 0;
}

/// Set the console text foreground color (RGB).
pub fn set_color(r: u8, g: u8, b: u8) {
    let mut console = CONSOLE.lock();
    console.fg_color = Color::rgb(r, g, b);
}

/// Set the console background color (RGB).
pub fn set_bg_color(r: u8, g: u8, b: u8) {
    let mut console = CONSOLE.lock();
    console.bg_color = Color::rgb(r, g, b);
}

/// Get the current cursor position (col, row).
pub fn cursor_position() -> (usize, usize) {
    let console = CONSOLE.lock();
    (console.col, console.row)
}

/// Get the console dimensions (cols, rows).
pub fn dimensions() -> (usize, usize) {
    let console = CONSOLE.lock();
    (console.cols, console.rows)
}

/// Draw or erase a block cursor at the current cursor position.
///
/// When `visible` is true, draws a solid block in the foreground color.
/// When `visible` is false, erases the cursor by filling with background color.
/// Calls present() to push the change to the hardware framebuffer immediately.
pub fn draw_cursor(visible: bool) {
    let (col, row, fg, bg);
    {
        let console = CONSOLE.lock();
        if !console.initialized { return; }
        col = console.col;
        row = console.row;
        // Don't draw cursor past the visible area
        if row >= console.rows || col >= console.cols { return; }
        fg = console.fg_color;
        bg = console.bg_color;
    }

    if visible {
        // Draw a block cursor (solid rectangle)
        let mut fb = FRAMEBUFFER.lock();
        let x = col as u32 * CHAR_WIDTH;
        let y = row as u32 * CHAR_HEIGHT;
        // Draw a slightly shorter block (bottom 3 rows of the char cell = underscore style)
        fb.fill_rect(x, y + CHAR_HEIGHT - 3, CHAR_WIDTH, 3, fg);
        fb.present();
    } else {
        // Erase cursor
        let mut fb = FRAMEBUFFER.lock();
        let x = col as u32 * CHAR_WIDTH;
        let y = row as u32 * CHAR_HEIGHT;
        fb.fill_rect(x, y + CHAR_HEIGHT - 3, CHAR_WIDTH, 3, bg);
        fb.present();
    }
}
/// Draw the status bar on the reserved bottom row of the screen.
///
/// Renders `text` in a contrasting color bar at the very last character row.
/// This row is excluded from the normal text area, so it won't be overwritten
/// by scrolling or regular print operations.
pub fn draw_status_bar(text: &str) {
    let (cols, bar_row, fg, bg);
    {
        let console = CONSOLE.lock();
        if !console.initialized { return; }
        cols = console.cols;
        bar_row = console.total_rows.saturating_sub(1);
        // Status bar colors: bright white text on dark slate background
        fg = Color::rgb(0xFF, 0xFF, 0xFF);
        bg = Color::rgb(0x20, 0x30, 0x50);
        let _ = &console; // keep borrow clear
    }

    // Fill the entire status bar row with the background color
    {
        let mut fb = FRAMEBUFFER.lock();
        let y = bar_row as u32 * CHAR_HEIGHT;
        let w = fb.width;
        fb.fill_rect(0, y, w, CHAR_HEIGHT, bg);
    }

    // Draw each character
    for (i, ch) in text.chars().enumerate() {
        if i >= cols { break; }
        draw_char(i, bar_row, ch, fg, bg);
    }

    // Present the back buffer to make the status bar visible
    let mut fb = FRAMEBUFFER.lock();
    fb.present();
}
/// Implement core::fmt::Write for use with write!/writeln! macros
pub struct ConsoleWriter;

impl core::fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        print(s);
        Ok(())
    }
}

/// Print formatted text to the console
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        $crate::graphics::console::ConsoleWriter.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

/// Print formatted text to the console with a newline
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)));
}
