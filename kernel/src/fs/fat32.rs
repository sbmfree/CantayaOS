//! FAT32 Filesystem
//!
//! Implements the VFS `Filesystem` trait on top of a virtio-blk device.
//! Supports directories, file read/write, create, delete, mkdir, and LFN.

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

use super::vfs::{self, Filesystem};
use super::{AccessMode, FileType, DirEntry, FileInfo, IoResult, IoError};

/// Access the virtio-blk driver through a helper to avoid cross-module issues.
/// We call it indirectly to avoid circular dependency with `drivers`.
fn blk_read_sector(sector: u64, buf: &mut [u8; SECTOR_SIZE]) -> bool {
    crate::drivers::virtio_blk::read_sector(sector, buf)
}
fn blk_write_sector(sector: u64, data: &[u8; SECTOR_SIZE]) -> bool {
    crate::drivers::virtio_blk::write_sector(sector, data)
}
fn blk_is_available() -> bool {
    crate::drivers::virtio_blk::is_available()
}

const SECTOR_SIZE: usize = 512;

// ---------------------------------------------------------------------------
// FAT32 on-disk structures
// ---------------------------------------------------------------------------

/// BIOS Parameter Block — parsed from sector 0
#[allow(dead_code)]
struct Bpb {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    total_sectors_32: u32,
    fat_size_32: u32,
    root_cluster: u32,
}

/// 32-byte short directory entry
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct DirEntry32 {
    name: [u8; 11],
    attr: u8,
    _nt_res: u8,
    _crt_time_tenth: u8,
    _crt_time: u16,
    _crt_date: u16,
    _acc_date: u16,
    cluster_hi: u16,
    _wrt_time: u16,
    _wrt_date: u16,
    cluster_lo: u16,
    file_size: u32,
}

const ATTR_READ_ONLY: u8  = 0x01;
const ATTR_HIDDEN: u8     = 0x02;
const ATTR_SYSTEM: u8     = 0x04;
const ATTR_VOLUME_ID: u8  = 0x08;
const ATTR_DIRECTORY: u8  = 0x10;
const ATTR_ARCHIVE: u8    = 0x20;
const ATTR_LONG_NAME: u8  = ATTR_READ_ONLY | ATTR_HIDDEN | ATTR_SYSTEM | ATTR_VOLUME_ID;

const DIR_ENTRY_SIZE: usize = 32;
const FAT_EOC: u32 = 0x0FFF_FFF8; // end-of-chain marker (>= this value means EOC)

// ---------------------------------------------------------------------------
// Open file tracking
// ---------------------------------------------------------------------------

struct OpenFile {
    start_cluster: u32,
    size: u32,
    is_dir: bool,
    /// Which directory contains this file (cluster), and the entry index within it
    parent_cluster: u32,
    entry_index: usize,
    dirty: bool,
    data: Vec<u8>, // cached file data (for writes)
}

// ---------------------------------------------------------------------------
// FAT32 state
// ---------------------------------------------------------------------------

struct Fat32State {
    bpb: Bpb,
    fat_start_sector: u32,
    data_start_sector: u32,
    open_files: BTreeMap<u64, OpenFile>,
    next_inode: u64,
}

impl Fat32State {
    /// Cluster number → first sector of that cluster
    fn cluster_to_sector(&self, cluster: u32) -> u64 {
        let c = cluster as u64;
        let ds = self.data_start_sector as u64;
        let spc = self.bpb.sectors_per_cluster as u64;
        ds + (c - 2) * spc
    }

    /// Read a full cluster into buf. Returns bytes read.
    fn read_cluster(&self, cluster: u32, buf: &mut Vec<u8>) {
        let sector = self.cluster_to_sector(cluster);
        let spc = self.bpb.sectors_per_cluster as u64;
        let mut sec_buf = [0u8; SECTOR_SIZE];
        for i in 0..spc {
            if blk_read_sector(sector + i, &mut sec_buf) {
                buf.extend_from_slice(&sec_buf);
            }
        }
    }

