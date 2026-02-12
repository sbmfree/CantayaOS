//! System Log (syslog)
//!
//! Structured kernel and userspace logging facility inspired by
//! UNIX syslog. Captures timestamped, leveled log messages into
//! a ring buffer accessible via /var/log/syslog.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use spin::Mutex;

extern crate alloc;

/// Log severity levels (RFC 5424)
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub enum LogLevel {
    Emergency = 0,
    Alert = 1,
    Critical = 2,
    Error = 3,
    Warning = 4,
    Notice = 5,
    Info = 6,
    Debug = 7,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Emergency => "EMERG",
            LogLevel::Alert     => "ALERT",
            LogLevel::Critical  => "CRIT",
            LogLevel::Error     => "ERR",
            LogLevel::Warning   => "WARN",
            LogLevel::Notice    => "NOTICE",
            LogLevel::Info      => "INFO",
            LogLevel::Debug     => "DEBUG",
        }
    }

    pub fn from_str(s: &str) -> Option<LogLevel> {
        match s.to_ascii_lowercase().as_str() {
            "emerg" | "emergency" => Some(LogLevel::Emergency),
            "alert" => Some(LogLevel::Alert),
            "crit" | "critical" => Some(LogLevel::Critical),
            "err" | "error" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warning),
            "notice" => Some(LogLevel::Notice),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            _ => None,
        }
    }
}

/// Log facility
#[derive(Clone, Copy, Debug)]
pub enum Facility {
    Kernel,
    User,
    Daemon,
    Auth,
    Syslog,
}

impl Facility {
    pub fn as_str(&self) -> &'static str {
        match self {
            Facility::Kernel => "kern",
            Facility::User   => "user",
            Facility::Daemon => "daemon",
            Facility::Auth   => "auth",
            Facility::Syslog => "syslog",
        }
    }
}

/// A single log entry
#[derive(Clone)]
pub struct LogEntry {
    pub timestamp_ms: u64,
    pub level: LogLevel,
    pub facility: Facility,
    pub message: String,
}

/// Max entries in the log ring
const MAX_LOG_ENTRIES: usize = 256;

struct SyslogState {
    entries: Vec<LogEntry>,
    min_level: LogLevel,
}

static SYSLOG: Mutex<SyslogState> = Mutex::new(SyslogState {
    entries: Vec::new(),
    min_level: LogLevel::Debug,
});

/// Initialize syslog
pub fn init() {
    log(LogLevel::Info, Facility::Kernel, "syslog: logging subsystem initialized");
    log(LogLevel::Info, Facility::Kernel, "syslog: log level set to DEBUG");
}

/// Log a message
pub fn log(level: LogLevel, facility: Facility, message: &str) {
    let mut state = SYSLOG.lock();
    if (level as u8) > (state.min_level as u8) {
        return;
    }
    let entry = LogEntry {
        timestamp_ms: crate::hal::timer::uptime_ms(),
        level,
        facility,
        message: String::from(message),
    };
    state.entries.push(entry);
    // Evict oldest if over capacity
    if state.entries.len() > MAX_LOG_ENTRIES {
        state.entries.remove(0);
    }
}

/// Get all log entries
pub fn get_entries() -> Vec<LogEntry> {
    SYSLOG.lock().entries.clone()
}

/// Get last N entries
pub fn get_tail(n: usize) -> Vec<LogEntry> {
    let state = SYSLOG.lock();
    let start = if state.entries.len() > n { state.entries.len() - n } else { 0 };
    state.entries[start..].to_vec()
}

/// Get entries matching a minimum level
pub fn get_by_level(level: LogLevel) -> Vec<LogEntry> {
    let state = SYSLOG.lock();
    state.entries.iter()
        .filter(|e| (e.level as u8) <= (level as u8))
        .cloned()
        .collect()
}

/// Format a log entry for display
pub fn format_entry(entry: &LogEntry) -> String {
    let secs = entry.timestamp_ms / 1000;
    let ms = entry.timestamp_ms % 1000;
    format!("[{:>5}.{:03}] {}.{}: {}",
        secs, ms,
        entry.facility.as_str(),
        entry.level.as_str(),
        entry.message)
}

/// Generate /var/log/syslog content
pub fn generate_syslog_content() -> String {
    let entries = get_entries();
    let mut s = String::new();
    for entry in &entries {
        s.push_str(&format_entry(entry));
        s.push('\n');
    }
    s
}

/// Entry count
pub fn entry_count() -> usize {
    SYSLOG.lock().entries.len()
}
