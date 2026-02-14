// Wallpaper rendering for CantayaOS desktop.

extern crate alloc;

use crate::graphics::framebuffer::{FRAMEBUFFER, Color};
use crate::graphics::font::{self, CHAR_WIDTH, CHAR_HEIGHT};

/// Cached wallpaper pixel data (native framebuffer format).
/// Computed once on first draw, then blitted via fast memcpy on subsequent redraws.
static mut WALLPAPER_CACHE: Option<alloc::vec::Vec<u32>> = None;
static mut WALLPAPER_CACHE_W: u32 = 0;
static mut WALLPAPER_CACHE_H: u32 = 0;

/// Draw a tiled wallpaper pattern on the desktop background.
/// First call computes the gradient + watermark pixel-by-pixel and caches the
/// result.  Every subsequent call blits the cached buffer in one fast memcpy
/// per scanline, eliminating the ~2 M put_pixel calls that caused the lag.
pub(super) fn draw_wallpaper(w: u32, h: u32) {
    // ── Fast path: blit from cache ──────────────────────────────────────
    unsafe {
        if let Some(ref cache) = WALLPAPER_CACHE {
            if WALLPAPER_CACHE_W == w && WALLPAPER_CACHE_H == h {
                let mut fb = FRAMEBUFFER.lock();
                fb.blit_buffer(0, 0, w, h, cache);
                return;
            }
        }
    }

    // ── Slow path (first draw only): compute and cache ──────────────────
    {
        let mut fb = FRAMEBUFFER.lock();

        // Gradient background: dark teal → lighter teal
        for y in 0..h {
            let t = y as f32 / h as f32;
            let r = 0u8;
            let g = (64.0 + t * 76.0) as u8;
            let b_val = (80.0 + t * 60.0) as u8;

            for x in 0..w {
                let pattern = ((x / 16 + y / 16) % 2 == 0) as u8;
                let pg = g.saturating_add(pattern * 6);
                let pb = b_val.saturating_add(pattern * 4);
                fb.put_pixel(x, y, Color::rgb(r, pg, pb));
            }
        }

        // Subtle CantayaOS watermark in centre (3× scale)
        let text = "CantayaOS";
        let text_w = text.len() as u32 * CHAR_WIDTH * 3;
        let text_x = (w.saturating_sub(text_w)) / 2;
        let text_y = (h.saturating_sub(CHAR_HEIGHT * 3)) / 2;

        for (ci, c) in text.chars().enumerate() {
            let bitmap = font::get_char_bitmap(c);
            for (dy, &row_bits) in bitmap.iter().enumerate() {
                for dx in 0..8u32 {
                    if (row_bits >> (7 - dx)) & 1 != 0 {
                        for sy in 0..3u32 {
                            for sx in 0..3u32 {
                                let px = text_x + ci as u32 * CHAR_WIDTH * 3 + dx * 3 + sx;
                                let py = text_y + dy as u32 * 3 + sy;
                                if px < w && py < h {
                                    let base_t = py as f32 / h as f32;
                                    let bg = (64.0 + base_t * 76.0) as u8;
                                    let bb = (80.0 + base_t * 60.0) as u8;
                                    fb.put_pixel(px, py, Color::rgb(0, bg.saturating_add(15), bb.saturating_add(10)));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Snapshot the finished wallpaper from the back buffer into the cache
        let total = (w * h) as usize;
        let mut cache = alloc::vec![0u32; total];
        fb.snapshot_region(0, 0, w, h, &mut cache);

        unsafe {
            WALLPAPER_CACHE = Some(cache);
            WALLPAPER_CACHE_W = w;
            WALLPAPER_CACHE_H = h;
        }
    }
}