    /// Read the FAT entry for a given cluster.
    fn fat_entry(&self, cluster: u32) -> u32 {
        let fat_offset = cluster as u64 * 4;
        let fat_sector = self.fat_start_sector as u64 + fat_offset / SECTOR_SIZE as u64;
        let entry_offset = (fat_offset % SECTOR_SIZE as u64) as usize;
        let mut sec = [0u8; SECTOR_SIZE];
        if !blk_read_sector(fat_sector, &mut sec) {
            return FAT_EOC;
        }
        let val = u32::from_le_bytes([
            sec[entry_offset],
            sec[entry_offset + 1],
            sec[entry_offset + 2],
            sec[entry_offset + 3],
        ]);
        val & 0x0FFF_FFFF
    }

    /// Write a FAT entry.
    fn set_fat_entry(&self, cluster: u32, value: u32) {
        let fat_offset = cluster as u64 * 4;
        let fat_sector = self.fat_start_sector as u64 + fat_offset / SECTOR_SIZE as u64;
        let entry_offset = (fat_offset % SECTOR_SIZE as u64) as usize;
        let mut sec = [0u8; SECTOR_SIZE];
        if !blk_read_sector(fat_sector, &mut sec) {
            return;
        }
        // Preserve upper 4 bits
        let old = u32::from_le_bytes([
            sec[entry_offset], sec[entry_offset + 1],
            sec[entry_offset + 2], sec[entry_offset + 3],
        ]);
        let new_val = (old & 0xF000_0000) | (value & 0x0FFF_FFFF);
        let bytes = new_val.to_le_bytes();
        sec[entry_offset..entry_offset + 4].copy_from_slice(&bytes);
        let _ = blk_write_sector(fat_sector, &sec);
    }

    /// Follow the cluster chain and read all data.
    fn read_chain(&self, start_cluster: u32) -> Vec<u8> {
        let mut data = Vec::new();
        let mut cluster = start_cluster;
        loop {
            if cluster < 2 || cluster >= FAT_EOC {
                break;
            }
            self.read_cluster(cluster, &mut data);
            cluster = self.fat_entry(cluster);
        }
        data
    }

    /// Allocate a free cluster from the FAT. Returns cluster number.
    fn alloc_cluster(&self) -> Option<u32> {
        // Simple linear scan of FAT (starting from cluster 2)
        let total = self.bpb.total_sectors_32 / self.bpb.sectors_per_cluster as u32;
        for c in 2..total {
            if self.fat_entry(c) == 0 {
                // Mark as end-of-chain
                self.set_fat_entry(c, 0x0FFF_FFFF);
                // Zero the cluster
                let sector = self.cluster_to_sector(c);
                let zero = [0u8; SECTOR_SIZE];
                for i in 0..self.bpb.sectors_per_cluster as u64 {
                    let _ = blk_write_sector(sector + i, &zero);
                }
                return Some(c);
            }
        }
        None
    }

    /// Write cluster chain data back to disk.
    fn write_chain(&self, start_cluster: u32, data: &[u8]) {
        let cluster_size = self.bpb.sectors_per_cluster as usize * SECTOR_SIZE;
        let mut cluster = start_cluster;
        let mut offset = 0usize;

        loop {
            if cluster < 2 || cluster >= FAT_EOC { break; }
            let sector = self.cluster_to_sector(cluster);
            let spc = self.bpb.sectors_per_cluster as u64;

            for i in 0..spc {
                let mut sec = [0u8; SECTOR_SIZE];
                let start = offset + i as usize * SECTOR_SIZE;
                let end = (start + SECTOR_SIZE).min(data.len());
                if start < data.len() {
                    let len = end - start;
                    sec[..len].copy_from_slice(&data[start..end]);
                }
                let _ = blk_write_sector(sector + i, &sec);
            }
            offset += cluster_size;
            if offset >= data.len() { break; }

            let next = self.fat_entry(cluster);
            if next >= FAT_EOC {
                // Need to allocate another cluster
                if let Some(new_c) = self.alloc_cluster() {
                    self.set_fat_entry(cluster, new_c);
                    cluster = new_c;
                } else {
                    break;
                }
            } else {
                cluster = next;
            }
        }
    }

