// PC Speaker Driver
//
// The PC speaker is controlled via:
//   - PIT Channel 2 (I/O ports 0x42/0x43) for tone generation
//   - Port 0x61 (system control port B) bits 0-1 to enable speaker
//
// PIT base frequency: 1,193,182 Hz
// Frequency = 1193182 / divisor

use super::port::{inb, outb};

const PIT_CHANNEL2: u16 = 0x42;
const PIT_COMMAND: u16 = 0x43;
const SYSTEM_CTRL_B: u16 = 0x61;
const PIT_FREQUENCY: u32 = 1_193_182;

/// Start playing a tone at the given frequency (Hz).
pub fn tone_on(frequency: u32) {
    if frequency == 0 {
        return;
    }

    let divisor = PIT_FREQUENCY / frequency;
    let divisor = if divisor > 0xFFFF { 0xFFFF } else { divisor as u16 };

    unsafe {
        // Program PIT channel 2 for square wave mode
        // 0xB6 = channel 2, lobyte/hibyte, mode 3 (square wave), binary
        outb(PIT_COMMAND, 0xB6);
        outb(PIT_CHANNEL2, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL2, ((divisor >> 8) & 0xFF) as u8);

        // Enable speaker: set bits 0 (gate) and 1 (speaker data)
        let ctrl = inb(SYSTEM_CTRL_B);
        outb(SYSTEM_CTRL_B, ctrl | 0x03);
    }
}

/// Stop the speaker.
pub fn tone_off() {
    unsafe {
        let ctrl = inb(SYSTEM_CTRL_B);
        outb(SYSTEM_CTRL_B, ctrl & !0x03);
    }
}

/// Play a tone for a given duration in milliseconds (blocking).
pub fn beep(frequency: u32, duration_ms: u64) {
    tone_on(frequency);
    crate::core_kernel::scheduler::sleep_ms(duration_ms);
    tone_off();
}

/// Play the CantayaOS startup chime — a pleasant ascending sequence.
pub fn startup_chime() {
    // C5 - E5 - G5 (major triad, quick)
    beep(523, 80);  // C5
    beep(659, 80);  // E5
    beep(784, 120); // G5
}

/// Play an error beep.
pub fn error_beep() {
    beep(200, 150);
}

/// Play a notification sound.
pub fn notify() {
    beep(880, 50);  // A5
    crate::core_kernel::scheduler::sleep_ms(30);
    beep(1047, 80); // C6
}
