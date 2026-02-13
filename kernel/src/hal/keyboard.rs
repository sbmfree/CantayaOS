// PS/2 Keyboard Driver — Scancode Set 1 Translation
//
// Translates raw PS/2 scancodes (Set 1) into ASCII characters and key events.
// The PS/2 controller delivers scancodes via IRQ1 (vector 33 after PIC remap).
//
// Scancode Set 1:
//   - Key press: sends a make code (0x01–0x58)
//   - Key release: sends make code | 0x80 (break code)
//   - Extended keys (arrows, etc.): prefixed with 0xE0
//
// In Windows, this is handled by i8042prt.sys (PS/2 port driver) and
// kbdclass.sys (keyboard class driver). We combine both roles here.

use spin::Mutex;

/// Key event passed to the input subsystem
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    /// The ASCII character (0 for non-printable keys)
    pub ascii: u8,
    /// The key code (scancode-derived identifier)
    pub key: KeyCode,
    /// Whether this is a press (true) or release (false)
    pub pressed: bool,
}

/// Logical key codes (scancode-independent)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KeyCode {
    Unknown = 0,
    Escape, F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Backquote, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9, Key0,
    Minus, Equals, Backspace,
    Tab, Q, W, E, R, T, Y, U, I, O, P, LeftBracket, RightBracket, Backslash,
    CapsLock, A, S, D, F, G, H, J, K, L, Semicolon, Quote, Enter,
    LeftShift, Z, X, C, V, B, N, M, Comma, Period, Slash, RightShift,
    LeftCtrl, LeftAlt, Space, RightAlt, RightCtrl,
    Up, Down, Left, Right,
    Home, End, PageUp, PageDown, Insert, Delete,
}

/// Modifier key state
#[derive(Debug, Clone, Copy)]
struct ModifierState {
    left_shift: bool,
    right_shift: bool,
    caps_lock: bool,
    left_ctrl: bool,
    right_ctrl: bool,
    left_alt: bool,
    right_alt: bool,
}

impl ModifierState {
    const fn new() -> Self {
        Self {
            left_shift: false,
            right_shift: false,
            caps_lock: false,
            left_ctrl: false,
            right_ctrl: false,
            left_alt: false,
            right_alt: false,
        }
    }

    fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    #[allow(dead_code)]
    fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }
}

/// Ring buffer for key events (lock-free single-producer single-consumer)
const KEY_BUFFER_SIZE: usize = 64;

struct KeyBuffer {
    buffer: [KeyEvent; KEY_BUFFER_SIZE],
    read_pos: usize,
    write_pos: usize,
}

impl KeyBuffer {
    const fn new() -> Self {
        Self {
            buffer: [KeyEvent { ascii: 0, key: KeyCode::Unknown, pressed: false }; KEY_BUFFER_SIZE],
            read_pos: 0,
            write_pos: 0,
        }
    }

    fn push(&mut self, event: KeyEvent) {
        let next = (self.write_pos + 1) % KEY_BUFFER_SIZE;
        if next != self.read_pos {
            self.buffer[self.write_pos] = event;
            self.write_pos = next;
        }
        // Drop event if buffer is full
    }

    fn pop(&mut self) -> Option<KeyEvent> {
        if self.read_pos == self.write_pos {
            None
        } else {
            let event = self.buffer[self.read_pos];
            self.read_pos = (self.read_pos + 1) % KEY_BUFFER_SIZE;
            Some(event)
        }
    }
}

static MODIFIERS: Mutex<ModifierState> = Mutex::new(ModifierState::new());
static KEY_BUFFER: Mutex<KeyBuffer> = Mutex::new(KeyBuffer::new());
static EXTENDED_KEY: Mutex<bool> = Mutex::new(false);