    /// Read directory entries from a cluster chain.
    fn read_dir_entries(&self, dir_cluster: u32) -> Vec<(String, DirEntry32, usize)> {
        let raw = self.read_chain(dir_cluster);
        let mut entries = Vec::new();
        let mut lfn_parts: Vec<(u8, String)> = Vec::new(); // (seq, partial name)

        let count = raw.len() / DIR_ENTRY_SIZE;
        for i in 0..count {
            let off = i * DIR_ENTRY_SIZE;
            if raw[off] == 0x00 { break; } // end of directory
            if raw[off] == 0xE5 { continue; } // deleted entry

            let entry = unsafe {
                core::ptr::read_unaligned(raw.as_ptr().add(off) as *const DirEntry32)
            };

            if entry.attr & ATTR_LONG_NAME == ATTR_LONG_NAME {
                // LFN entry — extract name fragment
                let seq = raw[off] & 0x1F;
                let mut name_chars = Vec::new();
                // Characters at offsets 1,3,5,7,9 (5 chars), 14,16,18,20,22,24 (6 chars), 28,30 (2 chars)
                let char_offsets = [1,3,5,7,9, 14,16,18,20,22,24, 28,30];
                for &co in &char_offsets {
                    if off + co + 1 < raw.len() {
                        let ch = u16::from_le_bytes([raw[off + co], raw[off + co + 1]]);
                        if ch == 0 || ch == 0xFFFF { break; }
                        if let Some(c) = char::from_u32(ch as u32) {
                            name_chars.push(c);
                        }
                    }
                }
                let part: String = name_chars.into_iter().collect();
                lfn_parts.push((seq, part));
                continue;
            }

            if entry.attr & ATTR_VOLUME_ID != 0 { continue; }

            // Determine name: use LFN if available, otherwise 8.3
            let name = if !lfn_parts.is_empty() {
                lfn_parts.sort_by_key(|(seq, _)| *seq);
                let full: String = lfn_parts.iter().map(|(_, p)| p.as_str()).collect();
                lfn_parts.clear();
                full
            } else {
                lfn_parts.clear();
                short_name_to_string(&entry.name)
            };

            entries.push((name, entry, i));
        }
        entries
    }

    /// Update a directory entry on disk.
    fn update_dir_entry(&self, dir_cluster: u32, entry_index: usize, entry: &DirEntry32) {
        let cluster_size = self.bpb.sectors_per_cluster as usize * SECTOR_SIZE;
        let byte_offset = entry_index * DIR_ENTRY_SIZE;
        let cluster_offset = byte_offset / cluster_size;
        let offset_in_cluster = byte_offset % cluster_size;

        // Walk the chain to find the right cluster
        let mut cluster = dir_cluster;
        for _ in 0..cluster_offset {
            cluster = self.fat_entry(cluster);
            if cluster >= FAT_EOC { return; }
        }

        let sector_in_cluster = offset_in_cluster / SECTOR_SIZE;
        let offset_in_sector = offset_in_cluster % SECTOR_SIZE;

        let sector = self.cluster_to_sector(cluster) + sector_in_cluster as u64;
        let mut sec = [0u8; SECTOR_SIZE];
        if !blk_read_sector(sector, &mut sec) { return; }

        let entry_bytes = unsafe {
            core::slice::from_raw_parts(entry as *const DirEntry32 as *const u8, DIR_ENTRY_SIZE)
        };
        sec[offset_in_sector..offset_in_sector + DIR_ENTRY_SIZE].copy_from_slice(entry_bytes);
        let _ = blk_write_sector(sector, &sec);
    }

