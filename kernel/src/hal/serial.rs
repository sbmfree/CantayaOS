// Serial Port Driver (COM1)
//
// The serial port (UART 16550) is the most reliable debug output device.
// It works even when the framebuffer isn't set up, and QEMU can capture its
// output to the host terminal, making it invaluable for kernel debugging.
//
// COM1 is at I/O port 0x3F8. We initialize it at 115200 baud, 8N1 (8 data bits,
// no parity, 1 stop bit) — the standard configuration.
//
// In Windows, serial output is handled by KdCom (Kernel Debugger Communications).
// Our serial module serves a similar purpose.

use super::port::{inb, outb};
use core::fmt;
use spin::Mutex;

/// COM1 base I/O port
const COM1: u16 = 0x3F8;

/// Global serial port writer, protected by a spinlock for thread safety.
/// Using a spinlock (not a mutex) because we can't block in kernel context.
pub static SERIAL: Mutex<SerialPort> = Mutex::new(SerialPort { port: COM1 });

pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    /// Initialize the UART with standard settings (115200 baud, 8N1)
    pub fn init(&self) {
        unsafe {
            // Disable interrupts (we poll for now)
            outb(self.port + 1, 0x00);

            // Set baud rate divisor (115200 baud = divisor 1)
            outb(self.port + 3, 0x80); // Enable DLAB (set baud rate divisor)
            outb(self.port + 0, 0x01); // Divisor low byte (115200 baud)
            outb(self.port + 1, 0x00); // Divisor high byte

            // 8 bits, no parity, 1 stop bit (8N1)
            outb(self.port + 3, 0x03);

            // Enable FIFO with 14-byte threshold
            outb(self.port + 2, 0xC7);

            // Enable IRQs, RTS/DSR set (for future interrupt-driven I/O)
            outb(self.port + 4, 0x0B);
        }
    }

    /// Check if the transmit buffer is empty (ready to send)
    fn is_transmit_empty(&self) -> bool {
        unsafe { inb(self.port + 5) & 0x20 != 0 }
    }

    /// Send a single byte over the serial port
    pub fn write_byte(&self, byte: u8) {
        // Wait for the transmit buffer to be ready
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            outb(self.port, byte);
        }
    }

    /// Send a string over the serial port
    pub fn write_str(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                // Serial terminals expect \r\n line endings
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
    }
}

/// Implement fmt::Write so we can use write!/writeln! with the serial port
impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerialPort::write_str(self, s);
        Ok(())
    }
}

/// Initialize the serial port
pub fn init() {
    SERIAL.lock().init();
}

/// Print a formatted string to serial (used by the logging infrastructure)
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ({
        use core::fmt::Write;
        $crate::hal::serial::SERIAL.lock().write_fmt(format_args!($($arg)*)).unwrap();
    });
}

/// Print a formatted string to serial with a newline
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
