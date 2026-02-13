// Framebuffer Driver — Double-Buffered
//
// Provides direct pixel-level access to the UEFI GOP framebuffer with
// double-buffering support for tear-free rendering.
//
// Architecture:
//   - Hardware framebuffer (write-combining memory, slow random access)
//   - Back buffer (regular RAM, fast random access)
//   - All drawing operations target the back buffer
//   - present() copies the back buffer to the hardware framebuffer
//
// The back buffer is allocated from the frame allocator on init.
// Until it's available, drawing goes directly to the hardware framebuffer.
//
// Performance considerations:
//   - Write-combining memory on the hardware FB makes sequential writes fast
//   - present() uses a simple memcpy — could be optimized with REP MOVSB or SSE
//   - Dirty-rectangle tracking could reduce the copy size (future optimization)

use cantaya_shared::boot_info::{FramebufferInfo, PixelFormat};
use crate::memory::frame_allocator;
use spin::Mutex;

/// A 32-bit RGBA color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    // Common colors (Windows-inspired palette)
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    pub const DARK_BLUE: Self = Self::rgb(0, 0, 128);       // Classic Windows desktop
    pub const LIGHT_GRAY: Self = Self::rgb(192, 192, 192);   // Windows 95 window chrome
    pub const DARK_GRAY: Self = Self::rgb(128, 128, 128);
    pub const TEAL: Self = Self::rgb(0, 128, 128);           // Windows 95/98 desktop
    pub const YELLOW: Self = Self::rgb(255, 255, 0);
    pub const BSOD_BLUE: Self = Self::rgb(0, 0, 170);       // Classic BSOD

    /// Convert to the framebuffer's pixel format (32-bit value)
    fn to_pixel(&self, format: PixelFormat) -> u32 {
        match format {
            PixelFormat::Bgr => {
                (self.b as u32) | ((self.g as u32) << 8) | ((self.r as u32) << 16)
            }
            PixelFormat::Rgb => {
                (self.r as u32) | ((self.g as u32) << 8) | ((self.b as u32) << 16)
            }
            PixelFormat::Unknown => {
                // Assume BGR as fallback
                (self.b as u32) | ((self.g as u32) << 8) | ((self.r as u32) << 16)
            }
        }
    }
}

/// Framebuffer state with double-buffering support
pub struct Framebuffer {
    /// Physical/virtual address of the hardware framebuffer
    pub fb_addr: u64,
    /// Address of the back buffer (in regular RAM)
    pub back_buffer: u64,
    /// Size of the framebuffer in bytes
    pub fb_size: usize,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Bytes per scanline (row) in pixels (stride)
    pub stride: u32,
    /// Pixel format
    format: PixelFormat,
    /// Whether the framebuffer is initialized
    initialized: bool,
    /// Whether we have a back buffer allocated
    double_buffered: bool,
    /// Dirty flag — set when drawing occurs, cleared by present()
    dirty: bool,
    /// Dirty rectangle (min corner, inclusive)
    dirty_x1: u32,
    dirty_y1: u32,
    /// Dirty rectangle (max corner, exclusive)
    dirty_x2: u32,
    dirty_y2: u32,
}

impl Framebuffer {
    const fn new() -> Self {
        Self {
            fb_addr: 0,
            back_buffer: 0,
            fb_size: 0,
            width: 0,
            height: 0,
            stride: 0,
            format: PixelFormat::Bgr,
            initialized: false,
            double_buffered: false,
            dirty: false,
            dirty_x1: u32::MAX,
            dirty_y1: u32::MAX,
            dirty_x2: 0,
            dirty_y2: 0,
        }
    }

    /// Get the draw target address (back buffer if available, otherwise hardware FB)
    #[inline(always)]
    pub fn draw_target(&self) -> u64 {
        if self.double_buffered {
            self.back_buffer
        } else {
            self.fb_addr
        }
    }

    /// Expand the dirty region to include a rectangle.
    #[inline(always)]
    fn mark_dirty_rect(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if w == 0 || h == 0 { return; }
        let x2 = (x + w).min(self.width);
        let y2 = (y + h).min(self.height);
        if x >= x2 || y >= y2 { return; }
        self.dirty_x1 = self.dirty_x1.min(x);
        self.dirty_y1 = self.dirty_y1.min(y);
        self.dirty_x2 = self.dirty_x2.max(x2);
        self.dirty_y2 = self.dirty_y2.max(y2);
        self.dirty = true;
    }

