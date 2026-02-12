//! Framebuffer driver — QEMU ramfb over fw-cfg
//!
//! Allocates a contiguous physical region for an 800×600 32-bit framebuffer,
//! registers it with QEMU via the `etc/ramfb` fw-cfg file, and exposes pixel
//! drawing primitives plus an 8×16 bitmap font renderer.
//!
//! Double-buffered: all drawing goes to a back buffer; `flush()` copies it
//! to the front (hardware) buffer atomically.

use crate::sync::IrqMutex;
use crate::mm::{physical, virtual_mem};
use crate::arch::mmu::PageFlags;
use crate::drivers::fwcfg;

pub const SCREEN_WIDTH: usize  = 800;
pub const SCREEN_HEIGHT: usize = 600;
pub const BPP: usize           = 4; // bytes per pixel (XRGB8888)
pub const STRIDE: usize        = SCREEN_WIDTH * BPP;
pub const FB_SIZE: usize       = STRIDE * SCREEN_HEIGHT;

/// Font dimensions
pub const FONT_WIDTH: usize  = 8;
pub const FONT_HEIGHT: usize = 16;

/// DRM fourcc for XRGB8888
const DRM_FORMAT_XRGB8888: u32 = 0x34325258; // 'XR24'

/// RAMFBCfg structure written to fw-cfg (all fields big-endian)
/// Packed to match QEMU's 28-byte QEMU_PACKED layout exactly.
#[repr(C, packed)]
struct RamfbCfg {
    addr:    u64,
    fourcc:  u32,
    flags:   u32,
    width:   u32,
    height:  u32,
    stride:  u32,
}

/// Framebuffer state
pub struct Framebuffer {
    front: usize,         // physical address of front buffer (QEMU reads this)
    back:  *mut u32,      // pointer to back buffer (we draw here)
    ready: bool,
}

unsafe impl Send for Framebuffer {}

impl Framebuffer {
    const fn new() -> Self {
        Framebuffer { front: 0, back: core::ptr::null_mut(), ready: false }
    }

    // -----------------------------------------------------------------------
    // Methods — operate directly on self (caller holds the lock)
    // -----------------------------------------------------------------------

    /// Put a single pixel at (x, y).
    #[inline]
    pub fn put_pixel(&mut self, x: u32, y: u32, color: u32) {
        let x = x as usize;
        let y = y as usize;
        if self.ready && x < SCREEN_WIDTH && y < SCREEN_HEIGHT {
            unsafe { *self.back.add(y * SCREEN_WIDTH + x) = color; }
        }
    }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: u32, h: u32, color: u32) {
        if !self.ready { return; }
        let x1 = (x.max(0) as usize).min(SCREEN_WIDTH);
        let y1 = (y.max(0) as usize).min(SCREEN_HEIGHT);
        let x2 = ((x + w as i32).max(0) as usize).min(SCREEN_WIDTH);
        let y2 = ((y + h as i32).max(0) as usize).min(SCREEN_HEIGHT);
        unsafe {
            for row in y1..y2 {
                let base = self.back.add(row * SCREEN_WIDTH);
                for col in x1..x2 {
                    *base.add(col) = color;
                }
            }
        }
    }

    /// Horizontal line.
    pub fn draw_hline(&mut self, x: i32, y: i32, w: u32, color: u32) {
        self.fill_rect(x, y, w, 1, color);
    }

    /// Vertical line.
    pub fn draw_vline(&mut self, x: i32, y: i32, h: u32, color: u32) {
        self.fill_rect(x, y, 1, h, color);
    }

    /// Clear entire back buffer.
    pub fn clear(&mut self, color: u32) {
        if !self.ready { return; }
        unsafe {
            let buf = core::slice::from_raw_parts_mut(self.back, SCREEN_WIDTH * SCREEN_HEIGHT);
            for px in buf.iter_mut() { *px = color; }
        }
    }

    /// Copy back buffer to front buffer.
    pub fn flush(&self) {
        if !self.ready { return; }
        unsafe {
            core::ptr::copy_nonoverlapping(
                self.back as *const u8,
                self.front as *mut u8,
                FB_SIZE,
            );
        }
    }

    /// Draw a character with transparent background.
    pub fn draw_char_transparent(&mut self, px: i32, py: i32, ch: char, fg: u32) {
        if !self.ready { return; }
        let idx = (ch as u32) as usize;
        let glyph = if idx < 256 { &FONT_8X16[idx * FONT_HEIGHT..(idx + 1) * FONT_HEIGHT] }
                    else { &FONT_8X16[0..FONT_HEIGHT] };
        unsafe {
            for row in 0..FONT_HEIGHT {
                let sy = py as usize + row;
                if sy >= SCREEN_HEIGHT { break; }
                let bits = glyph[row];
                for col in 0..FONT_WIDTH {
                    if (bits >> (7 - col)) & 1 != 0 {
                        let sx = px as usize + col;
                        if sx < SCREEN_WIDTH {
                            *self.back.add(sy * SCREEN_WIDTH + sx) = fg;
                        }
                    }
                }
            }
        }
    }

    /// Draw a string with transparent background.
    pub fn draw_string_transparent(&mut self, px: i32, py: i32, text: &str, fg: u32) {
        let mut x = px;
        for ch in text.chars() {
            if ch == '\n' { break; }
            if (x as usize) + FONT_WIDTH > SCREEN_WIDTH { break; }
            self.draw_char_transparent(x, py, ch, fg);
            x += FONT_WIDTH as i32;
        }
    }
}

