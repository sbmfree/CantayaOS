// PS/2 Mouse Driver
//
// Implements a standard PS/2 mouse driver that communicates via the 8042
// PS/2 controller. The mouse sends 3-byte packets (or 4-byte with scroll wheel)
// containing button state and X/Y delta movement.
//
// PS/2 Mouse Protocol:
//   Byte 1: [Y overflow | X overflow | Y sign | X sign | 1 | Middle | Right | Left]
//   Byte 2: X movement (signed, relative)
//   Byte 3: Y movement (signed, relative)
//
// The mouse is on IRQ12 (vector 44 after PIC remapping).
// It shares the 8042 PS/2 controller with the keyboard (port 0x60/0x64).
//
// In Windows, the PS/2 mouse is handled by i8042prt.sys and mouclass.sys.
// We combine both roles here.

use spin::Mutex;
use super::port::{inb, outb};

// ============================================================================
// Constants
// ============================================================================

/// 8042 PS/2 controller ports
const PS2_DATA: u16 = 0x60;
const PS2_STATUS: u16 = 0x64;
const PS2_COMMAND: u16 = 0x64;

/// Status register bits
const STATUS_OUTPUT_FULL: u8 = 0x01;
const STATUS_INPUT_FULL: u8 = 0x02;

/// 8042 controller commands
const CMD_READ_CONFIG: u8 = 0x20;
const CMD_WRITE_CONFIG: u8 = 0x60;
const CMD_ENABLE_AUX: u8 = 0xA8;   // Enable auxiliary (mouse) port
const CMD_DISABLE_AUX: u8 = 0xA7;  // Disable auxiliary port
const CMD_WRITE_AUX: u8 = 0xD4;    // Write next byte to auxiliary device

/// Mouse commands (sent via CMD_WRITE_AUX)
const MOUSE_SET_DEFAULTS: u8 = 0xF6;
const MOUSE_ENABLE_DATA: u8 = 0xF4;
const MOUSE_DISABLE_DATA: u8 = 0xF5;
const MOUSE_RESET: u8 = 0xFF;
const MOUSE_SET_SAMPLE_RATE: u8 = 0xF3;
const MOUSE_GET_DEVICE_ID: u8 = 0xF2;

/// Mouse response bytes
const MOUSE_ACK: u8 = 0xFA;
const MOUSE_SELF_TEST_OK: u8 = 0xAA;

// ============================================================================
// Mouse State
// ============================================================================

/// Mouse button state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
}

/// A mouse event with position and button state
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent {
    pub dx: i16,        // X movement delta
    pub dy: i16,        // Y movement delta
    pub buttons: MouseButtons,
}

/// Mouse packet accumulator
struct MousePacketState {
    /// Bytes received so far for the current packet
    bytes: [u8; 4],
    /// Number of bytes received (0-2 for 3-byte packets)
    byte_index: u8,
    /// Whether scroll wheel is supported (4-byte packets)
    has_scroll: bool,
}

impl MousePacketState {
    const fn new() -> Self {
        Self {
            bytes: [0; 4],
            byte_index: 0,
            has_scroll: false,
        }
    }

    /// Expected packet size
    fn packet_size(&self) -> u8 {
        if self.has_scroll { 4 } else { 3 }
    }
}

/// Ring buffer for mouse events
const MOUSE_BUFFER_SIZE: usize = 32;

struct MouseBuffer {
    events: [MouseEvent; MOUSE_BUFFER_SIZE],
    read_idx: usize,
    write_idx: usize,
    count: usize,
}

impl MouseBuffer {
    const fn new() -> Self {
        Self {
            events: [MouseEvent {
                dx: 0,
                dy: 0,
                buttons: MouseButtons { left: false, right: false, middle: false },
            }; MOUSE_BUFFER_SIZE],
            read_idx: 0,
            write_idx: 0,
            count: 0,
        }
    }

    fn push(&mut self, event: MouseEvent) {
        self.events[self.write_idx] = event;
        self.write_idx = (self.write_idx + 1) % MOUSE_BUFFER_SIZE;
        if self.count < MOUSE_BUFFER_SIZE {
            self.count += 1;
        } else {
            // Overwrite oldest
            self.read_idx = (self.read_idx + 1) % MOUSE_BUFFER_SIZE;
        }
    }

    fn pop(&mut self) -> Option<MouseEvent> {
        if self.count == 0 {
            return None;
        }
        let event = self.events[self.read_idx];
        self.read_idx = (self.read_idx + 1) % MOUSE_BUFFER_SIZE;
        self.count -= 1;
        Some(event)
    }
}