    /// Put a single pixel at (x, y) with the given color.
    ///
    /// Coordinates are bounds-checked. Out-of-bounds writes are silently ignored.
    pub fn put_pixel(&mut self, x: u32, y: u32, color: Color) {
        if !self.initialized || x >= self.width || y >= self.height {
            return;
        }

        let pixel = color.to_pixel(self.format);
        let offset = (y * self.stride + x) * 4;
        let target = self.draw_target();
        let ptr = (target + offset as u64) as *mut u32;

        unsafe {
            if self.double_buffered {
                core::ptr::write(ptr, pixel);
            } else {
                core::ptr::write_volatile(ptr, pixel);
            }
        }
        self.mark_dirty_rect(x, y, 1, 1);
    }

    /// Fill a rectangle with a solid color.
    ///
    /// Uses bulk memory fill (slice::fill) for maximum throughput on the back
    /// buffer. Falls back to volatile writes for direct hardware FB access.
    pub fn fill_rect(&mut self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        if !self.initialized {
            return;
        }

        let pixel = color.to_pixel(self.format);
        let x_end = (x + width).min(self.width);
        let y_end = (y + height).min(self.height);
        let cols = x_end.saturating_sub(x) as usize;
        if cols == 0 || y >= y_end { return; }
        let target = self.draw_target();

        if self.double_buffered {
            for row in y..y_end {
                let row_start = (target + (row * self.stride + x) as u64 * 4) as *mut u32;
                unsafe {
                    let slice = core::slice::from_raw_parts_mut(row_start, cols);
                    slice.fill(pixel);
                }
            }
        } else {
            for row in y..y_end {
                let row_start = (target + (row * self.stride + x) as u64 * 4) as *mut u32;
                for col in 0..cols {
                    unsafe {
                        core::ptr::write_volatile(row_start.add(col), pixel);
                    }
                }
            }
        }
        self.mark_dirty_rect(x, y, width, height);
    }

    /// Clear the entire screen to a solid color.
    pub fn clear(&mut self, color: Color) {
        if !self.initialized {
            return;
        }

        let pixel = color.to_pixel(self.format);
        let target = self.draw_target();
        let total_pixels = self.stride as usize * self.height as usize;

        if self.double_buffered {
            unsafe {
                let slice = core::slice::from_raw_parts_mut(target as *mut u32, total_pixels);
                slice.fill(pixel);
            }
        } else {
            let ptr = target as *mut u32;
            for i in 0..total_pixels {
                unsafe {
                    core::ptr::write_volatile(ptr.add(i), pixel);
                }
            }
        }
        self.mark_dirty_rect(0, 0, self.width, self.height);
    }

    /// Copy only the dirty region of the back buffer to the hardware framebuffer.
    ///
    /// Uses dirty-rectangle tracking so that cursor-only movement copies just
    /// the small cursor-sized region (~1 KB) instead of the entire ~8 MB buffer.
    pub fn present(&mut self) {
        if !self.initialized || !self.double_buffered || !self.dirty {
            return;
        }

        let x1 = self.dirty_x1.min(self.width);
        let y1 = self.dirty_y1.min(self.height);
        let x2 = self.dirty_x2.min(self.width);
        let y2 = self.dirty_y2.min(self.height);

        if x1 >= x2 || y1 >= y2 {
            self.dirty = false;
            return;
        }

        // If dirty region spans the full width, copy rows contiguously
        if x1 == 0 && x2 >= self.width {
            let offset = (y1 * self.stride) as usize * 4;
            let size = ((y2 - y1) * self.stride) as usize * 4;
            unsafe {
                let src = (self.back_buffer as usize + offset) as *const u8;
                let dst = (self.fb_addr as usize + offset) as *mut u8;
                core::ptr::copy_nonoverlapping(src, dst, size);
            }
        } else {
            // Copy only the dirty columns in each row
            let row_bytes = (x2 - x1) as usize * 4;
            for row in y1..y2 {
                let offset = (row * self.stride + x1) as usize * 4;
                unsafe {
                    let src = (self.back_buffer as usize + offset) as *const u8;
                    let dst = (self.fb_addr as usize + offset) as *mut u8;
                    core::ptr::copy_nonoverlapping(src, dst, row_bytes);
                }
            }
        }

        self.dirty = false;
        self.dirty_x1 = self.width;
        self.dirty_y1 = self.height;
        self.dirty_x2 = 0;
        self.dirty_y2 = 0;
    }

