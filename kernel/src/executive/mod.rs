// Executive Layer
//
// The executive is the highest layer of the kernel, providing system services
// that user-mode processes will eventually use via system calls.
//
// In Windows NT, the Executive (Ex) includes:
//   - Process Manager (Ps) — process/thread lifecycle
//   - I/O Manager (Io) — device and file I/O dispatch
//   - Object Manager (Ob) — handle-based resource management
//   - Security Reference Monitor (Se) — access control
//   - Memory Manager (Mm) — virtual memory (we put ours lower for now)
//   - Cache Manager (Cc) — file cache
//   - Configuration Manager (Cm) — registry
//
// We stub these out now and implement them incrementally.
// Each module will grow as CantayaOS gains capabilities.

pub mod process;
pub mod object;
pub mod syscall;
