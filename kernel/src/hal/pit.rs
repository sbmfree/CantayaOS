// Programmable Interval Timer (PIT) — Intel 8253/8254
//
// The PIT is the classic x86 timer that generates periodic interrupts.
// Channel 0 is connected to IRQ0 (vector 32 after PIC remap) and drives
// the system clock and preemptive scheduler.
//
// The PIT has an internal oscillator at 1,193,182 Hz. We program a divisor
// to get the desired tick rate. For example:
//   - Divisor 1193 → ~1000 Hz (1 ms per tick)
//   - Divisor 65536 → ~18.2 Hz (default, ~54.9 ms per tick)
//
// In Windows NT, the HAL programs the PIT (or APIC timer) to ~15.6 ms
// for the system clock, with high-resolution timers using HPET or TSC.
//
// Registers:
//   Port 0x40: Channel 0 data (read/write counter value)
//   Port 0x43: Mode/Command register

use super::port::outb;

/// PIT oscillator frequency in Hz
const PIT_FREQUENCY: u32 = 1_193_182;

/// Configured tick rate (ticks per second)
static TICK_RATE_HZ: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// Initialize the PIT Channel 0 to fire at the given frequency (Hz).
///
/// Common values:
///   - 100 Hz  → 10 ms per tick (Linux default for a long time)
///   - 250 Hz  → 4 ms per tick
///   - 1000 Hz → 1 ms per tick (best granularity)
///
/// We use 1000 Hz for millisecond-accurate uptime and responsive scheduling.
pub fn init(frequency_hz: u32) {
    let divisor = PIT_FREQUENCY / frequency_hz;

    // Clamp to valid range (16-bit counter)
    let divisor = if divisor > 65535 { 65535 } else if divisor < 1 { 1 } else { divisor };

    TICK_RATE_HZ.store(frequency_hz, core::sync::atomic::Ordering::Relaxed);

    unsafe {
        // Command byte: Channel 0, Access: lobyte/hibyte, Mode 3 (square wave), Binary
        // Bits 7-6: 00 = Channel 0
        // Bits 5-4: 11 = Access lobyte then hibyte
        // Bits 3-1: 011 = Mode 3 (square wave generator)
        // Bit 0:    0 = 16-bit binary
        outb(0x43, 0x36);

        // Send divisor (low byte first, then high byte)
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, ((divisor >> 8) & 0xFF) as u8);
    }

    log::info!("PIT: Channel 0 set to {} Hz (divisor {})", frequency_hz, divisor);
}

/// Get the configured tick rate in Hz.
pub fn tick_rate_hz() -> u32 {
    TICK_RATE_HZ.load(core::sync::atomic::Ordering::Relaxed)
}

/// Convert a tick count to milliseconds using the configured rate.
pub fn ticks_to_ms(ticks: u64) -> u64 {
    let rate = tick_rate_hz() as u64;
    if rate == 0 { return 0; }
    ticks * 1000 / rate
}
