// Virtual Filesystem (VFS) Layer
//
// Provides path-based file operations on top of the FAT32 driver.
// Maintains the current working directory and resolves paths.
//
// All paths use '/' as separator. Absolute paths start with '/'.
// Relative paths are resolved from the current working directory.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use crate::storage::fat32::{self, DirEntry};
use spin::Mutex;

/// Current working directory state
struct VfsState {
    /// Current directory cluster
    cwd_cluster: u32,
    /// Current path string
    cwd_path: String,
}

/// Global VFS state
static VFS: Mutex<Option<VfsState>> = Mutex::new(None);

/// Initialize the VFS with the root directory as CWD.
pub fn init() {
    let root = fat32::root_cluster();
    *VFS.lock() = Some(VfsState {
        cwd_cluster: root,
        cwd_path: String::from("/"),
    });
}

/// Check if VFS is initialized.
pub fn is_ready() -> bool {
    VFS.lock().is_some() && fat32::is_mounted()
}

/// Get the current working directory path.
pub fn cwd() -> String {
    let lock = VFS.lock();
    match lock.as_ref() {
        Some(s) => s.cwd_path.clone(),
        None => String::from("/"),
    }
}

/// Get the current working directory cluster.
pub fn cwd_cluster() -> u32 {
    let lock = VFS.lock();
    match lock.as_ref() {
        Some(s) => s.cwd_cluster,
        None => fat32::root_cluster(),
    }
}

/// Resolve a path to a (parent_cluster, entry_name) pair.
/// If the path points to a file/directory that exists, returns Some(DirEntry).
/// Handles both absolute and relative paths.
fn resolve_path(path: &str) -> Option<(u32, DirEntry)> {
    let (start_cluster, components) = parse_path(path);
    let mut current_cluster = start_cluster;

    if components.is_empty() {
        // Root directory
        return Some((current_cluster, DirEntry {
            name: String::from("/"),
            is_dir: true,
            size: 0,
            cluster: current_cluster,
            entry_sector: 0,
            entry_offset: 0,
        }));
    }

    let mut _parent_cluster = current_cluster;

    for (i, component) in components.iter().enumerate() {
        if component == &"." {
            continue;
        }
        if component == &".." {
            // Go up — we'd need parent tracking. For now, use root as fallback.
            // TODO: proper parent tracking
            current_cluster = fat32::root_cluster();
            _parent_cluster = current_cluster;
            continue;
        }

        match fat32::find_entry(current_cluster, component) {
            Some(entry) => {
                if i == components.len() - 1 {
                    // Last component — this is the target
                    return Some((current_cluster, entry));
                }
                if !entry.is_dir {
                    return None; // Non-final component is not a directory
                }
                _parent_cluster = current_cluster;
                current_cluster = entry.cluster;
            }
            None => return None,
        }
    }

    None
}

/// Resolve a path to the parent directory cluster and the final component name.
fn resolve_parent(path: &str) -> Option<(u32, String)> {
    let (start_cluster, components) = parse_path(path);

    if components.is_empty() {
        return None; // Can't get parent of root
    }

    let mut current_cluster = start_cluster;
    let final_name = components.last().unwrap().to_string();

    // Traverse all but the last component
    for component in &components[..components.len() - 1] {
        if *component == "." {
            continue;
        }
        if *component == ".." {
            current_cluster = fat32::root_cluster();
            continue;
        }
        match fat32::find_entry(current_cluster, component) {
            Some(entry) if entry.is_dir => {
                current_cluster = entry.cluster;
            }
            _ => return None,
        }
    }

    Some((current_cluster, final_name))
}

/// Parse a path into a starting cluster and component list.
fn parse_path(path: &str) -> (u32, Vec<&str>) {
    let is_absolute = path.starts_with('/');
    let start_cluster = if is_absolute {
        fat32::root_cluster()
    } else {
        cwd_cluster()
    };

    let components: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    (start_cluster, components)
}

// ============================================================================
// Public API
// ============================================================================

