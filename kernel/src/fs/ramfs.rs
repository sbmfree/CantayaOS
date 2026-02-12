//! RAM Filesystem
//!
//! In-memory filesystem mounted at "/" providing basic file and
//! directory storage. Used as the root filesystem during boot.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;
use core::sync::atomic::{AtomicU64, Ordering};

use super::vfs::{self, Filesystem};
use super::{AccessMode, FileType, DirEntry, FileInfo, IoResult, IoError};

extern crate alloc;

/// Maximum file size (1MB per file for RAM fs)
const MAX_FILE_SIZE: usize = 1024 * 1024;

/// Inode data
enum InodeData {
    File {
        data: Vec<u8>,
    },
    Directory {
        children: BTreeMap<String, u64>, // name → inode
    },
}

/// Inode
struct Inode {
    #[allow(dead_code)]
    id: u64,
    file_type: FileType,
    data: InodeData,
    created: u64,
    modified: u64,
}

/// RAM filesystem state
struct RamFs {
    inodes: BTreeMap<u64, Inode>,
    next_inode: AtomicU64,
}

impl RamFs {
    fn new() -> Self {
        let mut fs = RamFs {
            inodes: BTreeMap::new(),
            next_inode: AtomicU64::new(2),
        };

        // Create root directory (inode 1)
        fs.inodes.insert(1, Inode {
            id: 1,
            file_type: FileType::Directory,
            data: InodeData::Directory {
                children: BTreeMap::new(),
            },
            created: 0,
            modified: 0,
        });

        fs
    }

    fn alloc_inode(&self) -> u64 {
        self.next_inode.fetch_add(1, Ordering::SeqCst)
    }

    /// Resolve a path to an inode ID
    fn resolve_path(&self, path: &str) -> IoResult<u64> {
        if path == "/" || path.is_empty() {
            return Ok(1); // root
        }

        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        let mut current_inode: u64 = 1; // root

        for component in components {
            let inode = self.inodes.get(&current_inode).ok_or(IoError::NotFound)?;
            match &inode.data {
                InodeData::Directory { children } => {
                    current_inode = *children.get(component).ok_or(IoError::NotFound)?;
                }
                _ => return Err(IoError::NotADirectory),
            }
        }

        Ok(current_inode)
    }

    /// Resolve parent directory inode and child name
    fn resolve_parent(&self, path: &str) -> IoResult<(u64, String)> {
        let path = path.trim_start_matches('/');
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if components.is_empty() {
            return Err(IoError::InvalidPath);
        }

        let name = String::from(*components.last().unwrap());
        let parent_path = if components.len() == 1 {
            "/".into()
        } else {
            let parent_parts: Vec<&str> = components[..components.len() - 1].to_vec();
            let mut p = String::from("/");
            p.push_str(&parent_parts.join("/"));
            p
        };

        let parent_inode = self.resolve_path(&parent_path)?;
        Ok((parent_inode, name))
    }
}

static RAMFS: Mutex<RamFs> = Mutex::new(RamFs {
    inodes: BTreeMap::new(),
    next_inode: AtomicU64::new(2),
});

/// Wrapper that lets us implement Filesystem trait
struct RamFsDriver;

impl Filesystem for RamFsDriver {
    fn name(&self) -> &str {
        "ramfs"
    }

    fn open(&self, path: &str, _mode: AccessMode) -> IoResult<(u64, FileType)> {
        let fs = RAMFS.lock();
        let inode_id = fs.resolve_path(path)?;
        let inode = fs.inodes.get(&inode_id).ok_or(IoError::NotFound)?;
        Ok((inode_id, inode.file_type))
    }

    fn create_file(&self, path: &str) -> IoResult<u64> {
        let mut fs = RAMFS.lock();

        // Check if already exists
        if fs.resolve_path(path).is_ok() {
            return Err(IoError::AlreadyExists);
        }

        let (parent_inode_id, name) = fs.resolve_parent(path)?;
        let new_id = fs.alloc_inode();

        // Create file inode
        fs.inodes.insert(new_id, Inode {
            id: new_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: Vec::new() },
            created: crate::hal::timer::uptime_ms(),
            modified: crate::hal::timer::uptime_ms(),
        });

        // Add to parent directory
        if let Some(parent) = fs.inodes.get_mut(&parent_inode_id) {
            if let InodeData::Directory { children } = &mut parent.data {
                children.insert(name, new_id);
            } else {
                return Err(IoError::NotADirectory);
            }
        }

        Ok(new_id)
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        let fs = RAMFS.lock();
        let node = fs.inodes.get(&inode).ok_or(IoError::NotFound)?;