    /// Add a new directory entry to a directory.
    fn add_dir_entry(&self, dir_cluster: u32, name: &str, attr: u8, cluster: u32, size: u32) -> Option<usize> {
        let raw = self.read_chain(dir_cluster);
        let count = raw.len() / DIR_ENTRY_SIZE;

        // Find a free slot (0x00 or 0xE5)
        let mut slot = None;
        for i in 0..count {
            let off = i * DIR_ENTRY_SIZE;
            if raw[off] == 0x00 || raw[off] == 0xE5 {
                slot = Some(i);
                break;
            }
        }

        let index = match slot {
            Some(i) => i,
            None => {
                // Need to extend the directory (allocate another cluster)
                // For simplicity, fail if the directory is full
                return None;
            }
        };

        let short = string_to_short_name(name);
        let entry = DirEntry32 {
            name: short,
            attr,
            _nt_res: 0,
            _crt_time_tenth: 0,
            _crt_time: 0,
            _crt_date: 0,
            _acc_date: 0,
            cluster_hi: (cluster >> 16) as u16,
            _wrt_time: 0,
            _wrt_date: 0,
            cluster_lo: cluster as u16,
            file_size: size,
        };

        self.update_dir_entry(dir_cluster, index, &entry);
        Some(index)
    }

    /// Resolve a path to (parent_cluster, entry_name, entry) or just the entry for the target.
    fn resolve_path(&self, path: &str) -> IoResult<(u32, DirEntry32, u32, usize)> {
        // Returns (cluster, dir_entry, parent_cluster, entry_index)
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            // Root directory
            return Ok((self.bpb.root_cluster, DirEntry32 {
                name: [0x20; 11],
                attr: ATTR_DIRECTORY,
                _nt_res: 0, _crt_time_tenth: 0, _crt_time: 0, _crt_date: 0,
                _acc_date: 0, _wrt_time: 0, _wrt_date: 0,
                cluster_hi: (self.bpb.root_cluster >> 16) as u16,
                cluster_lo: self.bpb.root_cluster as u16,
                file_size: 0,
            }, 0, 0));
        }

        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut current_cluster = self.bpb.root_cluster;
        let mut parent_cluster = self.bpb.root_cluster;
        let mut last_entry = None;
        let mut last_index = 0usize;

        for (ci, component) in components.iter().enumerate() {
            let entries = self.read_dir_entries(current_cluster);
            let mut found = false;
            for (name, entry, idx) in &entries {
                if name.eq_ignore_ascii_case(component) {
                    let cluster = ((entry.cluster_hi as u32) << 16) | (entry.cluster_lo as u32);
                    parent_cluster = current_cluster;
                    last_index = *idx;
                    if ci + 1 < components.len() {
                        // Must be a directory
                        if entry.attr & ATTR_DIRECTORY == 0 {
                            return Err(IoError::NotADirectory);
                        }
                        current_cluster = cluster;
                    } else {
                        // Final component
                        current_cluster = cluster;
                        last_entry = Some(*entry);
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(IoError::NotFound);
            }
        }

        match last_entry {
            Some(e) => Ok((current_cluster, e, parent_cluster, last_index)),
            None => Err(IoError::NotFound),
        }
    }
}

static FAT32: Mutex<Option<Fat32State>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// Filesystem trait implementation
// ---------------------------------------------------------------------------

struct Fat32Driver;

impl Filesystem for Fat32Driver {
    fn name(&self) -> &str { "fat32" }

    fn open(&self, path: &str, _mode: AccessMode) -> IoResult<(u64, FileType)> {
        let mut fs = FAT32.lock();
        let fs = fs.as_mut().ok_or(IoError::DeviceError)?;

        let (cluster, entry, _parent, _idx) = fs.resolve_path(path)?;
        let is_dir = entry.attr & ATTR_DIRECTORY != 0;
        let ft = if is_dir { FileType::Directory } else { FileType::Regular };

        let inode = fs.next_inode;
        fs.next_inode += 1;

        // Read file data into memory
        let data = if !is_dir {
            let mut d = fs.read_chain(cluster);
            d.truncate(entry.file_size as usize);
            d
        } else {
            Vec::new()
        };

        fs.open_files.insert(inode, OpenFile {
            start_cluster: cluster,
            size: entry.file_size,
            is_dir,
            parent_cluster: _parent,
            entry_index: _idx,
            dirty: false,
            data,
        });

        Ok((inode, ft))
    }

