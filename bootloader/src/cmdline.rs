// Kernel Command Line Loader
//
// Loads a kernel command line from \cantaya\cmdline.txt on the EFI System Partition.
// If the file is missing or empty, returns an empty command line (not an error).
//
// The command line is stored as a UTF-8 byte array (max 256 bytes) in BootInfo.
// The kernel can parse it for options like "verbose", "debug", "nosmp", etc.

use uefi::CStr16;
use uefi::proto::media::file::{File, FileAttribute, FileMode};
use uefi::boot;

/// Maximum command line length (matches BootInfo::command_line capacity).
const MAX_CMDLINE_LEN: usize = 256;

/// Path to the command line file on the ESP.
const CMDLINE_PATH: &str = "\\cantaya\\cmdline.txt";

/// Load the kernel command line from the ESP filesystem.
///
/// Returns `(buffer, length)` where `buffer` contains the UTF-8 bytes
/// and `length` is the number of valid bytes. Leading/trailing whitespace
/// is stripped. If the file is not found, returns `([0; 256], 0)`.
pub fn load_command_line() -> ([u8; MAX_CMDLINE_LEN], usize) {
    let mut buf = [0u8; MAX_CMDLINE_LEN];

    // Locate the simple filesystem protocol on the boot device
    let fs_handle = match boot::get_handle_for_protocol::<uefi::proto::media::fs::SimpleFileSystem>() {
        Ok(h) => h,
        Err(_) => return (buf, 0),
    };

    let mut fs = match boot::open_protocol_exclusive::<uefi::proto::media::fs::SimpleFileSystem>(fs_handle) {
        Ok(f) => f,
        Err(_) => return (buf, 0),
    };

    let mut root = match fs.open_volume() {
        Ok(r) => r,
        Err(_) => return (buf, 0),
    };

    // Convert the path to UCS-2 for UEFI
    let mut path_buf = [0u16; 64];
    let path_ucs2 = match str_to_ucs2(CMDLINE_PATH, &mut path_buf) {
        Some(p) => p,
        None => return (buf, 0),
    };

    // Open the file (read-only)
    let file_handle = match root.open(path_ucs2, FileMode::Read, FileAttribute::empty()) {
        Ok(h) => h,
        Err(_) => {
            // File not found is normal — return empty
            return (buf, 0);
        }
    };

    let mut regular_file = match file_handle.into_regular_file() {
        Some(f) => f,
        None => return (buf, 0),
    };

    // Read the file contents (up to MAX_CMDLINE_LEN bytes)
    let bytes_read = match regular_file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return (buf, 0),
    };

    // Trim trailing whitespace and newlines
    let mut len = bytes_read;
    while len > 0 && (buf[len - 1] == b' ' || buf[len - 1] == b'\n' || buf[len - 1] == b'\r' || buf[len - 1] == b'\t') {
        len -= 1;
    }

    // Trim leading whitespace
    let mut start = 0;
    while start < len && (buf[start] == b' ' || buf[start] == b'\t') {
        start += 1;
    }

    // Shift trimmed content to the beginning
    if start > 0 && start < len {
        let trimmed_len = len - start;
        let mut new_buf = [0u8; MAX_CMDLINE_LEN];
        new_buf[..trimmed_len].copy_from_slice(&buf[start..len]);
        return (new_buf, trimmed_len);
    }

    (buf, len)
}

/// Convert a Rust &str to a null-terminated UCS-2 string in the provided buffer.
/// Returns the CStr16 slice, or None if the buffer is too small.
fn str_to_ucs2<'a>(s: &str, buf: &'a mut [u16]) -> Option<&'a CStr16> {
    let chars: usize = s.chars().count();
    if chars + 1 > buf.len() {
        return None;
    }
    for (i, ch) in s.chars().enumerate() {
        buf[i] = ch as u16;
    }
    buf[chars] = 0; // null terminator
    CStr16::from_u16_with_nul(&buf[..chars + 1]).ok()
}
