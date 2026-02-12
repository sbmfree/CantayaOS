//! Kernel Log Ring Buffer
//!
//! Captures all kernel output into a fixed-size ring buffer
//! so it can be replayed later via `dmesg`.

use spin::Mutex;
use core::fmt::{self, Write};

/// Log buffer size (64 KB)
const LOG_BUF_SIZE: usize = 64 * 1024;

struct LogBuffer {
    buf: [u8; LOG_BUF_SIZE],
    head: usize,
    count: usize,
}

impl LogBuffer {
    const fn new() -> Self {
        LogBuffer {
            buf: [0; LOG_BUF_SIZE],
            head: 0,
            count: 0,
        }
    }

    fn push_byte(&mut self, byte: u8) {
        self.buf[self.head] = byte;
        self.head = (self.head + 1) % LOG_BUF_SIZE;
        if self.count < LOG_BUF_SIZE {
            self.count += 1;
        }
    }

    /// Read the entire log into a closure, oldest first
    fn read_all<F: FnMut(u8)>(&self, mut f: F) {
        if self.count < LOG_BUF_SIZE {
            // Buffer hasn't wrapped
            for i in 0..self.count {
                f(self.buf[i]);
            }
        } else {
            // Buffer has wrapped — read from head (oldest) to end, then 0 to head
            for i in self.head..LOG_BUF_SIZE {
                f(self.buf[i]);
            }
            for i in 0..self.head {
                f(self.buf[i]);
            }
        }
    }

    /// Read the last `n` lines
    fn read_tail<F: FnMut(u8)>(&self, n: usize, mut f: F) {
        // First pass: count newlines from the end
        let mut lines_found = 0;
        let mut start_pos = 0;
        let total = self.count;

        if total == 0 {
            return;
        }

        // Walk backwards through the buffer to find where the Nth-from-last newline is
        let mut positions = 0;
        for i in (0..total).rev() {
            let idx = if self.count < LOG_BUF_SIZE {
                i
            } else {
                (self.head + i) % LOG_BUF_SIZE
            };
            if self.buf[idx] == b'\n' {
                lines_found += 1;
                if lines_found == n + 1 {
                    start_pos = i + 1;
                    break;
                }
            }
            positions += 1;
            let _ = positions;
        }

        // Now read from start_pos to end
        for i in start_pos..total {
            let idx = if self.count < LOG_BUF_SIZE {
                i
            } else {
                (self.head + i) % LOG_BUF_SIZE
            };
            f(self.buf[idx]);
        }
    }
}

impl Write for LogBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            self.push_byte(byte);
        }
        Ok(())
    }
}

static LOG: Mutex<LogBuffer> = Mutex::new(LogBuffer::new());

/// Record a string into the kernel log
pub fn log(args: fmt::Arguments) {
    let _ = LOG.lock().write_fmt(args);
}

/// Dump the entire log to the console
pub fn dump_all() {
    let log = LOG.lock();
    let console = &crate::hal::console::CONSOLE;
    log.read_all(|byte| {
        let mut c = console.lock();
        c.write_byte(byte);
    });
}

/// Dump the last `n` lines to the console
pub fn dump_tail(n: usize) {
    let log = LOG.lock();
    let console = &crate::hal::console::CONSOLE;
    log.read_tail(n, |byte| {
        let mut c = console.lock();
        c.write_byte(byte);
    });
}

/// Get total bytes in the log
pub fn log_size() -> usize {
    LOG.lock().count
}
