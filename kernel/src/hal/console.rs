//! Console I/O via UART (PL011)
//!
//! Provides both output (write) and input (read) via the PL011 UART.

use core::fmt::{self, Write};
use core::sync::atomic::{AtomicU32, Ordering};
use crate::sync::IrqMutex;

/// PID of the foreground user-mode process (0 = none).
/// When Ctrl+C is received via UART IRQ, this process is terminated.
static FOREGROUND_PID: AtomicU32 = AtomicU32::new(0);

/// Set the foreground process PID (0 to clear).
pub fn set_foreground_pid(pid: u32) {
    FOREGROUND_PID.store(pid, Ordering::SeqCst);
}

/// Get the foreground process PID.
pub fn foreground_pid() -> u32 {
    FOREGROUND_PID.load(Ordering::SeqCst)
}

/// PL011 UART base address (QEMU virt machine)
const UART_BASE: usize = 0x0900_0000;

/// UART Registers
const UART_DR: usize = 0x00;    // Data Register
const UART_FR: usize = 0x18;    // Flag Register
const UART_IBRD: usize = 0x24;  // Integer Baud Rate
const UART_FBRD: usize = 0x28;  // Fractional Baud Rate
const UART_LCR: usize = 0x2C;   // Line Control
const UART_CR: usize = 0x30;    // Control Register
const UART_IMSC: usize = 0x38;  // Interrupt Mask Set/Clear
#[allow(dead_code)]
const UART_ICR: usize = 0x44;   // Interrupt Clear Register

/// Flag Register bits
const FR_TXFF: u32 = 1 << 5;  // Transmit FIFO full
const FR_RXFE: u32 = 1 << 4;  // Receive FIFO empty

/// Input buffer size
const INPUT_BUF_SIZE: usize = 256;

/// Ring buffer for UART input
struct InputBuffer {
    buf: [u8; INPUT_BUF_SIZE],
    head: usize,
    tail: usize,
}

impl InputBuffer {
    const fn new() -> Self {
        InputBuffer {
            buf: [0; INPUT_BUF_SIZE],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        let next = (self.head + 1) % INPUT_BUF_SIZE;
        if next != self.tail {
            self.buf[self.head] = byte;
            self.head = next;
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.head == self.tail {
            None
        } else {
            let byte = self.buf[self.tail];
            self.tail = (self.tail + 1) % INPUT_BUF_SIZE;
            Some(byte)
        }
    }

    fn is_empty(&self) -> bool {
        self.head == self.tail
    }
}

pub static CONSOLE: IrqMutex<Console> = IrqMutex::new(Console::new());
static INPUT: IrqMutex<InputBuffer> = IrqMutex::new(InputBuffer::new());

pub struct Console {
    base: usize,
}

impl Console {
    const fn new() -> Self {
        Console { base: UART_BASE }
    }
    
    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            // Wait until TX FIFO is not full
            while (self.read_reg(UART_FR) & FR_TXFF) != 0 {}
            self.write_reg(UART_DR, byte as u32);
        }
    }

    fn read_byte_nonblocking(&self) -> Option<u8> {
        unsafe {
            if (self.read_reg(UART_FR) & FR_RXFE) == 0 {
                Some(self.read_reg(UART_DR) as u8)
            } else {
                None
            }
        }
    }
    
    unsafe fn read_reg(&self, offset: usize) -> u32 {
        core::ptr::read_volatile((self.base + offset) as *const u32)
    }
    
    unsafe fn write_reg(&self, offset: usize, val: u32) {
        core::ptr::write_volatile((self.base + offset) as *mut u32, val);
    }
}

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

/// Initialize console
pub fn init() {
    let console = CONSOLE.lock();
    unsafe {
        // Disable UART
        console.write_reg(UART_CR, 0);
        
        // Set baud rate (115200 @ 24MHz clock)
        console.write_reg(UART_IBRD, 13);
        console.write_reg(UART_FBRD, 1);
        
        // 8N1, enable FIFO
        console.write_reg(UART_LCR, (0b11 << 5) | (1 << 4));
        
        // Enable RX interrupt
        console.write_reg(UART_IMSC, 1 << 4); // RXIM
        
        // Enable UART, TX, RX
        console.write_reg(UART_CR, (1 << 0) | (1 << 8) | (1 << 9));
    }
}