/// Called from the keyboard IRQ handler with the raw scancode.
pub fn handle_scancode(scancode: u8) {
    // Check for extended key prefix
    if scancode == 0xE0 {
        *EXTENDED_KEY.lock() = true;
        return;
    }

    let is_extended = {
        let mut ext = EXTENDED_KEY.lock();
        let was = *ext;
        *ext = false;
        was
    };

    let pressed = scancode & 0x80 == 0;
    let code = scancode & 0x7F;

    let key = if is_extended {
        match code {
            0x48 => KeyCode::Up,
            0x50 => KeyCode::Down,
            0x4B => KeyCode::Left,
            0x4D => KeyCode::Right,
            0x47 => KeyCode::Home,
            0x4F => KeyCode::End,
            0x49 => KeyCode::PageUp,
            0x51 => KeyCode::PageDown,
            0x52 => KeyCode::Insert,
            0x53 => KeyCode::Delete,
            0x1D => KeyCode::RightCtrl,
            0x38 => KeyCode::RightAlt,
            _ => KeyCode::Unknown,
        }
    } else {
        scancode_to_keycode(code)
    };

    // Update modifier state
    {
        let mut mods = MODIFIERS.lock();
        match key {
            KeyCode::LeftShift => mods.left_shift = pressed,
            KeyCode::RightShift => mods.right_shift = pressed,
            KeyCode::LeftCtrl => mods.left_ctrl = pressed,
            KeyCode::RightCtrl => mods.right_ctrl = pressed,
            KeyCode::LeftAlt => mods.left_alt = pressed,
            KeyCode::RightAlt => mods.right_alt = pressed,
            KeyCode::CapsLock => {
                if pressed {
                    mods.caps_lock = !mods.caps_lock;
                }
            }
            _ => {}
        }
    }

    // Translate to ASCII for key press events
    let ascii = if pressed {
        let mods = MODIFIERS.lock();
        keycode_to_ascii(key, mods.shift(), mods.caps_lock, mods.ctrl())
    } else {
        0
    };

    let event = KeyEvent { ascii, key, pressed };
    KEY_BUFFER.lock().push(event);
}

/// Read the next key event from the buffer (non-blocking).
pub fn read_key() -> Option<KeyEvent> {
    KEY_BUFFER.lock().pop()
}

/// Wait for the next key press and return its ASCII character.
/// Returns 0 for non-printable keys. Blocks until a key is pressed.
pub fn read_char() -> KeyEvent {
    loop {
        if let Some(event) = read_key() {
            if event.pressed {
                return event;
            }
        }
        // Yield to avoid busy-spinning — HLT until next interrupt
        unsafe { core::arch::asm!("hlt"); }
    }
}

/// Try to read a key press without blocking.
/// Returns Some(event) if a key was pressed, None otherwise.
pub fn try_read_char() -> Option<KeyEvent> {
    loop {
        match read_key() {
            Some(event) if event.pressed => return Some(event),
            Some(_) => continue, // skip key-release events already in buffer
            None => return None,
        }
    }
}

/// Translate scancode (Set 1) to KeyCode
fn scancode_to_keycode(code: u8) -> KeyCode {
    match code {
        0x01 => KeyCode::Escape,
        0x02 => KeyCode::Key1,
        0x03 => KeyCode::Key2,
        0x04 => KeyCode::Key3,
        0x05 => KeyCode::Key4,
        0x06 => KeyCode::Key5,
        0x07 => KeyCode::Key6,
        0x08 => KeyCode::Key7,
        0x09 => KeyCode::Key8,
        0x0A => KeyCode::Key9,
        0x0B => KeyCode::Key0,
        0x0C => KeyCode::Minus,
        0x0D => KeyCode::Equals,
        0x0E => KeyCode::Backspace,
        0x0F => KeyCode::Tab,
        0x10 => KeyCode::Q,
        0x11 => KeyCode::W,
        0x12 => KeyCode::E,
        0x13 => KeyCode::R,
        0x14 => KeyCode::T,
        0x15 => KeyCode::Y,
        0x16 => KeyCode::U,
        0x17 => KeyCode::I,
        0x18 => KeyCode::O,
        0x19 => KeyCode::P,
        0x1A => KeyCode::LeftBracket,
        0x1B => KeyCode::RightBracket,
        0x1C => KeyCode::Enter,
        0x1D => KeyCode::LeftCtrl,
        0x1E => KeyCode::A,
        0x1F => KeyCode::S,
        0x20 => KeyCode::D,
        0x21 => KeyCode::F,
        0x22 => KeyCode::G,
        0x23 => KeyCode::H,
        0x24 => KeyCode::J,
        0x25 => KeyCode::K,
        0x26 => KeyCode::L,
        0x27 => KeyCode::Semicolon,
        0x28 => KeyCode::Quote,
        0x29 => KeyCode::Backquote,
        0x2A => KeyCode::LeftShift,
        0x2B => KeyCode::Backslash,
        0x2C => KeyCode::Z,
        0x2D => KeyCode::X,
        0x2E => KeyCode::C,
        0x2F => KeyCode::V,
        0x30 => KeyCode::B,
        0x31 => KeyCode::N,
        0x32 => KeyCode::M,
        0x33 => KeyCode::Comma,
        0x34 => KeyCode::Period,
        0x35 => KeyCode::Slash,
        0x36 => KeyCode::RightShift,
        0x38 => KeyCode::LeftAlt,
        0x39 => KeyCode::Space,
        0x3A => KeyCode::CapsLock,
        0x3B => KeyCode::F1,
        0x3C => KeyCode::F2,
        0x3D => KeyCode::F3,
        0x3E => KeyCode::F4,
        0x3F => KeyCode::F5,
        0x40 => KeyCode::F6,
        0x41 => KeyCode::F7,
        0x42 => KeyCode::F8,
        0x43 => KeyCode::F9,
        0x44 => KeyCode::F10,
        0x57 => KeyCode::F11,
        0x58 => KeyCode::F12,
        _ => KeyCode::Unknown,
    }
}

