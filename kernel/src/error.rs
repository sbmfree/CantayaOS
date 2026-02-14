// Kernel Error Types
//
// A unified error type for the CantayaOS kernel. All subsystems should use
// `KernelResult<T>` for fallible operations instead of ad-hoc bool/Option returns.
//
// Modeled loosely on Windows NTSTATUS codes — each error variant carries enough
// context for the caller to make a recovery decision.

use alloc::string::String;
use core::fmt;

/// Convenience alias used throughout the kernel.
pub type KernelResult<T> = Result<T, KernelError>;

/// Unified kernel error type.
#[derive(Debug, Clone)]
pub enum KernelError {
    // ── Memory ──────────────────────────────────────────────────────────
    /// Physical or virtual memory exhausted.
    OutOfMemory,
    /// Address is not mapped or falls outside valid ranges.
    InvalidAddress(u64),
    /// Alignment requirement not satisfied.
    BadAlignment { address: u64, required: usize },

    // ── Process / Thread ────────────────────────────────────────────────
    /// The requested process or thread ID does not exist.
    InvalidPid(u64),
    /// Operation not permitted (wrong privilege level, etc.).
    PermissionDenied,
    /// The executive object or subsystem is not yet implemented.
    NotImplemented(&'static str),

    // ── I/O & Storage ───────────────────────────────────────────────────
    /// Generic I/O failure (disk, serial, network …).
    IoError(&'static str),
    /// Path does not resolve to an existing entry.
    NotFound(String),
    /// Path already exists when it should not (e.g., mkdir on existing dir).
    AlreadyExists(String),
    /// Supplied buffer is too small for the requested operation.
    BufferTooSmall { needed: usize, provided: usize },
    /// The filesystem is full — no free clusters or inodes.
    DiskFull,
    /// The entry is not a directory when one was expected.
    NotADirectory(String),
    /// The entry is a directory when a file was expected.
    IsADirectory(String),
    /// The directory is not empty (for rmdir).
    DirectoryNotEmpty(String),

    // ── Network ─────────────────────────────────────────────────────────
    /// A network operation timed out.
    Timeout,
    /// The requested network resource is unavailable.
    NetworkUnavailable,
    /// The address or port is already in use.
    AddressInUse,
    /// Connection was refused by the remote host.
    ConnectionRefused,

    // ── Syscall / ABI ───────────────────────────────────────────────────
    /// Unknown or invalid syscall number.
    InvalidSyscall(u64),
    /// One of the syscall arguments is invalid.
    InvalidArgument(&'static str),

    // ── Catch-all ───────────────────────────────────────────────────────
    /// A miscellaneous internal error with a descriptive message.
    Internal(&'static str),
}

impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfMemory => write!(f, "out of memory"),
            Self::InvalidAddress(a) => write!(f, "invalid address: {:#X}", a),
            Self::BadAlignment { address, required } => {
                write!(f, "address {:#X} not aligned to {} bytes", address, required)
            }
            Self::InvalidPid(pid) => write!(f, "invalid PID: {}", pid),
            Self::PermissionDenied => write!(f, "permission denied"),
            Self::NotImplemented(what) => write!(f, "not implemented: {}", what),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
            Self::NotFound(path) => write!(f, "not found: {}", path),
            Self::AlreadyExists(path) => write!(f, "already exists: {}", path),
            Self::BufferTooSmall { needed, provided } => {
                write!(f, "buffer too small: need {} bytes, have {}", needed, provided)
            }
            Self::DiskFull => write!(f, "disk full"),
            Self::NotADirectory(p) => write!(f, "not a directory: {}", p),
            Self::IsADirectory(p) => write!(f, "is a directory: {}", p),
            Self::DirectoryNotEmpty(p) => write!(f, "directory not empty: {}", p),
            Self::Timeout => write!(f, "operation timed out"),
            Self::NetworkUnavailable => write!(f, "network unavailable"),
            Self::AddressInUse => write!(f, "address already in use"),
            Self::ConnectionRefused => write!(f, "connection refused"),
            Self::InvalidSyscall(n) => write!(f, "invalid syscall: {}", n),
            Self::InvalidArgument(msg) => write!(f, "invalid argument: {}", msg),
            Self::Internal(msg) => write!(f, "internal error: {}", msg),
        }
    }
}

/// Convert a KernelError into an integer status code (for syscall returns).
///
/// Negative values indicate errors (similar to Linux errno negation).
/// These codes are part of the kernel ABI — do not renumber.
impl KernelError {
    pub fn to_status_code(&self) -> i64 {
        match self {
            Self::OutOfMemory => -1,
            Self::InvalidAddress(_) => -2,
            Self::BadAlignment { .. } => -3,
            Self::InvalidPid(_) => -4,
            Self::PermissionDenied => -5,
            Self::NotImplemented(_) => -6,
            Self::IoError(_) => -7,
            Self::NotFound(_) => -8,
            Self::AlreadyExists(_) => -9,
            Self::BufferTooSmall { .. } => -10,
            Self::DiskFull => -11,
            Self::NotADirectory(_) => -12,
            Self::IsADirectory(_) => -13,
            Self::DirectoryNotEmpty(_) => -14,
            Self::Timeout => -15,
            Self::NetworkUnavailable => -16,
            Self::AddressInUse => -17,
            Self::ConnectionRefused => -18,
            Self::InvalidSyscall(_) => -19,
            Self::InvalidArgument(_) => -20,
            Self::Internal(_) => -99,
        }
    }
}
