//! Init Process — first user-space program
//!
//! Reads the /bin/init ELF binary from the RAM filesystem and launches it
//! as a user-space process via `spawn_user_process()`.

extern crate alloc;
use alloc::vec::Vec;
use crate::fs::{self, AccessMode};
use crate::process;
use crate::process::scheduler;

/// Read a file from the VFS into a `Vec<u8>`.
fn read_file_bytes(path: &str) -> Option<Vec<u8>> {
    let mut handle = fs::open(path, AccessMode::Read).ok()?;
    let mut content = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match fs::read(&mut handle, &mut buf) {
            Ok(0) => break,
            Ok(n) => content.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    fs::close(handle);
    Some(content)
}

/// Launch the init user-space process.
///
/// Called once from `kernel_main` after all subsystems are initialised.
/// Loads the ELF binary from `/bin/init` in ramfs and spawns it.
pub fn launch() {
    let elf_data = match read_file_bytes("/bin/init") {
        Some(data) => data,
        None => {
            crate::kprintln!("[init] ERROR: /bin/init not found in ramfs");
            return;
        }
    };

    crate::kprintln!("[init] Loaded /bin/init ({} bytes)", elf_data.len());

    match process::spawn_user_process("init", &elf_data, scheduler::PRIORITY_NORMAL, &["init"]) {
        Some(pid) => {
            crate::kprintln!("[init] User-space init launched (PID {})", pid);
        }
        None => {
            crate::kprintln!("[init] ERROR: failed to spawn init process");
        }
    }
}
