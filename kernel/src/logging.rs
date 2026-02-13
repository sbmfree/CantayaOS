// Kernel Logging Infrastructure
//
// Integrates with the `log` crate to provide kernel-wide logging.
// All log output goes to the serial port (COM1) for reliable debugging.
//
// Usage throughout the kernel:
//   log::info!("Message");
//   log::error!("Error: {}", details);
//   log::debug!("Debug: {:?}", struct);
//
// Log levels:
//   Error — unrecoverable or severe issues
//   Warn  — recoverable issues that need attention
//   Info  — important status messages
//   Debug — detailed debugging information
//   Trace — very verbose per-operation logging

use log::{LevelFilter, Log, Metadata, Record};
use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

extern crate alloc;

/// Ring buffer for kernel log messages (dmesg)
/// Stores the most recent LOG_RING_CAPACITY entries
const LOG_RING_CAPACITY: usize = 256;
const LOG_ENTRY_MAX: usize = 120;

struct LogEntry {
    buf: [u8; LOG_ENTRY_MAX],
    len: usize,
}

impl LogEntry {
    const fn empty() -> Self {
        Self { buf: [0u8; LOG_ENTRY_MAX], len: 0 }
    }
}

struct LogRing {
    entries: [LogEntry; LOG_RING_CAPACITY],
    write_pos: usize,
    count: usize,
}

impl LogRing {
    const fn new() -> Self {
        const EMPTY: LogEntry = LogEntry::empty();
        Self {
            entries: [EMPTY; LOG_RING_CAPACITY],
            write_pos: 0,
            count: 0,
        }
    }

    fn push(&mut self, msg: &str) {
        let entry = &mut self.entries[self.write_pos];
        let len = msg.len().min(LOG_ENTRY_MAX);
        entry.buf[..len].copy_from_slice(&msg.as_bytes()[..len]);
        entry.len = len;
        self.write_pos = (self.write_pos + 1) % LOG_RING_CAPACITY;
        if self.count < LOG_RING_CAPACITY {
            self.count += 1;
        }
    }
}

static LOG_RING: Mutex<LogRing> = Mutex::new(LogRing::new());
static LOG_SEQ: AtomicUsize = AtomicUsize::new(0);

/// Get all log entries from the ring buffer (oldest first)
pub fn get_log_entries() -> alloc::vec::Vec<alloc::string::String> {
    let ring = LOG_RING.lock();
    let mut result = alloc::vec::Vec::with_capacity(ring.count);

    if ring.count < LOG_RING_CAPACITY {
        // Haven't wrapped yet — entries are 0..count
        for i in 0..ring.count {
            if let Ok(s) = core::str::from_utf8(&ring.entries[i].buf[..ring.entries[i].len]) {
                result.push(alloc::string::String::from(s));
            }
        }
    } else {
        // Wrapped — oldest is at write_pos, newest at write_pos-1
        for offset in 0..LOG_RING_CAPACITY {
            let idx = (ring.write_pos + offset) % LOG_RING_CAPACITY;
            if let Ok(s) = core::str::from_utf8(&ring.entries[idx].buf[..ring.entries[idx].len]) {
                result.push(alloc::string::String::from(s));
            }
        }
    }
    result
}

/// Get the total number of log messages ever recorded
pub fn log_count() -> usize {
    LOG_SEQ.load(Ordering::Relaxed)
}

/// Our kernel logger implementation
struct KernelLogger;

/// Helper to format into a fixed-size buffer
struct FmtBuf {
    buf: [u8; LOG_ENTRY_MAX],
    pos: usize,
}

impl FmtBuf {
    fn new() -> Self { Self { buf: [0u8; LOG_ENTRY_MAX], pos: 0 } }
    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.pos]).unwrap_or("")
    }
}

impl Write for FmtBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = LOG_ENTRY_MAX - self.pos;
        let len = bytes.len().min(remaining);
        self.buf[self.pos..self.pos + len].copy_from_slice(&bytes[..len]);
        self.pos += len;
        Ok(())
    }
}

impl Log for KernelLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            // Format: [LEVEL] target: message
            crate::serial_println!(
                "[{:5}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );

            // Also store in ring buffer for dmesg
            let mut fmt = FmtBuf::new();
            let seq = LOG_SEQ.fetch_add(1, Ordering::Relaxed);
            let _ = write!(fmt, "[{:5}] #{:>4} {}", record.level(), seq, record.args());
            if let Some(mut ring) = LOG_RING.try_lock() {
                ring.push(fmt.as_str());
            }
        }
    }

    fn flush(&self) {
        // Serial port writes are immediate — nothing to flush
    }
}

static LOGGER: KernelLogger = KernelLogger;

/// Initialize the kernel logging system.
///
/// After this call, all `log::info!()` etc. macros will work throughout the kernel.
pub fn init() {
    log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(LevelFilter::Info))
        .expect("Failed to set kernel logger");
}