/// Global mouse state
static MOUSE_PACKET: Mutex<MousePacketState> = Mutex::new(MousePacketState::new());
static MOUSE_BUFFER: Mutex<MouseBuffer> = Mutex::new(MouseBuffer::new());

/// Absolute mouse position (updated by the desktop layer)
static MOUSE_X: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);
static MOUSE_Y: core::sync::atomic::AtomicI32 = core::sync::atomic::AtomicI32::new(0);

/// Current button state (latest)
static MOUSE_BUTTONS: Mutex<MouseButtons> = Mutex::new(MouseButtons {
    left: false,
    right: false,
    middle: false,
});

/// Whether the mouse has been initialized
static MOUSE_INITIALIZED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

// ============================================================================
// Initialization
// ============================================================================

/// Wait for the 8042 controller input buffer to be empty (ready to receive commands)
fn wait_input() {
    for _ in 0..100_000 {
        if unsafe { inb(PS2_STATUS) } & STATUS_INPUT_FULL == 0 {
            return;
        }
    }
}

/// Wait for the 8042 controller output buffer to be full (data available)
fn wait_output() -> bool {
    for _ in 0..100_000 {
        if unsafe { inb(PS2_STATUS) } & STATUS_OUTPUT_FULL != 0 {
            return true;
        }
    }
    false
}

/// Send a command byte to the 8042 controller
fn ps2_command(cmd: u8) {
    wait_input();
    unsafe { outb(PS2_COMMAND, cmd); }
}

/// Write a data byte to the 8042 data port
fn ps2_write_data(data: u8) {
    wait_input();
    unsafe { outb(PS2_DATA, data); }
}

/// Read a data byte from the 8042 data port (with timeout)
fn ps2_read_data() -> Option<u8> {
    if wait_output() {
        Some(unsafe { inb(PS2_DATA) })
    } else {
        None
    }
}

/// Send a command to the mouse (via auxiliary port)
fn mouse_write(cmd: u8) {
    ps2_command(CMD_WRITE_AUX);
    ps2_write_data(cmd);
}

/// Send a command to the mouse and wait for ACK
fn mouse_write_ack(cmd: u8) -> bool {
    mouse_write(cmd);
    // Wait for ACK (0xFA)
    match ps2_read_data() {
        Some(MOUSE_ACK) => true,
        _ => false,
    }
}

/// Initialize the PS/2 mouse.
///
/// This enables the auxiliary PS/2 port (mouse), sends the initialization
/// commands, and enables mouse data reporting.
pub fn init() {
    log::info!("Initializing PS/2 mouse...");

    // Step 1: Enable the auxiliary (mouse) port on the 8042 controller
    ps2_command(CMD_ENABLE_AUX);

    // Step 2: Read the controller configuration byte
    ps2_command(CMD_READ_CONFIG);
    let config = match ps2_read_data() {
        Some(c) => c,
        None => {
            log::warn!("PS/2 mouse: failed to read controller config");
            return;
        }
    };

    // Step 3: Enable IRQ12 (bit 1) and ensure auxiliary clock is enabled (bit 5 = 0)
    let new_config = (config | 0x02) & !0x20;
    ps2_command(CMD_WRITE_CONFIG);
    ps2_write_data(new_config);

    // Step 4: Set defaults (resets sample rate, resolution, etc.)
    mouse_write_ack(MOUSE_SET_DEFAULTS);

    // Step 5: Try to enable scroll wheel (IntelliMouse protocol)
    // Send the magic sample rate sequence: 200, 100, 80
    // This MUST happen BEFORE enabling data reporting, otherwise the mouse
    // starts sending movement packets that interfere with the detection.
    let has_scroll = try_enable_scroll_wheel();
    {
        let mut packet = MOUSE_PACKET.lock();
        packet.has_scroll = has_scroll;
    }

    // Step 6: Enable data reporting — LAST, after all configuration is done.
    // Once enabled, the mouse will start sending movement/button packets.
    mouse_write_ack(MOUSE_ENABLE_DATA);

    // Flush any pending data
    for _ in 0..16 {
        if unsafe { inb(PS2_STATUS) } & STATUS_OUTPUT_FULL != 0 {
            unsafe { inb(PS2_DATA); }
        }
    }

    MOUSE_INITIALIZED.store(true, core::sync::atomic::Ordering::SeqCst);

    if has_scroll {
        log::info!("PS/2 mouse initialized (IntelliMouse with scroll wheel)");
    } else {
        log::info!("PS/2 mouse initialized (standard 3-button)");
    }
}