    fn create_file(&self, path: &str) -> IoResult<u64> {
        let mut fs = FAT32.lock();
        let fs = fs.as_mut().ok_or(IoError::DeviceError)?;

        // Check if already exists
        if fs.resolve_path(path).is_ok() {
            return Err(IoError::AlreadyExists);
        }

        // Find parent directory
        let (parent_path, file_name) = split_path(path);
        let (parent_cluster, parent_entry, _, _) = fs.resolve_path(parent_path)?;
        if parent_entry.attr & ATTR_DIRECTORY == 0 {
            return Err(IoError::NotADirectory);
        }

        // Allocate a cluster for the new file
        let new_cluster = fs.alloc_cluster().ok_or(IoError::NoSpace)?;

        // Add directory entry
        let idx = fs.add_dir_entry(parent_cluster, file_name, ATTR_ARCHIVE, new_cluster, 0)
            .ok_or(IoError::NoSpace)?;

        let inode = fs.next_inode;
        fs.next_inode += 1;

        fs.open_files.insert(inode, OpenFile {
            start_cluster: new_cluster,
            size: 0,
            is_dir: false,
            parent_cluster: parent_cluster,
            entry_index: idx,
            dirty: false,
            data: Vec::new(),
        });

        Ok(inode)
    }

    fn read(&self, inode: u64, offset: u64, buf: &mut [u8]) -> IoResult<usize> {
        let fs = FAT32.lock();
        let fs = fs.as_ref().ok_or(IoError::DeviceError)?;

        let file = fs.open_files.get(&inode).ok_or(IoError::InvalidHandle)?;
        let offset = offset as usize;
        if offset >= file.data.len() {
            return Ok(0);
        }
        let available = file.data.len() - offset;
        let to_read = buf.len().min(available);
        buf[..to_read].copy_from_slice(&file.data[offset..offset + to_read]);
        Ok(to_read)
    }

    fn write(&self, inode: u64, offset: u64, buf: &[u8]) -> IoResult<usize> {
        let mut fs = FAT32.lock();
        let fs = fs.as_mut().ok_or(IoError::DeviceError)?;

        let file = fs.open_files.get_mut(&inode).ok_or(IoError::InvalidHandle)?;
        let offset = offset as usize;

        // Extend data buffer if needed
        if offset + buf.len() > file.data.len() {
            file.data.resize(offset + buf.len(), 0);
        }
        file.data[offset..offset + buf.len()].copy_from_slice(buf);
        file.size = file.data.len() as u32;
        file.dirty = true;
        Ok(buf.len())
    }

    fn close(&self, inode: u64) {
        let mut fs = FAT32.lock();
        let fs = match fs.as_mut() {
            Some(f) => f,
            None => return,
        };

        if let Some(file) = fs.open_files.remove(&inode) {
            if file.dirty && !file.is_dir {
                // Flush data to disk
                fs.write_chain(file.start_cluster, &file.data);

                // Update directory entry with new size
                let raw = fs.read_chain(file.parent_cluster);
                let off = file.entry_index * DIR_ENTRY_SIZE;
                if off + DIR_ENTRY_SIZE <= raw.len() {
                    let mut entry = unsafe {
                        core::ptr::read_unaligned(raw.as_ptr().add(off) as *const DirEntry32)
                    };
                    entry.file_size = file.size;
                    fs.update_dir_entry(file.parent_cluster, file.entry_index, &entry);
                }
            }
        }
    }

