//! Filesystem Subsystem
//!
//! Windows NT-like I/O manager with VFS layer supporting
//! multiple filesystem drivers under a unified namespace.

pub mod vfs;
pub mod ramfs;
pub mod devfs;
pub mod procfs;
pub mod elf;
pub mod fat32;

use alloc::string::String;
use spin::Mutex;

extern crate alloc;

/// File access modes
#[derive(Clone, Copy, Debug)]
pub enum AccessMode {
    Read,
    Write,
    ReadWrite,
}

/// File seek origin
#[derive(Clone, Copy, Debug)]
pub enum SeekFrom {
    Start(u64),
    Current(i64),
    End(i64),
}

/// File type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FileType {
    Regular,
    Directory,
    Device,
    Pipe,
    Symlink,
}

/// Directory entry
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub file_type: FileType,
    pub size: u64,
}

/// File metadata
#[derive(Clone, Debug)]
pub struct FileInfo {
    pub file_type: FileType,
    pub size: u64,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
}

/// I/O Status result
pub type IoResult<T> = Result<T, IoError>;

/// I/O Error codes (NTSTATUS-like)
#[derive(Clone, Copy, Debug)]
pub enum IoError {
    NotFound,
    AlreadyExists,
    PermissionDenied,
    NotADirectory,
    IsADirectory,
    NotEmpty,
    InvalidPath,
    NoSpace,
    InvalidHandle,
    EndOfFile,
    NotImplemented,
    DeviceError,
}

static INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Initialize filesystem subsystem
pub fn init() {
    vfs::init();
    ramfs::init();
    devfs::init();
    procfs::init();
    fat32::init();
    *INITIALIZED.lock() = true;
}

/// Open a file by path (NtCreateFile equivalent)
pub fn open(path: &str, mode: AccessMode) -> IoResult<vfs::FileHandle> {
    vfs::open(path, mode)
}

/// Create a file
pub fn create(path: &str) -> IoResult<vfs::FileHandle> {
    vfs::create_file(path)
}

/// Read from a file handle
pub fn read(handle: &mut vfs::FileHandle, buf: &mut [u8]) -> IoResult<usize> {
    vfs::read(handle, buf)
}

/// Write to a file handle
pub fn write(handle: &mut vfs::FileHandle, buf: &[u8]) -> IoResult<usize> {
    vfs::write(handle, buf)
}

/// Seek within a file
pub fn seek(handle: &mut vfs::FileHandle, whence: SeekFrom) -> IoResult<u64> {
    vfs::seek(handle, whence)
}

/// Close a file handle
pub fn close(handle: vfs::FileHandle) {
    vfs::close(handle);
}

/// Create a directory
pub fn mkdir(path: &str) -> IoResult<()> {
    vfs::mkdir(path)
}

/// List directory contents
pub fn readdir(path: &str) -> IoResult<alloc::vec::Vec<DirEntry>> {
    vfs::readdir(path)
}

/// Delete a file
pub fn delete(path: &str) -> IoResult<()> {
    vfs::unlink(path)
}
