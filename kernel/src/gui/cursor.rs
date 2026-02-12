//! Mouse cursor rendering — a 12×19 arrow sprite.

use crate::drivers::framebuffer;

pub const CURSOR_W: u32 = 12;
pub const CURSOR_H: u32 = 19;

/// Draw the mouse cursor at (x, y). Each pixel is either:
/// 'B' = black border (0xFF000000)
/// 'W' = white fill  (0xFFFFFFFF)
/// ' ' = transparent  (skip)
const CURSOR_BITMAP: [&[u8; 12]; 19] = [
    b"B           ",
    b"BB          ",
    b"BWB         ",
    b"BWWB        ",
    b"BWWWB       ",
    b"BWWWWB      ",
    b"BWWWWWB     ",
    b"BWWWWWWB    ",
    b"BWWWWWWWB   ",
    b"BWWWWWWWWB  ",
    b"BWWWWWWWWWB ",
    b"BWWWWWWWWWWB",
    b"BWWWWWBBBBB ",
    b"BWWBWWB     ",
    b"BWBB BWB    ",
    b"BB    BWB   ",
    b"B      BWB  ",
    b"        BWB ",
    b"         BB ",
];

/// Draw the cursor sprite at screen position (cx, cy).
pub fn draw_cursor(fb: &mut framebuffer::Framebuffer, cx: i32, cy: i32) {
    for (row, line) in CURSOR_BITMAP.iter().enumerate() {
        let py = cy + row as i32;
        if py < 0 || py >= framebuffer::SCREEN_HEIGHT as i32 { continue; }
        for (col, &ch) in line.iter().enumerate() {
            let px = cx + col as i32;
            if px < 0 || px >= framebuffer::SCREEN_WIDTH as i32 { continue; }
            match ch {
                b'B' => fb.put_pixel(px as u32, py as u32, 0xFF000000),
                b'W' => fb.put_pixel(px as u32, py as u32, 0xFFFFFFFF),
                _ => {} // transparent
            }
        }
    }
}