    fn mkdir(&self, path: &str) -> IoResult<()> {
        let mut fs = FAT32.lock();
        let fs = fs.as_mut().ok_or(IoError::DeviceError)?;

        if fs.resolve_path(path).is_ok() {
            return Err(IoError::AlreadyExists);
        }

        let (parent_path, dir_name) = split_path(path);
        let (parent_cluster, parent_entry, _, _) = fs.resolve_path(parent_path)?;
        if parent_entry.attr & ATTR_DIRECTORY == 0 {
            return Err(IoError::NotADirectory);
        }

        let new_cluster = fs.alloc_cluster().ok_or(IoError::NoSpace)?;

        // Add . and .. entries in the new directory
        let dot = DirEntry32 {
            name: *b".          ",
            attr: ATTR_DIRECTORY,
            _nt_res: 0, _crt_time_tenth: 0, _crt_time: 0, _crt_date: 0,
            _acc_date: 0, _wrt_time: 0, _wrt_date: 0,
            cluster_hi: (new_cluster >> 16) as u16,
            cluster_lo: new_cluster as u16,
            file_size: 0,
        };
        let dotdot = DirEntry32 {
            name: *b"..         ",
            attr: ATTR_DIRECTORY,
            _nt_res: 0, _crt_time_tenth: 0, _crt_time: 0, _crt_date: 0,
            _acc_date: 0, _wrt_time: 0, _wrt_date: 0,
            cluster_hi: (parent_cluster >> 16) as u16,
            cluster_lo: parent_cluster as u16,
            file_size: 0,
        };

        fs.update_dir_entry(new_cluster, 0, &dot);
        fs.update_dir_entry(new_cluster, 1, &dotdot);

        // Add entry in parent
        fs.add_dir_entry(parent_cluster, dir_name, ATTR_DIRECTORY, new_cluster, 0)
            .ok_or(IoError::NoSpace)?;

        Ok(())
    }

    fn readdir(&self, path: &str) -> IoResult<Vec<DirEntry>> {
        let fs = FAT32.lock();
        let fs = fs.as_ref().ok_or(IoError::DeviceError)?;

        let (cluster, entry, _, _) = fs.resolve_path(path)?;
        if entry.attr & ATTR_DIRECTORY == 0 {
            return Err(IoError::NotADirectory);
        }

        let raw_entries = fs.read_dir_entries(cluster);
        let mut result = Vec::new();
        for (name, de, _) in &raw_entries {
            if name == "." || name == ".." { continue; }
            let ft = if de.attr & ATTR_DIRECTORY != 0 {
                FileType::Directory
            } else {
                FileType::Regular
            };
            result.push(DirEntry {
                name: name.clone(),
                file_type: ft,
                size: de.file_size as u64,
            });
        }
        Ok(result)
    }

