//! Process Filesystem (/proc)
//!
//! Virtual filesystem exposing kernel state as readable files.
//! Supports subdirectories: /proc/net/*, /proc/sys/*
//!
//! Provides:
//!   /proc/cpuinfo      — CPU information
//!   /proc/meminfo      — Memory statistics
//!   /proc/uptime       — System uptime
//!   /proc/version      — Kernel version string
//!   /proc/mounts       — Mounted filesystems
//!   /proc/processes    — Process list
//!   /proc/interrupts   — Interrupt counters
//!   /proc/cmdline      — Kernel command line
//!   /proc/loadavg      — Load average
//!   /proc/filesystems  — Supported filesystem types
//!   /proc/stat         — Kernel statistics
//!   /proc/net/dev      — Network device statistics
//!   /proc/net/arp      — ARP table
//!   /proc/net/route    — Routing table
//!   /proc/net/tcp      — TCP connections
//!   /proc/sys/hostname — System hostname

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

use super::vfs::{self, Filesystem};
use super::{AccessMode, FileType, DirEntry, FileInfo, IoResult, IoError};

extern crate alloc;

/// All proc paths including subdirs. Paths are relative (no leading /).
/// Directories end with /
const PROC_PATHS: &[(&str, bool)] = &[
    // (path, is_directory)
    ("cpuinfo", false),
    ("meminfo", false),
    ("uptime", false),
    ("version", false),
    ("mounts", false),
    ("processes", false),
    ("interrupts", false),
    ("cmdline", false),
    ("loadavg", false),
    ("filesystems", false),
    ("stat", false),
    ("syslog", false),
    ("services", false),
    ("partitions", false),
    ("diskstats", false),
    ("net", true),       // directory
    ("net/dev", false),
    ("net/arp", false),
    ("net/route", false),
    ("net/tcp", false),
    ("sys", true),       // directory
    ("sys/hostname", false),
    ("sys/ostype", false),
    ("sys/kernel_version", false),
];

fn inode_for_path(path: &str) -> Option<u64> {
    // 0 = root dir, entries start at 1
    if path.is_empty() {
        return Some(0);
    }
    PROC_PATHS.iter().position(|(p, _)| *p == path).map(|i| (i + 1) as u64)
}

fn path_for_inode(inode: u64) -> Option<&'static str> {
    if inode == 0 { return Some(""); }
    let idx = (inode - 1) as usize;
    PROC_PATHS.get(idx).map(|(p, _)| *p)
}

fn is_directory(inode: u64) -> bool {
    if inode == 0 { return true; }
    let idx = (inode - 1) as usize;
    PROC_PATHS.get(idx).map(|(_, d)| *d).unwrap_or(false)
}