pub static FB: IrqMutex<Framebuffer> = IrqMutex::new(Framebuffer::new());

/// Initialise the framebuffer via QEMU ramfb.
///
/// Returns `true` on success, `false` if ramfb is not available.
pub fn init() -> bool {
    // 1. Find the ramfb fw-cfg file
    let (selector, _expected_size) = match fwcfg::find_file("etc/ramfb") {
        Some(s) => s,
        None => {
            crate::kprintln!("[fb] ramfb not found in fw-cfg — no graphical output");
            return false;
        }
    };

    // 2. Allocate contiguous physical frames for the front buffer
    let pages = (FB_SIZE + 0xFFF) / 0x1000;
    let front_phys = match physical::alloc_contiguous_frames(pages) {
        Some(a) => a,
        None => {
            crate::kprintln!("[fb] failed to allocate {} pages for framebuffer", pages);
            return false;
        }
    };

    // Identity-map the front buffer as Normal WB so the CPU can write to it
    // AND QEMU can read it (QEMU reads guest physical).
    for i in 0..pages {
        virtual_mem::map_page(
            front_phys + i * 0x1000,
            front_phys + i * 0x1000,
            PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                | PageFlags::INNER_SHAREABLE | PageFlags::ATTR_NORMAL_WB,
        );
    }

    // Zero the front buffer
    unsafe {
        core::ptr::write_bytes(front_phys as *mut u8, 0, FB_SIZE);
    }

    // 3. Tell QEMU about our framebuffer via fw-cfg DMA write
    let cfg = RamfbCfg {
        addr:   (front_phys as u64).to_be(),
        fourcc: DRM_FORMAT_XRGB8888.to_be(),
        flags:  0,
        width:  (SCREEN_WIDTH as u32).to_be(),
        height: (SCREEN_HEIGHT as u32).to_be(),
        stride: (STRIDE as u32).to_be(),
    };

    let cfg_bytes = unsafe {
        core::slice::from_raw_parts(
            &cfg as *const RamfbCfg as *const u8,
            core::mem::size_of::<RamfbCfg>(),
        )
    };

    crate::kprintln!("[fb] ramfb: selector={:#06X} front_phys={:#010X} size={}",
        selector, front_phys, cfg_bytes.len());

    fwcfg::write_file(selector, cfg_bytes);

    // 4. Allocate a back buffer from the heap (same size)
    //    We use alloc_contiguous_frames again to guarantee a flat region
    let back_phys = match physical::alloc_contiguous_frames(pages) {
        Some(a) => a,
        None => {
            crate::kprintln!("[fb] failed to allocate back buffer");
            return false;
        }
    };
    for i in 0..pages {
        virtual_mem::map_page(
            back_phys + i * 0x1000,
            back_phys + i * 0x1000,
            PageFlags::VALID | PageFlags::PAGE | PageFlags::ACCESSED
                | PageFlags::INNER_SHAREABLE | PageFlags::ATTR_NORMAL_WB,
        );
    }
    unsafe {
        core::ptr::write_bytes(back_phys as *mut u8, 0, FB_SIZE);
    }

    {
        let mut fb = FB.lock();
        fb.front = front_phys;
        fb.back  = back_phys as *mut u32;
        fb.ready = true;
    }

    true
}

/// Is the framebuffer initialised?
pub fn is_ready() -> bool {
    FB.lock().ready
}

// ---------------------------------------------------------------------------
// Pixel primitives — all operate on the back buffer
// ---------------------------------------------------------------------------

/// Raw back-buffer slice (SCREEN_WIDTH * SCREEN_HEIGHT u32 pixels).
#[allow(dead_code)]
fn back_buf() -> &'static mut [u32] {
    let fb = FB.lock();
    unsafe { core::slice::from_raw_parts_mut(fb.back, SCREEN_WIDTH * SCREEN_HEIGHT) }
}

/// Put a single pixel at (x, y).
#[inline]
pub fn put_pixel(x: usize, y: usize, color: u32) {
    if x < SCREEN_WIDTH && y < SCREEN_HEIGHT {
        let fb = FB.lock();
        if fb.ready {
            unsafe {
                *fb.back.add(y * SCREEN_WIDTH + x) = color;
            }
        }
    }
}

/// Fill a rectangle.
pub fn fill_rect(x: usize, y: usize, w: usize, h: usize, color: u32) {
    let fb = FB.lock();
    if !fb.ready { return; }
    let x1 = x.min(SCREEN_WIDTH);
    let y1 = y.min(SCREEN_HEIGHT);
    let x2 = (x + w).min(SCREEN_WIDTH);
    let y2 = (y + h).min(SCREEN_HEIGHT);
    unsafe {
        for row in y1..y2 {
            let base = fb.back.add(row * SCREEN_WIDTH);
            for col in x1..x2 {
                *base.add(col) = color;
            }
        }
    }
}

