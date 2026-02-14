// Mouse cursor rendering for CantayaOS desktop.

use crate::graphics::framebuffer::{Color, FRAMEBUFFER};

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
pub(super) fn draw_cursor(mx: u32, my: u32) {
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
// Cursor Save Buffer (fast cursor movement without full redraw)
// ============================================================================

/// Saved pixels under the mouse cursor for fast restore.
pub(super) struct CursorSave {
    data: [u32; (CURSOR_WIDTH * CURSOR_HEIGHT) as usize],
    x: u32,
    y: u32,
    valid: bool,
}

impl CursorSave {
    pub(super) fn new() -> Self {
        Self {
            data: [0u32; (CURSOR_WIDTH * CURSOR_HEIGHT) as usize],
            x: 0,
            y: 0,
            valid: false,
        }
    }
}

/// Save the pixels under the cursor position from the back buffer.
pub(super) fn save_under_cursor(save: &mut CursorSave, mx: u32, my: u32) {
    let fb = FRAMEBUFFER.lock();
    fb.save_region(mx, my, CURSOR_WIDTH, CURSOR_HEIGHT, &mut save.data);
    save.x = mx;
    save.y = my;
    save.valid = true;
}

/// Restore the saved pixels (erase the cursor from the back buffer).
pub(super) fn restore_cursor_save(save: &mut CursorSave) {
    if !save.valid { return; }
    let mut fb = FRAMEBUFFER.lock();
    fb.restore_region(save.x, save.y, CURSOR_WIDTH, CURSOR_HEIGHT, &save.data);
    save.valid = false;
}