/// Generate content for a proc file dynamically
fn generate_content(path: &str) -> Option<Vec<u8>> {
    match path {
        "cpuinfo" => {
            let mut s = String::new();
            s.push_str("processor\t: 0\n");
            s.push_str("model name\t: ARM Cortex-A72\n");
            s.push_str("architecture\t: AArch64 (ARMv8-A)\n");
            s.push_str("features\t: fp asimd evtstrm aes pmull sha1 sha2 crc32\n");
            s.push_str("cpu implementer\t: 0x41 (ARM)\n");
            s.push_str("cpu part\t: 0xd08 (Cortex-A72)\n");
            s.push_str("cpu variant\t: 0x0\n");
            s.push_str("cpu revision\t: 3\n");
            s.push_str("cache size\t: 48 KB (L1i), 32 KB (L1d), 1 MB (L2)\n");
            s.push_str("bogomips\t: 125.00\n");
            Some(s.into_bytes())
        }
        "meminfo" => {
            let free = crate::mm::physical::free_memory() as u64;
            let total_kb = 128 * 1024u64;
            let free_kb = free / 1024;
            let used_kb = if total_kb > free_kb { total_kb - free_kb } else { 0 };
            let heap_kb = 4 * 1024u64;
            let mut s = String::new();
            s.push_str(&format!("MemTotal:     {:>8} kB\n", total_kb));
            s.push_str(&format!("MemFree:      {:>8} kB\n", free_kb));
            s.push_str(&format!("MemUsed:      {:>8} kB\n", used_kb));
            s.push_str(&format!("MemAvailable: {:>8} kB\n", free_kb));
            s.push_str(&format!("Buffers:      {:>8} kB\n", 0));
            s.push_str(&format!("Cached:       {:>8} kB\n", 0));
            s.push_str(&format!("SwapTotal:    {:>8} kB\n", 0));
            s.push_str(&format!("SwapFree:     {:>8} kB\n", 0));
            s.push_str(&format!("KernelHeap:   {:>8} kB\n", heap_kb));
            s.push_str(&format!("PageSize:     {:>8} kB\n", 4));
            Some(s.into_bytes())
        }
        "uptime" => {
            let ms = crate::hal::timer::uptime_ms();
            let secs = ms / 1000;
            let frac = (ms % 1000) / 10;
            Some(format!("{}.{:02} 0.00\n", secs, frac).into_bytes())
        }
        "version" => {
            Some(format!(
                "{} version {} ({}) (rustc nightly) #1 SMP PREEMPT {}\n",
                crate::KERNEL_NAME,
                crate::KERNEL_VERSION,
                crate::KERNEL_ARCH,
                "Sun Feb 09 00:00:00 UTC 2026"
            ).into_bytes())
        }
        "mounts" => {
            let mut s = String::new();
            s.push_str("ramfs / ramfs rw 0 0\n");
            s.push_str("devfs /dev devfs rw 0 0\n");
            s.push_str("procfs /proc procfs ro 0 0\n");
            Some(s.into_bytes())
        }
        "processes" => {
            let procs = crate::process::list_processes();
            let mut s = String::new();
            s.push_str("PID    STATE      PRI  THR  NAME\n");
            for p in &procs {
                s.push_str(&format!("{:<6} {:<10} {:<4} {:<4} {}\n",
                    p.pid, p.state.as_str(), p.priority, p.threads, p.name));
            }
            s.push_str(&format!("\nTotal: {} processes, {} threads\n",
                crate::process::process_count(),
                crate::process::thread_count()));
            Some(s.into_bytes())
        }
        "interrupts" => {
            let ms = crate::hal::timer::uptime_ms();
            let timer_irqs = ms / 10;
            let mut s = String::new();
            s.push_str("           CPU0\n");
            s.push_str(&format!(" 30:  {:>8}   GICv3  ARM Generic Timer\n", timer_irqs));
            s.push_str(&format!(" 33:  {:>8}   GICv3  PL011 UART (serial)\n", 0));
            s.push_str(&format!("IPI:  {:>8}   Inter-processor interrupts\n", 0));
            s.push_str(&format!("ERR:  {:>8}\n", 0));
            Some(s.into_bytes())
        }
        "cmdline" => {
            Some(b"cantaya_kernel console=ttyAMA0\n".to_vec())
        }
        "loadavg" => {
            Some(b"0.00 0.00 0.00 1/1 1\n".to_vec())
        }
        "filesystems" => {
            let mut s = String::new();
            s.push_str("\tramfs\n");
            s.push_str("\tdevfs\n");
            s.push_str("nodev\tprocfs\n");
            Some(s.into_bytes())
        }
        "stat" => {
            let ms = crate::hal::timer::uptime_ms();
            let jiffies = ms / 10;
            let mut s = String::new();
            s.push_str(&format!("cpu  {} 0 {} 0 0 0 0 0 0 0\n", jiffies / 2, jiffies / 2));
            s.push_str(&format!("cpu0 {} 0 {} 0 0 0 0 0 0 0\n", jiffies / 2, jiffies / 2));
            s.push_str(&format!("btime {}\n", 0));
            s.push_str(&format!("processes {}\n", crate::process::process_count()));
            s.push_str("procs_running 1\n");
            s.push_str("procs_blocked 0\n");
            Some(s.into_bytes())
        }
        "syslog" => {
            Some(crate::hal::syslog::generate_syslog_content().into_bytes())
        }
        // /proc/net/*
        "net/dev" => {
            Some(crate::drivers::net::proc_net_dev().into_bytes())
        }
        "net/arp" => {
            Some(crate::drivers::net::proc_net_arp().into_bytes())
        }
        "net/route" => {
            Some(crate::drivers::net::proc_net_route().into_bytes())
        }
        "net/tcp" => {
            let conns = crate::drivers::net::get_connections();
            let mut s = String::new();
            s.push_str("Proto  Local Address          Foreign Address        State\n");
            for c in &conns {
                s.push_str(&format!("{:<7}{:<23}{:<23}{}\n",
                    c.proto,
                    format!("{}:{}", crate::drivers::net::format_ip(&c.local_ip), c.local_port),
                    format!("{}:{}", crate::drivers::net::format_ip(&c.remote_ip), c.remote_port),
                    c.state));
            }
            Some(s.into_bytes())
        }
        // /proc/sys/*
        "sys/hostname" => {
            Some(b"cantaya\n".to_vec())
        }
        "sys/ostype" => {
            Some(b"CantayaOS\n".to_vec())
        }
        "sys/kernel_version" => {
            Some(format!("{}\n", crate::KERNEL_VERSION).into_bytes())
        }
        "services" => {
            Some(crate::hal::services::generate_services_content().into_bytes())
        }
        "partitions" => {
            let mut s = String::new();
            s.push_str("major minor  #blocks  name\n\n");
            s.push_str("   8     0   131072  sda\n");
            s.push_str("   8     1   122880  sda1\n");
            Some(s.into_bytes())
        }
        "diskstats" => {
            let ms = crate::hal::timer::uptime_ms();
            let reads = ms / 500;
            let writes = ms / 800;
            let mut s = String::new();
            s.push_str(&format!("   8    0 sda {} 0 {} 0 {} 0 {} 0 0 0 0\n", reads, reads * 8, writes, writes * 4));
            s.push_str(&format!("   8    1 sda1 {} 0 {} 0 {} 0 {} 0 0 0 0\n", reads, reads * 8, writes, writes * 4));
            Some(s.into_bytes())
        }
        _ => None,
    }
}