/// List directory contents at the given path.
pub fn list_dir(path: &str) -> Option<Vec<DirEntry>> {
    let cluster = if path.is_empty() || path == "." {
        cwd_cluster()
    } else if path == "/" {
        fat32::root_cluster()
    } else {
        match resolve_path(path) {
            Some((_, entry)) if entry.is_dir => entry.cluster,
            _ => return None,
        }
    };

    Some(fat32::read_dir(cluster))
}

/// Read a file at the given path.
pub fn read_file(path: &str) -> Option<Vec<u8>> {
    match resolve_path(path) {
        Some((_, entry)) if !entry.is_dir => Some(fat32::read_file(&entry)),
        _ => None,
    }
}

/// Write data to a file at the given path. Creates or overwrites.
pub fn write_file(path: &str, data: &[u8]) -> bool {
    match resolve_parent(path) {
        Some((parent_cluster, name)) => fat32::write_file(parent_cluster, &name, data),
        None => false,
    }
}

/// Create a directory at the given path.
pub fn mkdir(path: &str) -> bool {
    match resolve_parent(path) {
        Some((parent_cluster, name)) => fat32::mkdir(parent_cluster, &name),
        None => false,
    }
}

/// Delete a file or empty directory at the given path.
pub fn delete(path: &str) -> bool {
    match resolve_parent(path) {
        Some((parent_cluster, name)) => fat32::delete(parent_cluster, &name),
        None => false,
    }
}

/// Change the current working directory.
pub fn cd(path: &str) -> bool {
    let target_cluster;
    let new_path;

    if path == "/" {
        target_cluster = fat32::root_cluster();
        new_path = String::from("/");
    } else if path == ".." {
        // Go up one level
        let mut lock = VFS.lock();
        let state = match lock.as_mut() {
            Some(s) => s,
            None => return false,
        };

        if state.cwd_path == "/" {
            return true; // Already at root
        }

        // Find parent path
        let mut parts: Vec<&str> = state.cwd_path.split('/').filter(|s| !s.is_empty()).collect();
        parts.pop();

        target_cluster = if parts.is_empty() {
            fat32::root_cluster()
        } else {
            // Navigate to parent
            let mut cluster = fat32::root_cluster();
            for part in &parts {
                match fat32::find_entry(cluster, part) {
                    Some(e) if e.is_dir => cluster = e.cluster,
                    _ => return false,
                }
            }
            cluster
        };

        new_path = if parts.is_empty() {
            String::from("/")
        } else {
            let mut p = String::from("/");
            p.push_str(&parts.join("/"));
            p
        };

        state.cwd_cluster = target_cluster;
        state.cwd_path = new_path;
        return true;
    } else {
        match resolve_path(path) {
            Some((_, entry)) if entry.is_dir => {
                target_cluster = entry.cluster;
                let mut lock = VFS.lock();
                let state = match lock.as_mut() {
                    Some(s) => s,
                    None => return false,
                };

                if path.starts_with('/') {
                    new_path = if path.ends_with('/') {
                        path.trim_end_matches('/').to_string()
                    } else {
                        path.to_string()
                    };
                } else {
                    new_path = if state.cwd_path == "/" {
                        let mut p = String::from("/");
                        p.push_str(path);
                        p
                    } else {
                        let mut p = state.cwd_path.clone();
                        p.push('/');
                        p.push_str(path);
                        p
                    };
                }

                state.cwd_cluster = target_cluster;
                state.cwd_path = new_path;
                return true;
            }
            _ => return false,
        }
    }

    let mut lock = VFS.lock();
    if let Some(state) = lock.as_mut() {
        state.cwd_cluster = target_cluster;
        state.cwd_path = new_path;
    }
    true
}

/// Copy a file from src path to dst path.
pub fn copy_file(src: &str, dst: &str) -> bool {
    let data = match read_file(src) {
        Some(d) => d,
        None => return false,
    };
    write_file(dst, &data)
}

/// Check if a path exists.
pub fn exists(path: &str) -> bool {
    resolve_path(path).is_some()
}

/// Check if a path is a directory.
pub fn is_dir(path: &str) -> bool {
    match resolve_path(path) {
        Some((_, entry)) => entry.is_dir,
        None => false,
    }
}