/// Translate a KeyCode to ASCII, applying shift/caps/ctrl modifiers.
fn keycode_to_ascii(key: KeyCode, shift: bool, caps: bool, ctrl: bool) -> u8 {
    // Ctrl+letter produces control codes (Ctrl+C = 0x03, etc.)
    if ctrl {
        return match key {
            KeyCode::A => 0x01, KeyCode::B => 0x02, KeyCode::C => 0x03,
            KeyCode::D => 0x04, KeyCode::E => 0x05, KeyCode::F => 0x06,
            KeyCode::L => 0x0C, // Form feed (clear screen)
            _ => 0,
        };
    }

    // Determine if we should use uppercase
    let upper = shift ^ caps; // XOR: shift inverts caps lock behavior

    match key {
        // Letters
        KeyCode::A => if upper { b'A' } else { b'a' },
        KeyCode::B => if upper { b'B' } else { b'b' },
        KeyCode::C => if upper { b'C' } else { b'c' },
        KeyCode::D => if upper { b'D' } else { b'd' },
        KeyCode::E => if upper { b'E' } else { b'e' },
        KeyCode::F => if upper { b'F' } else { b'f' },
        KeyCode::G => if upper { b'G' } else { b'g' },
        KeyCode::H => if upper { b'H' } else { b'h' },
        KeyCode::I => if upper { b'I' } else { b'i' },
        KeyCode::J => if upper { b'J' } else { b'j' },
        KeyCode::K => if upper { b'K' } else { b'k' },
        KeyCode::L => if upper { b'L' } else { b'l' },
        KeyCode::M => if upper { b'M' } else { b'm' },
        KeyCode::N => if upper { b'N' } else { b'n' },
        KeyCode::O => if upper { b'O' } else { b'o' },
        KeyCode::P => if upper { b'P' } else { b'p' },
        KeyCode::Q => if upper { b'Q' } else { b'q' },
        KeyCode::R => if upper { b'R' } else { b'r' },
        KeyCode::S => if upper { b'S' } else { b's' },
        KeyCode::T => if upper { b'T' } else { b't' },
        KeyCode::U => if upper { b'U' } else { b'u' },
        KeyCode::V => if upper { b'V' } else { b'v' },
        KeyCode::W => if upper { b'W' } else { b'w' },
        KeyCode::X => if upper { b'X' } else { b'x' },
        KeyCode::Y => if upper { b'Y' } else { b'y' },
        KeyCode::Z => if upper { b'Z' } else { b'z' },

        // Numbers / symbols row
        KeyCode::Key1 => if shift { b'!' } else { b'1' },
        KeyCode::Key2 => if shift { b'@' } else { b'2' },
        KeyCode::Key3 => if shift { b'#' } else { b'3' },
        KeyCode::Key4 => if shift { b'$' } else { b'4' },
        KeyCode::Key5 => if shift { b'%' } else { b'5' },
        KeyCode::Key6 => if shift { b'^' } else { b'6' },
        KeyCode::Key7 => if shift { b'&' } else { b'7' },
        KeyCode::Key8 => if shift { b'*' } else { b'8' },
        KeyCode::Key9 => if shift { b'(' } else { b'9' },
        KeyCode::Key0 => if shift { b')' } else { b'0' },
        KeyCode::Minus => if shift { b'_' } else { b'-' },
        KeyCode::Equals => if shift { b'+' } else { b'=' },

        // Punctuation
        KeyCode::LeftBracket => if shift { b'{' } else { b'[' },
        KeyCode::RightBracket => if shift { b'}' } else { b']' },
        KeyCode::Backslash => if shift { b'|' } else { b'\\' },
        KeyCode::Semicolon => if shift { b':' } else { b';' },
        KeyCode::Quote => if shift { b'"' } else { b'\'' },
        KeyCode::Backquote => if shift { b'~' } else { b'`' },
        KeyCode::Comma => if shift { b'<' } else { b',' },
        KeyCode::Period => if shift { b'>' } else { b'.' },
        KeyCode::Slash => if shift { b'?' } else { b'/' },

        // Whitespace / control
        KeyCode::Space => b' ',
        KeyCode::Enter => b'\n',
        KeyCode::Tab => b'\t',
        KeyCode::Backspace => 0x08,

        _ => 0,
    }
}