/// Try to enable the IntelliMouse scroll wheel by sending the magic
/// sample rate sequence (200, 100, 80). If device ID changes to 3,
/// scroll wheel is supported and packets are 4 bytes.
fn try_enable_scroll_wheel() -> bool {
    // Set sample rate to 200
    mouse_write_ack(MOUSE_SET_SAMPLE_RATE);
    mouse_write_ack(200);

    // Set sample rate to 100
    mouse_write_ack(MOUSE_SET_SAMPLE_RATE);
    mouse_write_ack(100);

    // Set sample rate to 80
    mouse_write_ack(MOUSE_SET_SAMPLE_RATE);
    mouse_write_ack(80);

    // Read device ID
    mouse_write_ack(MOUSE_GET_DEVICE_ID);
    match ps2_read_data() {
        Some(3) => true,  // IntelliMouse with scroll
        _ => false,       // Standard mouse
    }
}

// ============================================================================
// IRQ Handler
// ============================================================================

/// Called from the IRQ12 interrupt handler.
/// Reads one byte from the mouse and accumulates packets.
pub fn handle_irq() {
    let byte = unsafe { inb(PS2_DATA) };

    let mut packet = MOUSE_PACKET.lock();

    // Synchronization: Byte 0 must have bit 3 set (the "always 1" bit)
    if packet.byte_index == 0 && (byte & 0x08) == 0 {
        // Out-of-sync — discard and wait for a valid first byte
        return;
    }

    let idx = packet.byte_index as usize;
    packet.bytes[idx] = byte;
    packet.byte_index += 1;

    if packet.byte_index >= packet.packet_size() {
        // Complete packet — decode it
        let b0 = packet.bytes[0];
        let b1 = packet.bytes[1];
        let b2 = packet.bytes[2];

        // Decode button state
        let buttons = MouseButtons {
            left: b0 & 0x01 != 0,
            right: b0 & 0x02 != 0,
            middle: b0 & 0x04 != 0,
        };

        // Decode X movement (signed 9-bit value)
        let mut dx = b1 as i16;
        if b0 & 0x10 != 0 {
            dx |= !0xFF; // Sign-extend (X sign bit is bit 4 of byte 0)
        }
        // Check for X overflow
        if b0 & 0x40 != 0 {
            dx = 0; // Discard on overflow
        }

        // Decode Y movement (signed 9-bit value)
        let mut dy = b2 as i16;
        if b0 & 0x20 != 0 {
            dy |= !0xFF; // Sign-extend (Y sign bit is bit 5 of byte 0)
        }
        // Check for Y overflow
        if b0 & 0x80 != 0 {
            dy = 0;
        }

        // PS/2 mouse Y is inverted (positive = up), we want positive = down
        dy = -dy;

        let event = MouseEvent { dx, dy, buttons };

        // Update button state
        *MOUSE_BUTTONS.lock() = buttons;

        // Push to event buffer
        MOUSE_BUFFER.lock().push(event);

        // Reset for next packet
        packet.byte_index = 0;
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Read the next mouse event (non-blocking).
///
/// Disables interrupts while holding the lock to prevent deadlock:
/// the IRQ12 handler also locks MOUSE_BUFFER, and if an IRQ fires
/// while we hold the lock, the handler would spin forever.
pub fn try_read_event() -> Option<MouseEvent> {
    super::interrupts::without_interrupts(|| {
        MOUSE_BUFFER.lock().pop()
    })
}

/// Get current absolute mouse position.
pub fn position() -> (i32, i32) {
    let x = MOUSE_X.load(core::sync::atomic::Ordering::Relaxed);
    let y = MOUSE_Y.load(core::sync::atomic::Ordering::Relaxed);
    (x, y)
}

/// Set absolute mouse position (called by the desktop layer after applying deltas).
pub fn set_position(x: i32, y: i32) {
    MOUSE_X.store(x, core::sync::atomic::Ordering::Relaxed);
    MOUSE_Y.store(y, core::sync::atomic::Ordering::Relaxed);
}

/// Get current button state.
pub fn buttons() -> MouseButtons {
    super::interrupts::without_interrupts(|| {
        *MOUSE_BUTTONS.lock()
    })
}

/// Check if mouse has been initialized.
pub fn is_initialized() -> bool {
    MOUSE_INITIALIZED.load(core::sync::atomic::Ordering::Relaxed)
}