/// Horizontal line.
pub fn draw_hline(x: usize, y: usize, w: usize, color: u32) {
    fill_rect(x, y, w, 1, color);
}

/// Vertical line.
pub fn draw_vline(x: usize, y: usize, h: usize, color: u32) {
    fill_rect(x, y, 1, h, color);
}

/// Clear the entire back buffer to a colour.
pub fn clear(color: u32) {
    let fb = FB.lock();
    if !fb.ready { return; }
    unsafe {
        let buf = core::slice::from_raw_parts_mut(fb.back, SCREEN_WIDTH * SCREEN_HEIGHT);
        for px in buf.iter_mut() {
            *px = color;
        }
    }
}

/// Copy the back buffer to the front buffer (makes drawing visible).
pub fn flush() {
    let fb = FB.lock();
    if !fb.ready { return; }
    unsafe {
        core::ptr::copy_nonoverlapping(
            fb.back as *const u8,
            fb.front as *mut u8,
            FB_SIZE,
        );
    }
}

/// Blit a buffer of `w * h` pixels at position (x, y).
pub fn blit(x: usize, y: usize, w: usize, h: usize, pixels: &[u32]) {
    let fb = FB.lock();
    if !fb.ready { return; }
    unsafe {
        for row in 0..h {
            let sy = y + row;
            if sy >= SCREEN_HEIGHT { break; }
            for col in 0..w {
                let sx = x + col;
                if sx >= SCREEN_WIDTH { continue; }
                let src = pixels[row * w + col];
                if src & 0xFF00_0000 != 0 { // skip fully transparent (alpha-key)
                    *fb.back.add(sy * SCREEN_WIDTH + sx) = src;
                }
            }
        }
    }
}

/// Draw a single pixel without bounds-checking (caller must ensure valid coords).
/// Useful for inner loops that have already been clipped.
#[inline]
pub unsafe fn put_pixel_unchecked(buf: *mut u32, x: usize, y: usize, color: u32) {
    *buf.add(y * SCREEN_WIDTH + x) = color;
}

/// Get the raw back buffer pointer (for compositing).
pub fn back_buffer_ptr() -> *mut u32 {
    FB.lock().back
}

// ---------------------------------------------------------------------------
// 8×16 bitmap font renderer
// ---------------------------------------------------------------------------

/// Draw a character at pixel position (px, py) using the built-in font.
pub fn draw_char(px: usize, py: usize, ch: char, fg: u32, bg: u32) {
    let idx = (ch as u32) as usize;
    let glyph = if idx < 256 { &FONT_8X16[idx * FONT_HEIGHT..(idx + 1) * FONT_HEIGHT] }
                else { &FONT_8X16[0..FONT_HEIGHT] }; // fallback to NUL glyph

    let fb = FB.lock();
    if !fb.ready { return; }
    unsafe {
        for row in 0..FONT_HEIGHT {
            let sy = py + row;
            if sy >= SCREEN_HEIGHT { break; }
            let bits = glyph[row];
            for col in 0..FONT_WIDTH {
                let sx = px + col;
                if sx >= SCREEN_WIDTH { continue; }
                let color = if (bits >> (7 - col)) & 1 != 0 { fg } else { bg };
                *fb.back.add(sy * SCREEN_WIDTH + sx) = color;
            }
        }
    }
}

/// Draw a character with transparent background (only foreground pixels drawn).
pub fn draw_char_transparent(px: usize, py: usize, ch: char, fg: u32) {
    let idx = (ch as u32) as usize;
    let glyph = if idx < 256 { &FONT_8X16[idx * FONT_HEIGHT..(idx + 1) * FONT_HEIGHT] }
                else { &FONT_8X16[0..FONT_HEIGHT] };

    let fb = FB.lock();
    if !fb.ready { return; }
    unsafe {
        for row in 0..FONT_HEIGHT {
            let sy = py + row;
            if sy >= SCREEN_HEIGHT { break; }
            let bits = glyph[row];
            for col in 0..FONT_WIDTH {
                if (bits >> (7 - col)) & 1 != 0 {
                    let sx = px + col;
                    if sx < SCREEN_WIDTH {
                        *fb.back.add(sy * SCREEN_WIDTH + sx) = fg;
                    }
                }
            }
        }
    }
}

/// Draw a string at pixel position.
pub fn draw_string(px: usize, py: usize, text: &str, fg: u32, bg: u32) {
    let mut x = px;
    for ch in text.chars() {
        if ch == '\n' { break; }
        if x + FONT_WIDTH > SCREEN_WIDTH { break; }
        draw_char(x, py, ch, fg, bg);
        x += FONT_WIDTH;
    }
}

/// Draw a string with transparent background.
pub fn draw_string_transparent(px: usize, py: usize, text: &str, fg: u32) {
    let mut x = px;
    for ch in text.chars() {
        if ch == '\n' { break; }
        if x + FONT_WIDTH > SCREEN_WIDTH { break; }
        draw_char_transparent(x, py, ch, fg);
        x += FONT_WIDTH;
    }
}

/// Measure string width in pixels.
pub fn text_width(text: &str) -> usize {
    text.len() * FONT_WIDTH
}