    /// Direct write to the hardware framebuffer (bypasses double buffer).
    /// Used for panic/BSOD screens where we need guaranteed display.
    pub fn put_pixel_direct(&self, x: u32, y: u32, color: Color) {
        if !self.initialized || x >= self.width || y >= self.height {
            return;
        }

        let pixel = color.to_pixel(self.format);
        let offset = (y * self.stride + x) * 4;
        let ptr = (self.fb_addr + offset as u64) as *mut u32;

        unsafe {
            core::ptr::write_volatile(ptr, pixel);
        }
    }

    /// Direct fill rectangle to hardware framebuffer (bypasses double buffer).
    /// Used for panic/BSOD screens.
    pub fn fill_rect_direct(&self, x: u32, y: u32, width: u32, height: u32, color: Color) {
        if !self.initialized {
            return;
        }

        let pixel = color.to_pixel(self.format);
        let x_end = (x + width).min(self.width);
        let y_end = (y + height).min(self.height);

        for row in y..y_end {
            let row_start = (self.fb_addr + (row * self.stride + x) as u64 * 4) as *mut u32;
            for col in 0..(x_end - x) {
                unsafe {
                    core::ptr::write_volatile(row_start.add(col as usize), pixel);
                }
            }
        }
    }

    /// Clear directly to hardware framebuffer (bypasses double buffer).
    pub fn clear_direct(&self, color: Color) {
        self.fill_rect_direct(0, 0, self.width, self.height, color);
    }

    /// Save a rectangular region from the draw target into a buffer.
    /// Used for cursor save/restore to avoid full redraws on mouse movement.
    pub fn save_region(&self, x: u32, y: u32, w: u32, h: u32, buf: &mut [u32]) {
        if !self.initialized { return; }
        let target = self.draw_target();
        for row in 0..h {
            let py = y + row;
            if py >= self.height { break; }
            for col in 0..w {
                let px = x + col;
                if px >= self.width { break; }
                let offset = (py * self.stride + px) as u64 * 4;
                let ptr = (target + offset) as *const u32;
                buf[(row * w + col) as usize] = unsafe { core::ptr::read(ptr) };
            }
        }
    }

    /// Restore a rectangular region to the draw target from a buffer.
    /// Marks the restored area as dirty so it will be presented.
    pub fn restore_region(&mut self, x: u32, y: u32, w: u32, h: u32, buf: &[u32]) {
        if !self.initialized { return; }
        let target = self.draw_target();
        for row in 0..h {
            let py = y + row;
            if py >= self.height { break; }
            for col in 0..w {
                let px = x + col;
                if px >= self.width { break; }
                let offset = (py * self.stride + px) as u64 * 4;
                let ptr = (target + offset) as *mut u32;
                unsafe { core::ptr::write(ptr, buf[(row * w + col) as usize]); }
            }
        }
        self.mark_dirty_rect(x, y, w, h);
    }
}

pub static FRAMEBUFFER: Mutex<Framebuffer> = Mutex::new(Framebuffer::new());

/// Initialize the framebuffer from boot info.
///
/// Allocates a back buffer from the frame allocator for double-buffering.
/// Falls back to direct rendering if allocation fails.
pub fn init(info: &FramebufferInfo) {
    let mut fb = FRAMEBUFFER.lock();
    fb.fb_addr = info.address;
    fb.width = info.width;
    fb.height = info.height;
    fb.stride = info.stride;
    fb.format = info.pixel_format;
    fb.initialized = true;

    // Calculate framebuffer size
    let fb_size = (info.stride as usize) * (info.height as usize) * 4;
    fb.fb_size = fb_size;

    // Try to allocate a back buffer
    let pages_needed = (fb_size + 4095) / 4096;
    match frame_allocator::allocate_contiguous_frames(pages_needed) {
        Some(back_buffer_phys) => {
            fb.back_buffer = back_buffer_phys;
            fb.double_buffered = true;

            // Clear the back buffer
            unsafe {
                core::ptr::write_bytes(back_buffer_phys as *mut u8, 0, fb_size);
            }

            log::info!(
                "Framebuffer: {}x{}, stride={}, format={:?}, double-buffered ({} KiB back buffer)",
                fb.width, fb.height, fb.stride, fb.format, fb_size / 1024
            );
        }
        None => {
            fb.double_buffered = false;
            log::warn!(
                "Framebuffer: {}x{}, stride={}, format={:?}, NO double buffer (alloc failed)",
                fb.width, fb.height, fb.stride, fb.format
            );
        }
    }

    // Clear screen to the classic Windows teal/dark blue
    fb.clear(Color::TEAL);
    if fb.double_buffered {
        fb.present();
    }
}