/// Handle UART RX interrupt — called from IRQ handler
pub fn handle_uart_irq() {
    let console = CONSOLE.lock();
    // Read all available characters
    while let Some(byte) = console.read_byte_nonblocking() {
        drop(console);
        // Ctrl+C: if a foreground process is running, terminate it
        if byte == 0x03 {
            let fg = FOREGROUND_PID.load(Ordering::SeqCst);
            if fg != 0 {
                crate::kprintln!("^C");
                crate::process::terminate_process(fg, -2); // -2 = killed by signal
                FOREGROUND_PID.store(0, Ordering::SeqCst);
                return;
            }
        }
        INPUT.lock().push(byte);
        return; // Only read one at a time to avoid re-locking issues
    }
}

/// Read one character (non-blocking). Returns None if no input available.
pub fn read_char() -> Option<u8> {
    // First, poll UART directly for any pending data
    {
        let console = CONSOLE.lock();
        while let Some(byte) = console.read_byte_nonblocking() {
            drop(console);
            INPUT.lock().push(byte);
            break;
        }
    }
    INPUT.lock().pop()
}

/// Read one character (blocking). Yields to scheduler until a character is available.
pub fn read_char_blocking() -> u8 {
    loop {
        if let Some(ch) = read_char() {
            return ch;
        }
        // Yield to other threads — lets user-space processes run while the
        // shell waits for input.
        crate::process::scheduler::yield_thread();
    }
}

/// Read a line of input with echo and basic line editing.
/// Returns the number of bytes written to `buf` (not including CR/LF).
pub fn read_line(buf: &mut [u8]) -> usize {
    let mut pos = 0;
    loop {
        let ch = read_char_blocking();
        match ch {
            // Enter (CR or LF)
            b'\r' | b'\n' => {
                write_byte(b'\r');
                write_byte(b'\n');
                break;
            }
            // Backspace / DEL
            0x08 | 0x7F => {
                if pos > 0 {
                    pos -= 1;
                    // Erase character on screen
                    write_byte(0x08);
                    write_byte(b' ');
                    write_byte(0x08);
                }
            }
            // Escape sequences (arrow keys etc.) — consume and ignore
            0x1B => {
                // Try to eat the rest of the escape sequence
                let _b1 = read_char_blocking();
                let _b2 = read_char_blocking();
            }
            // Tab — insert spaces
            b'\t' => {
                let spaces = 4 - (pos % 4);
                for _ in 0..spaces {
                    if pos < buf.len() {
                        buf[pos] = b' ';
                        pos += 1;
                        write_byte(b' ');
                    }
                }
            }
            // Ctrl-C — cancel line
            0x03 => {
                write_byte(b'^');
                write_byte(b'C');
                write_byte(b'\r');
                write_byte(b'\n');
                pos = 0;
                break;
            }
            // Ctrl-U — clear line
            0x15 => {
                while pos > 0 {
                    pos -= 1;
                    write_byte(0x08);
                    write_byte(b' ');
                    write_byte(0x08);
                }
            }
            // Printable characters
            ch if ch >= 0x20 && ch < 0x7F => {
                if pos < buf.len() {
                    buf[pos] = ch;
                    pos += 1;
                    write_byte(ch); // Echo
                }
            }
            _ => {}
        }
    }
    pos
}

/// Write a single byte (public helper)
pub fn write_byte(byte: u8) {
    CONSOLE.lock().write_byte(byte);
}

/// Check if input is available
pub fn has_input() -> bool {
    // Poll the UART
    {
        let console = CONSOLE.lock();
        if let Some(byte) = console.read_byte_nonblocking() {
            drop(console);
            INPUT.lock().push(byte);
        }
    }
    !INPUT.lock().is_empty()
}

/// Print formatted string (internal)
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    CONSOLE.lock().write_fmt(args).unwrap();
}
