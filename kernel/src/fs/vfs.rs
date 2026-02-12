//! Virtual File System (VFS)
//!
//! Windows NT I/O Manager-like VFS that dispatches file operations
//! to registered filesystem drivers using a unified namespace tree.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use core::sync::atomic::{AtomicU32, Ordering};

use super::{AccessMode, FileType, DirEntry, FileInfo, IoResult, IoError};

extern crate alloc;

/// File handle returned to callers
#[derive(Debug)]
pub struct FileHandle {
    pub id: u32,
    pub mount_id: u32,
    pub inode: u64,
    pub position: u64,
    pub mode: AccessMode,
}

/// Filesystem driver trait – each FS implements this
pub trait Filesystem: Send + Sync {
    fn name(&self) -> &str;

    fn open(&self, path: &str, mode: AccessMode) -> IoResult<(u64, FileType)>;
    fn create_file(&self, path: &str) -> IoResult<u64>;
    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> IoResult<usize>;
    fn write(&self, inode: u64, offset: u64, buf: &[u8]) -> IoResult<usize>;
    fn close(&self, inode: u64);

    fn mkdir(&self, path: &str) -> IoResult<()>;
    fn readdir(&self, path: &str) -> IoResult<Vec<DirEntry>>;
    fn stat(&self, path: &str) -> IoResult<FileInfo>;
    fn unlink(&self, path: &str) -> IoResult<()>;
}

/// Mount point entry
struct MountPoint {
    id: u32,
    prefix: String,
    fs: Box<dyn Filesystem>,
}

static MOUNTS: Mutex<Vec<MountPoint>> = Mutex::new(Vec::new());
static NEXT_MOUNT_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_HANDLE_ID: AtomicU32 = AtomicU32::new(1);

/// Initialize VFS
pub fn init() {
    // Root mount will be added by ramfs::init
}

/// Register (mount) a filesystem at the given path prefix
pub fn mount(prefix: &str, fs: Box<dyn Filesystem>) -> u32 {
    let id = NEXT_MOUNT_ID.fetch_add(1, Ordering::SeqCst);
    MOUNTS.lock().push(MountPoint {
        id,
        prefix: String::from(prefix),
        fs,
    });
    id
}

/// Find the mount point for a given path and return (mount_id, relative_path)
fn resolve_mount(path: &str) -> IoResult<(u32, String)> {
    let mounts = MOUNTS.lock();
    let mut best: Option<(u32, &str)> = None;
    let mut best_len = 0;

    for mount in mounts.iter() {
        if path.starts_with(mount.prefix.as_str()) && mount.prefix.len() >= best_len {
            best_len = mount.prefix.len();
            best = Some((mount.id, mount.prefix.as_str()));
        }
    }

    match best {
        Some((id, prefix)) => {
            let relative = &path[prefix.len()..];
            let relative = if relative.is_empty() { "/" } else { relative };
            Ok((id, String::from(relative)))
        }
        None => Err(IoError::NotFound),
    }
}

/// Get reference to filesystem by mount ID (runs closure with &dyn Filesystem)
fn with_fs<F, T>(mount_id: u32, f: F) -> IoResult<T>
where
    F: FnOnce(&dyn Filesystem) -> IoResult<T>,
{
    let mounts = MOUNTS.lock();
    for mount in mounts.iter() {
        if mount.id == mount_id {
            return f(mount.fs.as_ref());
        }
    }
    Err(IoError::NotFound)
}

/// Open a file
pub fn open(path: &str, mode: AccessMode) -> IoResult<FileHandle> {
    let (mount_id, relative) = resolve_mount(path)?;
    let (inode, _file_type) = with_fs(mount_id, |fs| fs.open(&relative, mode))?;

    Ok(FileHandle {
        id: NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst),
        mount_id,
        inode,
        position: 0,
        mode,
    })
}

/// Create a file
pub fn create_file(path: &str) -> IoResult<FileHandle> {
    let (mount_id, relative) = resolve_mount(path)?;
    let inode = with_fs(mount_id, |fs| fs.create_file(&relative))?;

    Ok(FileHandle {
        id: NEXT_HANDLE_ID.fetch_add(1, Ordering::SeqCst),
        mount_id,
        inode,
        position: 0,
        mode: AccessMode::ReadWrite,
    })
}

/// Read from an open file handle
pub fn read(handle: &mut FileHandle, buf: &mut [u8]) -> IoResult<usize> {
    let bytes = with_fs(handle.mount_id, |fs| {
        fs.read(handle.inode, handle.position, buf)
    })?;
    handle.position += bytes as u64;
    Ok(bytes)
}

/// Write to an open file handle
pub fn write(handle: &mut FileHandle, buf: &[u8]) -> IoResult<usize> {
    let bytes = with_fs(handle.mount_id, |fs| {
        fs.write(handle.inode, handle.position, buf)
    })?;
    handle.position += bytes as u64;
    Ok(bytes)
}

/// Close a file handle
pub fn close(handle: FileHandle) {
    let _ = with_fs(handle.mount_id, |fs| {
        fs.close(handle.inode);
        Ok(())
    });
}

/// Seek within a file
pub fn seek(handle: &mut FileHandle, whence: super::SeekFrom) -> IoResult<u64> {
    // Get file size if needed for SeekFrom::End
    let size = with_fs(handle.mount_id, |_fs| {
        // Try to get size via read of 0 bytes, or default
        Ok(0u64) // TODO: Add size() to Filesystem trait
    })?;
    
    let new_pos = match whence {
        super::SeekFrom::Start(offset) => offset,
        super::SeekFrom::Current(offset) => {
            if offset >= 0 {
                handle.position.saturating_add(offset as u64)
            } else {
                handle.position.saturating_sub((-offset) as u64)
            }
        }
        super::SeekFrom::End(offset) => {
            if offset >= 0 {
                size.saturating_add(offset as u64)
            } else {
                size.saturating_sub((-offset) as u64)
            }
        }
    };
    
    handle.position = new_pos;
    Ok(new_pos)
}

/// Create directory
pub fn mkdir(path: &str) -> IoResult<()> {
    let (mount_id, relative) = resolve_mount(path)?;
    with_fs(mount_id, |fs| fs.mkdir(&relative))
}

/// Read directory
pub fn readdir(path: &str) -> IoResult<Vec<DirEntry>> {
    let (mount_id, relative) = resolve_mount(path)?;
    with_fs(mount_id, |fs| fs.readdir(&relative))
}

/// Get file info
pub fn stat(path: &str) -> IoResult<FileInfo> {
    let (mount_id, relative) = resolve_mount(path)?;
    with_fs(mount_id, |fs| fs.stat(&relative))
}

/// Delete a file
pub fn unlink(path: &str) -> IoResult<()> {
    let (mount_id, relative) = resolve_mount(path)?;
    with_fs(mount_id, |fs| fs.unlink(&relative))
}