// ---------------------------------------------------------------------------
// Built-in 8×16 bitmap font (CP437-derived, ASCII 0–127 + blanks)
// Each glyph is 16 bytes (one byte per scan line, MSB = leftmost pixel).
// ---------------------------------------------------------------------------

#[rustfmt::skip]
static FONT_8X16: [u8; 256 * 16] = {
    let mut f = [0u8; 256 * 16];

    // We define each printable ASCII glyph inline.
    // Helper: glyph offset = char_code * 16

    // Space (32)
    // all zeros — already initialised

    // ! (33)
    f[33*16+ 1] = 0x18; f[33*16+ 2] = 0x3C; f[33*16+ 3] = 0x3C;
    f[33*16+ 4] = 0x18; f[33*16+ 5] = 0x18; f[33*16+ 6] = 0x18;
    f[33*16+ 7] = 0x18; f[33*16+ 8] = 0x00; f[33*16+ 9] = 0x18;
    f[33*16+10] = 0x18;

    // " (34)
    f[34*16+ 1] = 0x66; f[34*16+ 2] = 0x66; f[34*16+ 3] = 0x24;

    // # (35)
    f[35*16+ 2] = 0x6C; f[35*16+ 3] = 0x6C; f[35*16+ 4] = 0xFE;
    f[35*16+ 5] = 0x6C; f[35*16+ 6] = 0xFE; f[35*16+ 7] = 0x6C;
    f[35*16+ 8] = 0x6C;

    // $ (36)
    f[36*16+ 1] = 0x18; f[36*16+ 2] = 0x3E; f[36*16+ 3] = 0x60;
    f[36*16+ 4] = 0x3C; f[36*16+ 5] = 0x06; f[36*16+ 6] = 0x7C;
    f[36*16+ 7] = 0x18;

    // % (37)
    f[37*16+ 2] = 0x62; f[37*16+ 3] = 0x66; f[37*16+ 4] = 0x0C;
    f[37*16+ 5] = 0x18; f[37*16+ 6] = 0x30; f[37*16+ 7] = 0x66;
    f[37*16+ 8] = 0x46;

    // & (38)
    f[38*16+ 2] = 0x3C; f[38*16+ 3] = 0x66; f[38*16+ 4] = 0x3C;
    f[38*16+ 5] = 0x38; f[38*16+ 6] = 0x67; f[38*16+ 7] = 0x66;
    f[38*16+ 8] = 0x3F;

    // ' (39)
    f[39*16+ 1] = 0x18; f[39*16+ 2] = 0x18; f[39*16+ 3] = 0x30;

    // ( (40)
    f[40*16+ 1] = 0x0C; f[40*16+ 2] = 0x18; f[40*16+ 3] = 0x30;
    f[40*16+ 4] = 0x30; f[40*16+ 5] = 0x30; f[40*16+ 6] = 0x18;
    f[40*16+ 7] = 0x0C;

    // ) (41)
    f[41*16+ 1] = 0x30; f[41*16+ 2] = 0x18; f[41*16+ 3] = 0x0C;
    f[41*16+ 4] = 0x0C; f[41*16+ 5] = 0x0C; f[41*16+ 6] = 0x18;
    f[41*16+ 7] = 0x30;

    // * (42)
    f[42*16+ 3] = 0x66; f[42*16+ 4] = 0x3C; f[42*16+ 5] = 0xFF;
    f[42*16+ 6] = 0x3C; f[42*16+ 7] = 0x66;

    // + (43)
    f[43*16+ 4] = 0x18; f[43*16+ 5] = 0x18; f[43*16+ 6] = 0x7E;
    f[43*16+ 7] = 0x18; f[43*16+ 8] = 0x18;

    // , (44)
    f[44*16+ 9] = 0x18; f[44*16+10] = 0x18; f[44*16+11] = 0x30;

    // - (45)
    f[45*16+ 6] = 0x7E;

    // . (46)
    f[46*16+ 9] = 0x18; f[46*16+10] = 0x18;

    // / (47)
    f[47*16+ 2] = 0x06; f[47*16+ 3] = 0x0C; f[47*16+ 4] = 0x18;
    f[47*16+ 5] = 0x30; f[47*16+ 6] = 0x60;

    // 0 (48)
    f[48*16+ 2] = 0x3C; f[48*16+ 3] = 0x66; f[48*16+ 4] = 0x6E;
    f[48*16+ 5] = 0x76; f[48*16+ 6] = 0x66; f[48*16+ 7] = 0x66;
    f[48*16+ 8] = 0x3C;

    // 1 (49)
    f[49*16+ 2] = 0x18; f[49*16+ 3] = 0x38; f[49*16+ 4] = 0x18;
    f[49*16+ 5] = 0x18; f[49*16+ 6] = 0x18; f[49*16+ 7] = 0x18;
    f[49*16+ 8] = 0x7E;

    // 2 (50)
    f[50*16+ 2] = 0x3C; f[50*16+ 3] = 0x66; f[50*16+ 4] = 0x06;
    f[50*16+ 5] = 0x1C; f[50*16+ 6] = 0x30; f[50*16+ 7] = 0x60;
    f[50*16+ 8] = 0x7E;

    // 3 (51)
    f[51*16+ 2] = 0x3C; f[51*16+ 3] = 0x66; f[51*16+ 4] = 0x06;
    f[51*16+ 5] = 0x1C; f[51*16+ 6] = 0x06; f[51*16+ 7] = 0x66;
    f[51*16+ 8] = 0x3C;

    // 4 (52)
    f[52*16+ 2] = 0x0C; f[52*16+ 3] = 0x1C; f[52*16+ 4] = 0x3C;
    f[52*16+ 5] = 0x6C; f[52*16+ 6] = 0x7E; f[52*16+ 7] = 0x0C;
    f[52*16+ 8] = 0x0C;

    // 5 (53)
    f[53*16+ 2] = 0x7E; f[53*16+ 3] = 0x60; f[53*16+ 4] = 0x7C;
    f[53*16+ 5] = 0x06; f[53*16+ 6] = 0x06; f[53*16+ 7] = 0x66;
    f[53*16+ 8] = 0x3C;

    // 6 (54)
    f[54*16+ 2] = 0x1C; f[54*16+ 3] = 0x30; f[54*16+ 4] = 0x60;
    f[54*16+ 5] = 0x7C; f[54*16+ 6] = 0x66; f[54*16+ 7] = 0x66;
    f[54*16+ 8] = 0x3C;

    // 7 (55)
    f[55*16+ 2] = 0x7E; f[55*16+ 3] = 0x06; f[55*16+ 4] = 0x0C;
    f[55*16+ 5] = 0x18; f[55*16+ 6] = 0x30; f[55*16+ 7] = 0x30;
    f[55*16+ 8] = 0x30;

    // 8 (56)
    f[56*16+ 2] = 0x3C; f[56*16+ 3] = 0x66; f[56*16+ 4] = 0x66;
    f[56*16+ 5] = 0x3C; f[56*16+ 6] = 0x66; f[56*16+ 7] = 0x66;
    f[56*16+ 8] = 0x3C;

    // 9 (57)
    f[57*16+ 2] = 0x3C; f[57*16+ 3] = 0x66; f[57*16+ 4] = 0x66;
    f[57*16+ 5] = 0x3E; f[57*16+ 6] = 0x06; f[57*16+ 7] = 0x0C;
    f[57*16+ 8] = 0x38;

    // : (58)
    f[58*16+ 4] = 0x18; f[58*16+ 5] = 0x18; f[58*16+ 8] = 0x18;
    f[58*16+ 9] = 0x18;

    // ; (59)
    f[59*16+ 4] = 0x18; f[59*16+ 5] = 0x18; f[59*16+ 8] = 0x18;
    f[59*16+ 9] = 0x18; f[59*16+10] = 0x30;

    // < (60)
    f[60*16+ 3] = 0x06; f[60*16+ 4] = 0x0C; f[60*16+ 5] = 0x18;
    f[60*16+ 6] = 0x30; f[60*16+ 7] = 0x18; f[60*16+ 8] = 0x0C;
    f[60*16+ 9] = 0x06;

    // = (61)
    f[61*16+ 5] = 0x7E; f[61*16+ 7] = 0x7E;

    // > (62)
    f[62*16+ 3] = 0x60; f[62*16+ 4] = 0x30; f[62*16+ 5] = 0x18;
    f[62*16+ 6] = 0x0C; f[62*16+ 7] = 0x18; f[62*16+ 8] = 0x30;
    f[62*16+ 9] = 0x60;

    // ? (63)
    f[63*16+ 2] = 0x3C; f[63*16+ 3] = 0x66; f[63*16+ 4] = 0x06;
    f[63*16+ 5] = 0x0C; f[63*16+ 6] = 0x18; f[63*16+ 7] = 0x00;
    f[63*16+ 8] = 0x18;

    // @ (64)
    f[64*16+ 2] = 0x3C; f[64*16+ 3] = 0x66; f[64*16+ 4] = 0x6E;
    f[64*16+ 5] = 0x6A; f[64*16+ 6] = 0x6E; f[64*16+ 7] = 0x60;
    f[64*16+ 8] = 0x3E;

    // A-Z (65-90)
    f[65*16+ 2] = 0x18; f[65*16+ 3] = 0x3C; f[65*16+ 4] = 0x66;
    f[65*16+ 5] = 0x66; f[65*16+ 6] = 0x7E; f[65*16+ 7] = 0x66;
    f[65*16+ 8] = 0x66;

    f[66*16+ 2] = 0x7C; f[66*16+ 3] = 0x66; f[66*16+ 4] = 0x66;
    f[66*16+ 5] = 0x7C; f[66*16+ 6] = 0x66; f[66*16+ 7] = 0x66;
    f[66*16+ 8] = 0x7C;

    f[67*16+ 2] = 0x3C; f[67*16+ 3] = 0x66; f[67*16+ 4] = 0x60;
    f[67*16+ 5] = 0x60; f[67*16+ 6] = 0x60; f[67*16+ 7] = 0x66;
    f[67*16+ 8] = 0x3C;

    f[68*16+ 2] = 0x7C; f[68*16+ 3] = 0x66; f[68*16+ 4] = 0x66;
    f[68*16+ 5] = 0x66; f[68*16+ 6] = 0x66; f[68*16+ 7] = 0x66;
    f[68*16+ 8] = 0x7C;

    f[69*16+ 2] = 0x7E; f[69*16+ 3] = 0x60; f[69*16+ 4] = 0x60;
    f[69*16+ 5] = 0x78; f[69*16+ 6] = 0x60; f[69*16+ 7] = 0x60;
    f[69*16+ 8] = 0x7E;

    f[70*16+ 2] = 0x7E; f[70*16+ 3] = 0x60; f[70*16+ 4] = 0x60;
    f[70*16+ 5] = 0x78; f[70*16+ 6] = 0x60; f[70*16+ 7] = 0x60;
    f[70*16+ 8] = 0x60;

    f[71*16+ 2] = 0x3C; f[71*16+ 3] = 0x66; f[71*16+ 4] = 0x60;
    f[71*16+ 5] = 0x6E; f[71*16+ 6] = 0x66; f[71*16+ 7] = 0x66;
    f[71*16+ 8] = 0x3E;

    f[72*16+ 2] = 0x66; f[72*16+ 3] = 0x66; f[72*16+ 4] = 0x66;
    f[72*16+ 5] = 0x7E; f[72*16+ 6] = 0x66; f[72*16+ 7] = 0x66;
    f[72*16+ 8] = 0x66;

    f[73*16+ 2] = 0x3C; f[73*16+ 3] = 0x18; f[73*16+ 4] = 0x18;
    f[73*16+ 5] = 0x18; f[73*16+ 6] = 0x18; f[73*16+ 7] = 0x18;
    f[73*16+ 8] = 0x3C;

    f[74*16+ 2] = 0x06; f[74*16+ 3] = 0x06; f[74*16+ 4] = 0x06;
    f[74*16+ 5] = 0x06; f[74*16+ 6] = 0x66; f[74*16+ 7] = 0x66;
    f[74*16+ 8] = 0x3C;

    f[75*16+ 2] = 0x66; f[75*16+ 3] = 0x6C; f[75*16+ 4] = 0x78;
    f[75*16+ 5] = 0x70; f[75*16+ 6] = 0x78; f[75*16+ 7] = 0x6C;
    f[75*16+ 8] = 0x66;

    f[76*16+ 2] = 0x60; f[76*16+ 3] = 0x60; f[76*16+ 4] = 0x60;
    f[76*16+ 5] = 0x60; f[76*16+ 6] = 0x60; f[76*16+ 7] = 0x60;
    f[76*16+ 8] = 0x7E;

    f[77*16+ 2] = 0xC6; f[77*16+ 3] = 0xEE; f[77*16+ 4] = 0xFE;
    f[77*16+ 5] = 0xD6; f[77*16+ 6] = 0xC6; f[77*16+ 7] = 0xC6;
    f[77*16+ 8] = 0xC6;

    f[78*16+ 2] = 0x66; f[78*16+ 3] = 0x76; f[78*16+ 4] = 0x7E;
    f[78*16+ 5] = 0x6E; f[78*16+ 6] = 0x66; f[78*16+ 7] = 0x66;
    f[78*16+ 8] = 0x66;

    f[79*16+ 2] = 0x3C; f[79*16+ 3] = 0x66; f[79*16+ 4] = 0x66;
    f[79*16+ 5] = 0x66; f[79*16+ 6] = 0x66; f[79*16+ 7] = 0x66;
    f[79*16+ 8] = 0x3C;

    f[80*16+ 2] = 0x7C; f[80*16+ 3] = 0x66; f[80*16+ 4] = 0x66;
    f[80*16+ 5] = 0x7C; f[80*16+ 6] = 0x60; f[80*16+ 7] = 0x60;
    f[80*16+ 8] = 0x60;

    f[81*16+ 2] = 0x3C; f[81*16+ 3] = 0x66; f[81*16+ 4] = 0x66;
    f[81*16+ 5] = 0x66; f[81*16+ 6] = 0x6A; f[81*16+ 7] = 0x6C;
    f[81*16+ 8] = 0x36;

    f[82*16+ 2] = 0x7C; f[82*16+ 3] = 0x66; f[82*16+ 4] = 0x66;
    f[82*16+ 5] = 0x7C; f[82*16+ 6] = 0x6C; f[82*16+ 7] = 0x66;
    f[82*16+ 8] = 0x66;

    f[83*16+ 2] = 0x3C; f[83*16+ 3] = 0x66; f[83*16+ 4] = 0x60;
    f[83*16+ 5] = 0x3C; f[83*16+ 6] = 0x06; f[83*16+ 7] = 0x66;
    f[83*16+ 8] = 0x3C;

    f[84*16+ 2] = 0x7E; f[84*16+ 3] = 0x18; f[84*16+ 4] = 0x18;
    f[84*16+ 5] = 0x18; f[84*16+ 6] = 0x18; f[84*16+ 7] = 0x18;
    f[84*16+ 8] = 0x18;

    f[85*16+ 2] = 0x66; f[85*16+ 3] = 0x66; f[85*16+ 4] = 0x66;
    f[85*16+ 5] = 0x66; f[85*16+ 6] = 0x66; f[85*16+ 7] = 0x66;
    f[85*16+ 8] = 0x3C;

    f[86*16+ 2] = 0x66; f[86*16+ 3] = 0x66; f[86*16+ 4] = 0x66;
    f[86*16+ 5] = 0x66; f[86*16+ 6] = 0x66; f[86*16+ 7] = 0x3C;
    f[86*16+ 8] = 0x18;

    f[87*16+ 2] = 0xC6; f[87*16+ 3] = 0xC6; f[87*16+ 4] = 0xC6;
    f[87*16+ 5] = 0xD6; f[87*16+ 6] = 0xFE; f[87*16+ 7] = 0xEE;
    f[87*16+ 8] = 0xC6;

    f[88*16+ 2] = 0x66; f[88*16+ 3] = 0x66; f[88*16+ 4] = 0x3C;
    f[88*16+ 5] = 0x18; f[88*16+ 6] = 0x3C; f[88*16+ 7] = 0x66;
    f[88*16+ 8] = 0x66;

    f[89*16+ 2] = 0x66; f[89*16+ 3] = 0x66; f[89*16+ 4] = 0x66;
    f[89*16+ 5] = 0x3C; f[89*16+ 6] = 0x18; f[89*16+ 7] = 0x18;
    f[89*16+ 8] = 0x18;

    f[90*16+ 2] = 0x7E; f[90*16+ 3] = 0x06; f[90*16+ 4] = 0x0C;
    f[90*16+ 5] = 0x18; f[90*16+ 6] = 0x30; f[90*16+ 7] = 0x60;
    f[90*16+ 8] = 0x7E;

    // [ (91)
    f[91*16+ 1] = 0x3C; f[91*16+ 2] = 0x30; f[91*16+ 3] = 0x30;
    f[91*16+ 4] = 0x30; f[91*16+ 5] = 0x30; f[91*16+ 6] = 0x30;
    f[91*16+ 7] = 0x3C;

    // \ (92)
    f[92*16+ 2] = 0x60; f[92*16+ 3] = 0x30; f[92*16+ 4] = 0x18;
    f[92*16+ 5] = 0x0C; f[92*16+ 6] = 0x06;

    // ] (93)
    f[93*16+ 1] = 0x3C; f[93*16+ 2] = 0x0C; f[93*16+ 3] = 0x0C;
    f[93*16+ 4] = 0x0C; f[93*16+ 5] = 0x0C; f[93*16+ 6] = 0x0C;
    f[93*16+ 7] = 0x3C;

    // ^ (94)
    f[94*16+ 1] = 0x18; f[94*16+ 2] = 0x3C; f[94*16+ 3] = 0x66;

    // _ (95)
    f[95*16+12] = 0xFF;

    // ` (96)
    f[96*16+ 1] = 0x30; f[96*16+ 2] = 0x18;

    // a-z (97-122)
    f[97*16+ 4] = 0x3C; f[97*16+ 5] = 0x06; f[97*16+ 6] = 0x3E;
    f[97*16+ 7] = 0x66; f[97*16+ 8] = 0x3E;

    f[98*16+ 2] = 0x60; f[98*16+ 3] = 0x60; f[98*16+ 4] = 0x7C;
    f[98*16+ 5] = 0x66; f[98*16+ 6] = 0x66; f[98*16+ 7] = 0x66;
    f[98*16+ 8] = 0x7C;

    f[99*16+ 4] = 0x3C; f[99*16+ 5] = 0x66; f[99*16+ 6] = 0x60;
    f[99*16+ 7] = 0x66; f[99*16+ 8] = 0x3C;

    f[100*16+ 2] = 0x06; f[100*16+ 3] = 0x06; f[100*16+ 4] = 0x3E;
    f[100*16+ 5] = 0x66; f[100*16+ 6] = 0x66; f[100*16+ 7] = 0x66;
    f[100*16+ 8] = 0x3E;

    f[101*16+ 4] = 0x3C; f[101*16+ 5] = 0x66; f[101*16+ 6] = 0x7E;
    f[101*16+ 7] = 0x60; f[101*16+ 8] = 0x3C;

    f[102*16+ 2] = 0x1C; f[102*16+ 3] = 0x30; f[102*16+ 4] = 0x7C;
    f[102*16+ 5] = 0x30; f[102*16+ 6] = 0x30; f[102*16+ 7] = 0x30;
    f[102*16+ 8] = 0x30;

    f[103*16+ 4] = 0x3E; f[103*16+ 5] = 0x66; f[103*16+ 6] = 0x66;
    f[103*16+ 7] = 0x3E; f[103*16+ 8] = 0x06; f[103*16+ 9] = 0x3C;

    f[104*16+ 2] = 0x60; f[104*16+ 3] = 0x60; f[104*16+ 4] = 0x7C;
    f[104*16+ 5] = 0x66; f[104*16+ 6] = 0x66; f[104*16+ 7] = 0x66;
    f[104*16+ 8] = 0x66;

    f[105*16+ 2] = 0x18; f[105*16+ 3] = 0x00; f[105*16+ 4] = 0x38;
    f[105*16+ 5] = 0x18; f[105*16+ 6] = 0x18; f[105*16+ 7] = 0x18;
    f[105*16+ 8] = 0x3C;

    f[106*16+ 2] = 0x06; f[106*16+ 3] = 0x00; f[106*16+ 4] = 0x06;
    f[106*16+ 5] = 0x06; f[106*16+ 6] = 0x06; f[106*16+ 7] = 0x66;
    f[106*16+ 8] = 0x3C;

    f[107*16+ 2] = 0x60; f[107*16+ 3] = 0x60; f[107*16+ 4] = 0x66;
    f[107*16+ 5] = 0x6C; f[107*16+ 6] = 0x78; f[107*16+ 7] = 0x6C;
    f[107*16+ 8] = 0x66;

    f[108*16+ 2] = 0x38; f[108*16+ 3] = 0x18; f[108*16+ 4] = 0x18;
    f[108*16+ 5] = 0x18; f[108*16+ 6] = 0x18; f[108*16+ 7] = 0x18;
    f[108*16+ 8] = 0x3C;

    f[109*16+ 4] = 0xEC; f[109*16+ 5] = 0xFE; f[109*16+ 6] = 0xD6;
    f[109*16+ 7] = 0xC6; f[109*16+ 8] = 0xC6;

    f[110*16+ 4] = 0x7C; f[110*16+ 5] = 0x66; f[110*16+ 6] = 0x66;
    f[110*16+ 7] = 0x66; f[110*16+ 8] = 0x66;

    f[111*16+ 4] = 0x3C; f[111*16+ 5] = 0x66; f[111*16+ 6] = 0x66;
    f[111*16+ 7] = 0x66; f[111*16+ 8] = 0x3C;

    f[112*16+ 4] = 0x7C; f[112*16+ 5] = 0x66; f[112*16+ 6] = 0x66;
    f[112*16+ 7] = 0x7C; f[112*16+ 8] = 0x60; f[112*16+ 9] = 0x60;

    f[113*16+ 4] = 0x3E; f[113*16+ 5] = 0x66; f[113*16+ 6] = 0x66;
    f[113*16+ 7] = 0x3E; f[113*16+ 8] = 0x06; f[113*16+ 9] = 0x06;

    f[114*16+ 4] = 0x7C; f[114*16+ 5] = 0x66; f[114*16+ 6] = 0x60;
    f[114*16+ 7] = 0x60; f[114*16+ 8] = 0x60;

    f[115*16+ 4] = 0x3E; f[115*16+ 5] = 0x60; f[115*16+ 6] = 0x3C;
    f[115*16+ 7] = 0x06; f[115*16+ 8] = 0x7C;

    f[116*16+ 2] = 0x30; f[116*16+ 3] = 0x30; f[116*16+ 4] = 0x7C;
    f[116*16+ 5] = 0x30; f[116*16+ 6] = 0x30; f[116*16+ 7] = 0x30;
    f[116*16+ 8] = 0x1C;

    f[117*16+ 4] = 0x66; f[117*16+ 5] = 0x66; f[117*16+ 6] = 0x66;
    f[117*16+ 7] = 0x66; f[117*16+ 8] = 0x3E;

    f[118*16+ 4] = 0x66; f[118*16+ 5] = 0x66; f[118*16+ 6] = 0x66;
    f[118*16+ 7] = 0x3C; f[118*16+ 8] = 0x18;

    f[119*16+ 4] = 0xC6; f[119*16+ 5] = 0xC6; f[119*16+ 6] = 0xD6;
    f[119*16+ 7] = 0xFE; f[119*16+ 8] = 0x6C;

    f[120*16+ 4] = 0x66; f[120*16+ 5] = 0x3C; f[120*16+ 6] = 0x18;
    f[120*16+ 7] = 0x3C; f[120*16+ 8] = 0x66;

    f[121*16+ 4] = 0x66; f[121*16+ 5] = 0x66; f[121*16+ 6] = 0x3E;
    f[121*16+ 7] = 0x06; f[121*16+ 8] = 0x3C;

    f[122*16+ 4] = 0x7E; f[122*16+ 5] = 0x0C; f[122*16+ 6] = 0x18;
    f[122*16+ 7] = 0x30; f[122*16+ 8] = 0x7E;

    // { (123)
    f[123*16+ 1] = 0x0E; f[123*16+ 2] = 0x18; f[123*16+ 3] = 0x18;
    f[123*16+ 4] = 0x70; f[123*16+ 5] = 0x18; f[123*16+ 6] = 0x18;
    f[123*16+ 7] = 0x0E;

    // | (124)
    f[124*16+ 1] = 0x18; f[124*16+ 2] = 0x18; f[124*16+ 3] = 0x18;
    f[124*16+ 4] = 0x18; f[124*16+ 5] = 0x18; f[124*16+ 6] = 0x18;
    f[124*16+ 7] = 0x18; f[124*16+ 8] = 0x18;

    // } (125)
    f[125*16+ 1] = 0x70; f[125*16+ 2] = 0x18; f[125*16+ 3] = 0x18;
    f[125*16+ 4] = 0x0E; f[125*16+ 5] = 0x18; f[125*16+ 6] = 0x18;
    f[125*16+ 7] = 0x70;

    // ~ (126)
    f[126*16+ 5] = 0x76; f[126*16+ 6] = 0xDC;

    f
};
