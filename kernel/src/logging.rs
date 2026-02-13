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

/// Our kernel logger implementation
struct KernelLogger;

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
