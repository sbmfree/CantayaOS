//! GUI event system — translates raw InputEvents into GUI-level events
//! (mouse move, click, key) and maintains mouse/pointer state.

use crate::drivers::framebuffer::{SCREEN_WIDTH, SCREEN_HEIGHT};

extern crate alloc;

/// GUI-level event.
#[derive(Clone, Copy, Debug)]
pub enum GuiEvent {
    MouseMove { x: i32, y: i32 },
    MouseDown { x: i32, y: i32, button: u8 },
    MouseUp   { x: i32, y: i32, button: u8 },
    KeyDown   { code: u16 },
    KeyUp     { code: u16 },
}

/// Global mouse state, updated by the compositor loop.
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub buttons: [bool; 3], // left, right, middle
}

impl MouseState {
    pub const fn new() -> Self {
        MouseState { x: 400, y: 300, buttons: [false; 3] }
    }
}

/// Drain raw input events and convert them to GuiEvents.
pub fn drain_input(mouse: &mut MouseState) -> alloc::vec::Vec<GuiEvent> {
    let mut events = alloc::vec::Vec::new();

    while let Some(raw) = crate::drivers::virtio_input::poll_event() {
        match raw {
            crate::drivers::virtio_input::InputEvent::MouseMove { x, y } => {
                mouse.x = (x as i32).clamp(0, SCREEN_WIDTH as i32 - 1);
                mouse.y = (y as i32).clamp(0, SCREEN_HEIGHT as i32 - 1);
                events.push(GuiEvent::MouseMove { x: mouse.x, y: mouse.y });
            }
            crate::drivers::virtio_input::InputEvent::MouseButton { button, pressed } => {
                let idx = (button as usize).min(2);
                mouse.buttons[idx] = pressed;
                if pressed {
                    events.push(GuiEvent::MouseDown { x: mouse.x, y: mouse.y, button });
                } else {
                    events.push(GuiEvent::MouseUp { x: mouse.x, y: mouse.y, button });
                }
            }
            crate::drivers::virtio_input::InputEvent::Key { code, pressed } => {
                if pressed {
                    events.push(GuiEvent::KeyDown { code });
                } else {
                    events.push(GuiEvent::KeyUp { code });
                }
            }
        }
    }
    events
}

// ---------------------------------------------------------------------------
// Linux key-code to ASCII helper (US QWERTY, minimal)
// ---------------------------------------------------------------------------
pub fn keycode_to_char(code: u16, _shift: bool) -> Option<char> {
    // Linux scancodes for common keys
    let c = match code {
        2  => '1', 3  => '2', 4  => '3', 5  => '4', 6  => '5',
        7  => '6', 8  => '7', 9  => '8', 10 => '9', 11 => '0',
        16 => 'q', 17 => 'w', 18 => 'e', 19 => 'r', 20 => 't',
        21 => 'y', 22 => 'u', 23 => 'i', 24 => 'o', 25 => 'p',
        30 => 'a', 31 => 's', 32 => 'd', 33 => 'f', 34 => 'g',
        35 => 'h', 36 => 'j', 37 => 'k', 38 => 'l',
        44 => 'z', 45 => 'x', 46 => 'c', 47 => 'v', 48 => 'b',
        49 => 'n', 50 => 'm',
        57 => ' ',
        _ => return None,
    };
    Some(c)
}