struct ProcFsDriver;

impl Filesystem for ProcFsDriver {
    fn name(&self) -> &str { "procfs" }

    fn open(&self, path: &str, _mode: AccessMode) -> IoResult<(u64, FileType)> {
        let name = path.trim_start_matches('/');
        if name.is_empty() {
            return Ok((0, FileType::Directory));
        }
        match inode_for_path(name) {
            Some(inode) => {
                let ftype = if is_directory(inode) { FileType::Directory } else { FileType::Regular };
                Ok((inode, ftype))
            }
            None => Err(IoError::NotFound),
        }
    }

    fn create_file(&self, _path: &str) -> IoResult<u64> {
        Err(IoError::PermissionDenied)
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        if is_directory(inode) {
            return Err(IoError::IsADirectory);
        }
        let path = path_for_inode(inode).ok_or(IoError::NotFound)?;
        let content = generate_content(path).ok_or(IoError::NotFound)?;
        let offset = offset as usize;
        if offset >= content.len() {
            return Ok(0);
        }
        let available = content.len() - offset;
        let to_read = core::cmp::min(buf.len(), available);
        buf[..to_read].copy_from_slice(&content[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, _inode: u64, _offset: u64, _buf: &[u8]) -> IoResult<usize> {
        Err(IoError::PermissionDenied)
    }

    fn close(&self, _inode: u64) {}

    fn mkdir(&self, _path: &str) -> IoResult<()> {
        Err(IoError::PermissionDenied)
    }

    fn readdir(&self, path: &str) -> IoResult<Vec<DirEntry>> {
        let name = path.trim_start_matches('/');
        // Check this is a valid directory
        if !name.is_empty() {
            let inode = inode_for_path(name).ok_or(IoError::NotFound)?;
            if !is_directory(inode) {
                return Err(IoError::NotADirectory);
            }
        }

        let prefix = if name.is_empty() { "" } else { name };
        let mut entries = Vec::new();

        for &(entry_path, is_dir) in PROC_PATHS {
            // Entry must be a direct child of the requested directory
            let relative = if prefix.is_empty() {
                entry_path
            } else if let Some(rel) = entry_path.strip_prefix(prefix) {
                let rel = rel.trim_start_matches('/');
                if rel.is_empty() { continue; }
                rel
            } else {
                continue;
            };

            // Only show direct children (no nested slashes)
            if relative.contains('/') {
                continue;
            }

            let ftype = if is_dir { FileType::Directory } else { FileType::Regular };
            let size = if is_dir { 0 } else {
                generate_content(entry_path).map(|c| c.len() as u64).unwrap_or(0)
            };
            entries.push(DirEntry {
                name: String::from(relative),
                file_type: ftype,
                size,
            });
        }
        Ok(entries)
    }

    fn stat(&self, path: &str) -> IoResult<FileInfo> {
        let name = path.trim_start_matches('/');
        if name.is_empty() {
            return Ok(FileInfo {
                file_type: FileType::Directory,
                size: 0,
                created: 0,
                modified: crate::hal::timer::uptime_ms(),
                accessed: crate::hal::timer::uptime_ms(),
            });
        }
        let inode = inode_for_path(name).ok_or(IoError::NotFound)?;
        let ftype = if is_directory(inode) { FileType::Directory } else { FileType::Regular };
        let size = if is_directory(inode) { 0 } else {
            generate_content(name).map(|c| c.len() as u64).unwrap_or(0)
        };
        Ok(FileInfo {
            file_type: ftype,
            size,
            created: 0,
            modified: crate::hal::timer::uptime_ms(),
            accessed: crate::hal::timer::uptime_ms(),
        })
    }

    fn unlink(&self, _path: &str) -> IoResult<()> {
        Err(IoError::PermissionDenied)
    }
}

/// Initialize procfs and mount at /proc
pub fn init() {
    let _ = crate::fs::mkdir("/proc");
    vfs::mount("/proc", Box::new(ProcFsDriver));
}