        match &node.data {
            InodeData::File { data } => {
                let offset = offset as usize;
                if offset >= data.len() {
                    return Ok(0); // EOF
                }
                let available = data.len() - offset;
                let to_read = core::cmp::min(buf.len(), available);
                buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
                Ok(to_read)
            }
            InodeData::Directory { .. } => Err(IoError::IsADirectory),
        }
    }

    fn write(&self, inode: u64, offset: u64, buf: &[u8]) -> IoResult<usize> {
        let mut fs = RAMFS.lock();
        let node = fs.inodes.get_mut(&inode).ok_or(IoError::NotFound)?;

        match &mut node.data {
            InodeData::File { data } => {
                let offset = offset as usize;

                // Extend file if needed
                if offset + buf.len() > data.len() {
                    if offset + buf.len() > MAX_FILE_SIZE {
                        return Err(IoError::NoSpace);
                    }
                    data.resize(offset + buf.len(), 0);
                }

                data[offset..offset + buf.len()].copy_from_slice(buf);
                node.modified = crate::hal::timer::uptime_ms();
                Ok(buf.len())
            }
            InodeData::Directory { .. } => Err(IoError::IsADirectory),
        }
    }

    fn close(&self, _inode: u64) {
        // Nothing to do for RAM files
    }

    fn mkdir(&self, path: &str) -> IoResult<()> {
        let mut fs = RAMFS.lock();

        if fs.resolve_path(path).is_ok() {
            return Err(IoError::AlreadyExists);
        }

        let (parent_inode_id, name) = fs.resolve_parent(path)?;
        let new_id = fs.alloc_inode();

        fs.inodes.insert(new_id, Inode {
            id: new_id,
            file_type: FileType::Directory,
            data: InodeData::Directory {
                children: BTreeMap::new(),
            },
            created: crate::hal::timer::uptime_ms(),
            modified: crate::hal::timer::uptime_ms(),
        });

        if let Some(parent) = fs.inodes.get_mut(&parent_inode_id) {
            if let InodeData::Directory { children } = &mut parent.data {
                children.insert(name, new_id);
            } else {
                return Err(IoError::NotADirectory);
            }
        }

        Ok(())
    }

    fn readdir(&self, path: &str) -> IoResult<Vec<DirEntry>> {
        let fs = RAMFS.lock();
        let inode_id = fs.resolve_path(path)?;
        let inode = fs.inodes.get(&inode_id).ok_or(IoError::NotFound)?;

        match &inode.data {
            InodeData::Directory { children } => {
                let mut entries = Vec::new();
                for (name, child_id) in children.iter() {
                    if let Some(child) = fs.inodes.get(child_id) {
                        let size = match &child.data {
                            InodeData::File { data } => data.len() as u64,
                            InodeData::Directory { children } => children.len() as u64,
                        };
                        entries.push(DirEntry {
                            name: name.clone(),
                            file_type: child.file_type,
                            size,
                        });
                    }
                }
                Ok(entries)
            }
            _ => Err(IoError::NotADirectory),
        }
    }

    fn stat(&self, path: &str) -> IoResult<FileInfo> {
        let fs = RAMFS.lock();
        let inode_id = fs.resolve_path(path)?;
        let inode = fs.inodes.get(&inode_id).ok_or(IoError::NotFound)?;

        let size = match &inode.data {
            InodeData::File { data } => data.len() as u64,
            InodeData::Directory { children } => children.len() as u64,
        };

        Ok(FileInfo {
            file_type: inode.file_type,
            size,
            created: inode.created,
            modified: inode.modified,
            accessed: inode.modified,
        })
    }

    fn unlink(&self, path: &str) -> IoResult<()> {
        let mut fs = RAMFS.lock();

        let inode_id = fs.resolve_path(path)?;

        // Check it's not a non-empty directory
        if let Some(inode) = fs.inodes.get(&inode_id) {
            match &inode.data {
                InodeData::Directory { children } if !children.is_empty() => {
                    return Err(IoError::NotEmpty);
                }
                _ => {}
            }
        }

        // Remove from parent
        let (parent_inode_id, name) = fs.resolve_parent(path)?;
        if let Some(parent) = fs.inodes.get_mut(&parent_inode_id) {
            if let InodeData::Directory { children } = &mut parent.data {
                children.remove(&name);
            }
        }

        // Remove inode
        fs.inodes.remove(&inode_id);
        Ok(())
    }
}

