//! Device Filesystem
//!
//! Exposes kernel devices as files under /dev
//! (e.g., /dev/null, /dev/console, /dev/zero)

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

use super::vfs::{self, Filesystem};
use super::{AccessMode, FileType, DirEntry, FileInfo, IoResult, IoError};

extern crate alloc;

/// Device file operations
pub trait DeviceOps: Send + Sync {
    fn read(&self, offset: u64, buf: &mut [u8]) -> IoResult<usize>;
    fn write(&self, offset: u64, buf: &[u8]) -> IoResult<usize>;
}

/// Null device — discards writes, reads return EOF
struct NullDevice;

impl DeviceOps for NullDevice {
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> IoResult<usize> {
        Ok(0) // EOF
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len()) // discard
    }
}

/// Zero device — reads return zeroes
struct ZeroDevice;

impl DeviceOps for ZeroDevice {
    fn read(&self, _offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        for b in buf.iter_mut() {
            *b = 0;
        }
        Ok(buf.len())
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len())
    }
}

/// Console device — reads/writes go to UART
struct ConsoleDevice;

impl DeviceOps for ConsoleDevice {
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> IoResult<usize> {
        Ok(0)
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        for &b in buf {
            if b == b'\n' {
                crate::kprint!("\n");
            } else {
                crate::kprint!("{}", b as char);
            }
        }
        Ok(buf.len())
    }
}

/// Random device — reads return pseudo-random bytes
struct RandomDevice;

impl DeviceOps for RandomDevice {
    fn read(&self, _offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        // Simple PRNG based on timer + LCG
        let mut seed = crate::hal::timer::uptime_ms() as u64;
        for b in buf.iter_mut() {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            *b = (seed >> 33) as u8;
        }
        Ok(buf.len())
    }

    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len()) // seed mixing (no-op)
    }
}

/// TTY device — alias for console
struct TtyDevice;

impl DeviceOps for TtyDevice {
    fn read(&self, _offset: u64, _buf: &mut [u8]) -> IoResult<usize> {
        Ok(0)
    }
    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        for &b in buf { crate::kprint!("{}", b as char); }
        Ok(buf.len())
    }
}

/// Block device stub (sda)
struct BlockDevice {
    size_bytes: u64,
}

impl DeviceOps for BlockDevice {
    fn read(&self, offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        if offset >= self.size_bytes { return Ok(0); }
        let avail = core::cmp::min(buf.len() as u64, self.size_bytes - offset) as usize;
        for b in buf[..avail].iter_mut() { *b = 0; }
        Ok(avail)
    }
    fn write(&self, _offset: u64, buf: &[u8]) -> IoResult<usize> {
        Ok(buf.len())
    }
}

/// Device entry
struct DevEntry {
    #[allow(dead_code)]
    inode: u64,
    name: String,
    ops: Box<dyn DeviceOps>,
}

/// DevFS state  
struct DevFs {
    devices: BTreeMap<u64, DevEntry>,
    name_to_inode: BTreeMap<String, u64>,
    next_inode: u64,
}

impl DevFs {
    fn new() -> Self {
        DevFs {
            devices: BTreeMap::new(),
            name_to_inode: BTreeMap::new(),
            next_inode: 1000,
        }
    }

    fn register_device(&mut self, name: &str, ops: Box<dyn DeviceOps>) {
        let inode = self.next_inode;
        self.next_inode += 1;

        self.name_to_inode.insert(String::from(name), inode);
        self.devices.insert(inode, DevEntry {
            inode,
            name: String::from(name),
            ops,
        });
    }
}

static DEVFS: Mutex<DevFs> = Mutex::new(DevFs {
    devices: BTreeMap::new(),
    name_to_inode: BTreeMap::new(),
    next_inode: 1000,
});

/// DevFS filesystem driver
struct DevFsDriver;

impl Filesystem for DevFsDriver {
    fn name(&self) -> &str {
        "devfs"
    }

    fn open(&self, path: &str, _mode: AccessMode) -> IoResult<(u64, FileType)> {
        let name = path.trim_start_matches('/');
        let fs = DEVFS.lock();
        let inode = fs.name_to_inode.get(name).ok_or(IoError::NotFound)?;
        Ok((*inode, FileType::Device))
    }

    fn create_file(&self, _path: &str) -> IoResult<u64> {
        Err(IoError::PermissionDenied) // can't create files in devfs
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        let fs = DEVFS.lock();
        let dev = fs.devices.get(&inode).ok_or(IoError::NotFound)?;
        dev.ops.read(offset, buf)
    }

    fn write(&self, inode: u64, offset: u64, buf: &[u8]) -> IoResult<usize> {
        let fs = DEVFS.lock();
        let dev = fs.devices.get(&inode).ok_or(IoError::NotFound)?;
        dev.ops.write(offset, buf)
    }

    fn close(&self, _inode: u64) {}

    fn mkdir(&self, _path: &str) -> IoResult<()> {
        Err(IoError::PermissionDenied)
    }

    fn readdir(&self, path: &str) -> IoResult<Vec<DirEntry>> {
        let name = path.trim_start_matches('/');
        if !name.is_empty() {
            return Err(IoError::NotADirectory);
        }

        let fs = DEVFS.lock();
        let mut entries = Vec::new();
        for dev in fs.devices.values() {
            entries.push(DirEntry {
                name: dev.name.clone(),
                file_type: FileType::Device,
                size: 0,
            });
        }
        Ok(entries)
    }

    fn stat(&self, path: &str) -> IoResult<FileInfo> {
        let name = path.trim_start_matches('/');
        let fs = DEVFS.lock();
        let _inode = fs.name_to_inode.get(name).ok_or(IoError::NotFound)?;
        Ok(FileInfo {
            file_type: FileType::Device,
            size: 0,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }

    fn unlink(&self, _path: &str) -> IoResult<()> {
        Err(IoError::PermissionDenied)
    }
}

/// Initialize device filesystem
pub fn init() {
    // Register built-in devices
    {
        let mut fs = DEVFS.lock();
        *fs = DevFs::new();
        fs.register_device("null", Box::new(NullDevice));
        fs.register_device("zero", Box::new(ZeroDevice));
        fs.register_device("console", Box::new(ConsoleDevice));
        fs.register_device("random", Box::new(RandomDevice));
        fs.register_device("urandom", Box::new(RandomDevice));
        fs.register_device("tty", Box::new(TtyDevice));
        fs.register_device("tty0", Box::new(TtyDevice));
        fs.register_device("sda", Box::new(BlockDevice { size_bytes: 128 * 1024 * 1024 }));
        fs.register_device("sda1", Box::new(BlockDevice { size_bytes: 120 * 1024 * 1024 }));
    }

    // Mount at /dev
    vfs::mount("/dev", Box::new(DevFsDriver));
}

/// Register a new device at runtime
pub fn register_device(name: &str, ops: Box<dyn DeviceOps>) {
    DEVFS.lock().register_device(name, ops);
}