    fn stat(&self, path: &str) -> IoResult<FileInfo> {
        let fs = FAT32.lock();
        let fs = fs.as_ref().ok_or(IoError::DeviceError)?;

        let (_cluster, entry, _, _) = fs.resolve_path(path)?;
        let ft = if entry.attr & ATTR_DIRECTORY != 0 {
            FileType::Directory
        } else {
            FileType::Regular
        };
        Ok(FileInfo {
            file_type: ft,
            size: entry.file_size as u64,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }

    fn unlink(&self, path: &str) -> IoResult<()> {
        let mut fs = FAT32.lock();
        let fs = fs.as_mut().ok_or(IoError::DeviceError)?;

        let (cluster, entry, parent_cluster, entry_index) = fs.resolve_path(path)?;

        // If directory, check it's empty
        if entry.attr & ATTR_DIRECTORY != 0 {
            let entries = fs.read_dir_entries(cluster);
            let non_dot = entries.iter().filter(|(n, _, _)| n != "." && n != "..").count();
            if non_dot > 0 {
                return Err(IoError::NotEmpty);
            }
        }

        // Mark directory entry as deleted (0xE5)
        let raw = fs.read_chain(parent_cluster);
        let off = entry_index * DIR_ENTRY_SIZE;
        if off < raw.len() {
            let mut del_entry = unsafe {
                core::ptr::read_unaligned(raw.as_ptr().add(off) as *const DirEntry32)
            };
            del_entry.name[0] = 0xE5;
            fs.update_dir_entry(parent_cluster, entry_index, &del_entry);
        }

        // Free the cluster chain
        let mut c = cluster;
        loop {
            if c < 2 || c >= FAT_EOC { break; }
            let next = fs.fat_entry(c);
            fs.set_fat_entry(c, 0);
            c = next;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert an 8.3 short name to a readable string.
fn short_name_to_string(name: &[u8; 11]) -> String {
    let base: String = name[0..8].iter()
        .map(|&b| b as char)
        .collect::<String>()
        .trim_end()
        .to_string();
    let ext: String = name[8..11].iter()
        .map(|&b| b as char)
        .collect::<String>()
        .trim_end()
        .to_string();

    let mut result = base.to_lowercase();
    if !ext.is_empty() {
        result.push('.');
        result.push_str(&ext.to_lowercase());
    }
    result
}

fn to_upper(c: u8) -> u8 {
    if c >= b'a' && c <= b'z' { c - 32 } else { c }
}

/// Convert a string to an 8.3 short name.
fn string_to_short_name(name: &str) -> [u8; 11] {
    let mut short = [0x20u8; 11];
    let name_upper = name.as_bytes();

    // Find the last dot
    let dot_pos = name.rfind('.');
    let (base, ext) = match dot_pos {
        Some(pos) => (&name_upper[..pos], &name_upper[pos + 1..]),
        None => (name_upper, &[] as &[u8]),
    };

    // Copy base (up to 8 chars)
    for (i, &b) in base.iter().take(8).enumerate() {
        short[i] = to_upper(b);
    }
    // Copy extension (up to 3 chars)
    for (i, &b) in ext.iter().take(3).enumerate() {
        short[8 + i] = to_upper(b);
    }
    short
}

/// Split a path into (parent_path, file_name).
fn split_path(path: &str) -> (&str, &str) {
    let path = path.trim_start_matches('/');
    match path.rfind('/') {
        Some(pos) => {
            let parent = &path[..pos];
            let name = &path[pos + 1..];
            (parent, name)
        }
        None => ("", path),
    }
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Try to mount a FAT32 filesystem from the virtio-blk device.
pub fn init() {
    if !blk_is_available() {
        crate::kprintln!("[fat32] No block device available, skipping");
        return;
    }

    // Read sector 0 (boot sector / BPB)
    let mut sector0 = [0u8; SECTOR_SIZE];
    if !blk_read_sector(0, &mut sector0) {
        crate::kprintln!("[fat32] Failed to read boot sector");
        return;
    }

    // Parse BPB
    let bytes_per_sector = u16::from_le_bytes([sector0[11], sector0[12]]);
    let sectors_per_cluster = sector0[13];
    let reserved_sectors = u16::from_le_bytes([sector0[14], sector0[15]]);
    let num_fats = sector0[16];
    let total_sectors_32 = u32::from_le_bytes([sector0[32], sector0[33], sector0[34], sector0[35]]);
    let fat_size_32 = u32::from_le_bytes([sector0[36], sector0[37], sector0[38], sector0[39]]);
    let root_cluster = u32::from_le_bytes([sector0[44], sector0[45], sector0[46], sector0[47]]);

    if bytes_per_sector != 512 || sectors_per_cluster == 0 {
        crate::kprintln!("[fat32] Invalid BPB: bps={} spc={}", bytes_per_sector, sectors_per_cluster);
        return;
    }

    let bpb = Bpb {
        bytes_per_sector,
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        total_sectors_32,
        fat_size_32,
        root_cluster,
    };

    let fat_start_sector = reserved_sectors as u32;
    let data_start_sector = fat_start_sector + (num_fats as u32 * fat_size_32);

    crate::kprintln!("[fat32] BPS={} SPC={} FATs={} FATsize={} root_cluster={}",
        bytes_per_sector, sectors_per_cluster, num_fats, fat_size_32, root_cluster);
    crate::kprintln!("[fat32] FAT starts at sector {}, data at sector {}",
        fat_start_sector, data_start_sector);

    *FAT32.lock() = Some(Fat32State {
        bpb,
        fat_start_sector,
        data_start_sector,
        open_files: BTreeMap::new(),
        next_inode: 1,
    });

    // Mount at /mnt/disk
    // First create the mount point directory in ramfs
    let _ = crate::fs::mkdir("/mnt");
    let _ = crate::fs::mkdir("/mnt/disk");
    vfs::mount("/mnt/disk", Box::new(Fat32Driver));

    crate::kprintln!("[fat32] Mounted at /mnt/disk");
}