/// Initialize RAM filesystem and mount at "/"
pub fn init() {
    // Initialize the global RAMFS state
    {
        let mut fs = RAMFS.lock();
        *fs = RamFs::new();

        // Create initial directory structure
        // We need to release the lock before calling mount, so we set up inodes here
        let etc_id = fs.alloc_inode();
        fs.inodes.insert(etc_id, Inode {
            id: etc_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });

        let tmp_id = fs.alloc_inode();
        fs.inodes.insert(tmp_id, Inode {
            id: tmp_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });

        let dev_id = fs.alloc_inode();
        fs.inodes.insert(dev_id, Inode {
            id: dev_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });

        // Add to root
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("etc"), etc_id);
                children.insert(String::from("tmp"), tmp_id);
                children.insert(String::from("dev"), dev_id);
            }
        }
        
        // Create /etc/motd (message of the day)
        let motd_id = fs.alloc_inode();
        let motd_text = b"Welcome to CantayaOS!\nA hybrid kernel for ARM64.\nType 'help' to get started.\n";
        fs.inodes.insert(motd_id, Inode {
            id: motd_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: motd_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("motd"), motd_id);
            }
        }
        
        // Create /etc/hostname
        let hostname_id = fs.alloc_inode();
        fs.inodes.insert(hostname_id, Inode {
            id: hostname_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: b"cantaya\n".to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("hostname"), hostname_id);
            }
        }
        
        // Create /etc/version
        let ver_id = fs.alloc_inode();
        fs.inodes.insert(ver_id, Inode {
            id: ver_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: b"CantayaOS v0.1.0\n".to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("version"), ver_id);
            }
        }

        // Create /home directory
        let home_id = fs.alloc_inode();
        fs.inodes.insert(home_id, Inode {
            id: home_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("home"), home_id);
            }
        }

        // Create /var directory
        let var_id = fs.alloc_inode();
        fs.inodes.insert(var_id, Inode {
            id: var_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("var"), var_id);
            }
        }

        // Create /var/log directory
        let log_id = fs.alloc_inode();
        fs.inodes.insert(log_id, Inode {
            id: log_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(var) = fs.inodes.get_mut(&var_id) {
            if let InodeData::Directory { children } = &mut var.data {
                children.insert(String::from("log"), log_id);
            }
        }

        // Create /bin directory
        let bin_id = fs.alloc_inode();
        fs.inodes.insert(bin_id, Inode {
            id: bin_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("bin"), bin_id);
            }
        }

        // ---- Embed real cross-compiled userspace ELF binaries ----
        // These are built by `build.sh` (userspace step) before the kernel.

        // /bin/init — the init process ELF
        let init_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/cantaya_init");
        let init_id = fs.alloc_inode();
        fs.inodes.insert(init_id, Inode {
            id: init_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: init_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("init"), init_id);
            }
        }

        // /bin/hello — simple hello-world program
        let hello_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/hello");
        let hello_id = fs.alloc_inode();
        fs.inodes.insert(hello_id, Inode {
            id: hello_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: hello_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("hello"), hello_id);
            }
        }

        // /bin/shell_hello — syscall demo program
        let shell_hello_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/shell_hello");
        let shell_hello_id = fs.alloc_inode();
        fs.inodes.insert(shell_hello_id, Inode {
            id: shell_hello_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: shell_hello_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("shell_hello"), shell_hello_id);
            }
        }

        // /bin/echo — prints its arguments
        let echo_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/echo");
        let echo_id = fs.alloc_inode();
        fs.inodes.insert(echo_id, Inode {
            id: echo_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: echo_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("echo"), echo_id);
            }
        }

        // /bin/cat — reads and displays file contents
        let cat_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/cat");
        let cat_id = fs.alloc_inode();
        fs.inodes.insert(cat_id, Inode {
            id: cat_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: cat_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("cat"), cat_id);
            }
        }

        // /bin/draw — pixel buffer GUI demo
        let draw_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/draw");
        let draw_id = fs.alloc_inode();
        fs.inodes.insert(draw_id, Inode {
            id: draw_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: draw_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("draw"), draw_id);
            }
        }

        // /bin/http_get — simple TCP HTTP GET client
        let http_get_elf: &[u8] = include_bytes!("../../../userspace/target/aarch64-unknown-none/release/http_get");
        let http_get_id = fs.alloc_inode();
        fs.inodes.insert(http_get_id, Inode {
            id: http_get_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: http_get_elf.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(bin) = fs.inodes.get_mut(&bin_id) {
            if let InodeData::Directory { children } = &mut bin.data {
                children.insert(String::from("http_get"), http_get_id);
            }
        }

        // Create /sbin directory
        let sbin_id = fs.alloc_inode();
        fs.inodes.insert(sbin_id, Inode {
            id: sbin_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("sbin"), sbin_id);
            }
        }

        // Create /etc/profile (shell startup script)
        let profile_id = fs.alloc_inode();
        let profile_text = b"# /etc/profile - System-wide shell initialization\n# CantayaOS default profile\n\n# Set environment\nset EDITOR=vi\nset PAGER=cat\nset PS1=cantaya\n\n# Default aliases\nalias ll=ls -l\nalias la=ls -a\nalias cls=clear\nalias ..=cd ..\nalias q=halt\n";
        fs.inodes.insert(profile_id, Inode {
            id: profile_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: profile_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("profile"), profile_id);
            }
        }

        // Create /etc/issue (login banner)
        let issue_id = fs.alloc_inode();
        let issue_text = b"CantayaOS v0.1.0 (AArch64)\nKernel \\r on \\m\n\n";
        fs.inodes.insert(issue_id, Inode {
            id: issue_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: issue_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("issue"), issue_id);
            }
        }

        // Create /etc/passwd (user database)
        let passwd_id = fs.alloc_inode();
        let passwd_text = b"root:x:0:0:System Administrator:/root:/bin/csh\ndaemon:x:1:1:System Daemon:/usr/sbin:/bin/false\nnobody:x:65534:65534:Nobody:/nonexistent:/bin/false\n";
        fs.inodes.insert(passwd_id, Inode {
            id: passwd_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: passwd_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("passwd"), passwd_id);
            }
        }

        // Create /etc/group (group database)
        let group_id = fs.alloc_inode();
        let group_text = b"root:x:0:root\nwheel:x:10:root\ndaemon:x:1:\nnogroup:x:65534:\n";
        fs.inodes.insert(group_id, Inode {
            id: group_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: group_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("group"), group_id);
            }
        }

        // Create /etc/resolv.conf (DNS config)
        let resolv_id = fs.alloc_inode();
        let resolv_text = b"# DNS resolver configuration\nnameserver 10.0.2.3\nsearch cantaya.local\n";
        fs.inodes.insert(resolv_id, Inode {
            id: resolv_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: resolv_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("resolv.conf"), resolv_id);
            }
        }

        // Create /etc/services (well-known ports)
        let services_id = fs.alloc_inode();
        let services_text = b"ssh       22/tcp     # Secure Shell\ntelnet    23/tcp     # Telnet\nhttp      80/tcp     # HTTP\nhttps     443/tcp    # HTTPS\ndns       53/udp     # Domain Name System\ndhcp      67/udp     # DHCP Server\nntp       123/udp    # Network Time Protocol\n";
        fs.inodes.insert(services_id, Inode {
            id: services_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: services_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(etc) = fs.inodes.get_mut(&etc_id) {
            if let InodeData::Directory { children } = &mut etc.data {
                children.insert(String::from("services"), services_id);
            }
        }

        // Create /root directory
        let root_home_id = fs.alloc_inode();
        fs.inodes.insert(root_home_id, Inode {
            id: root_home_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("root"), root_home_id);
            }
        }

        // Create /root/.cshrc
        let cshrc_id = fs.alloc_inode();
        let cshrc_text = b"# CantayaOS Shell Run Commands\n# This file is sourced on shell startup\nalias l=ls\nalias h=history\n";
        fs.inodes.insert(cshrc_id, Inode {
            id: cshrc_id,
            file_type: FileType::Regular,
            data: InodeData::File { data: cshrc_text.to_vec() },
            created: 0,
            modified: 0,
        });
        if let Some(root_home) = fs.inodes.get_mut(&root_home_id) {
            if let InodeData::Directory { children } = &mut root_home.data {
                children.insert(String::from(".cshrc"), cshrc_id);
            }
        }

        // Create /usr directory tree
        let usr_id = fs.alloc_inode();
        fs.inodes.insert(usr_id, Inode {
            id: usr_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(root) = fs.inodes.get_mut(&1) {
            if let InodeData::Directory { children } = &mut root.data {
                children.insert(String::from("usr"), usr_id);
            }
        }

        // Create /usr/share
        let share_id = fs.alloc_inode();
        fs.inodes.insert(share_id, Inode {
            id: share_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(usr) = fs.inodes.get_mut(&usr_id) {
            if let InodeData::Directory { children } = &mut usr.data {
                children.insert(String::from("share"), share_id);
            }
        }

        // Create /usr/share/man directory
        let man_id = fs.alloc_inode();
        fs.inodes.insert(man_id, Inode {
            id: man_id,
            file_type: FileType::Directory,
            data: InodeData::Directory { children: BTreeMap::new() },
            created: 0,
            modified: 0,
        });
        if let Some(share) = fs.inodes.get_mut(&share_id) {
            if let InodeData::Directory { children } = &mut share.data {
                children.insert(String::from("man"), man_id);
            }
        }
    }

    // Mount ramfs at root
    vfs::mount("/", Box::new(RamFsDriver));
}
