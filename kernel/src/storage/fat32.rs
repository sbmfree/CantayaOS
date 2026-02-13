// FAT32 Filesystem Driver
//
// Implements read-write FAT32 filesystem access per the Microsoft FAT specification.
//
// FAT32 Layout:
//   [Boot Sector (BPB)] [FSInfo] [Reserved] [FAT1] [FAT2] [Data Clusters...]
//
// Key concepts:
//   - Cluster: allocation unit (group of sectors, typically 4-8 KiB)
//   - FAT: File Allocation Table — linked list of cluster chains
//   - Directory entry: 32 bytes with 8.3 filename, attributes, cluster, size
//   - LFN: Long File Name entries use multiple 32-byte slots before the 8.3 entry
//
// FAT entry values:
//   0x00000000 = free cluster
//   0x00000002..0x0FFFFFEF = next cluster in chain
//   0x0FFFFFF8..0x0FFFFFFF = end of chain
//   0x0FFFFFF7 = bad cluster

use alloc::string::String;
use alloc::vec::Vec;
use crate::storage::block;
use spin::Mutex;

/// FAT entry constants
const FAT_FREE: u32 = 0x00000000;
const FAT_EOC: u32 = 0x0FFFFFF8;  // End of chain (or higher)
const FAT_BAD: u32 = 0x0FFFFFF7;
const FAT_MASK: u32 = 0x0FFFFFFF; // FAT32 uses only 28 bits

/// Directory entry attribute flags
const ATTR_READ_ONLY: u8 = 0x01;
const ATTR_HIDDEN: u8 = 0x02;
const ATTR_SYSTEM: u8 = 0x04;
const ATTR_VOLUME_ID: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LFN: u8 = 0x0F; // Long filename entry

/// A parsed directory entry
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
    pub cluster: u32,
    /// Sector of the directory entry on disk
    pub entry_sector: u64,
    /// Byte offset within the sector
    pub entry_offset: usize,
}

/// Raw 32-byte directory entry on disk
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct RawDirEntry {
    name: [u8; 11],      // 8.3 filename
    attr: u8,            // Attribute flags
    nt_reserved: u8,     // Reserved for Windows NT
    create_time_tenth: u8,
    create_time: u16,
    create_date: u16,
    access_date: u16,
    cluster_high: u16,   // High 16 bits of first cluster
    write_time: u16,
    write_date: u16,
    cluster_low: u16,    // Low 16 bits of first cluster
    file_size: u32,
}

/// FAT32 filesystem state
struct Fat32State {
    /// Sectors per cluster
    sectors_per_cluster: u8,
    /// Number of reserved sectors (before FAT)
    reserved_sectors: u16,
    /// Number of FATs (usually 2)
    num_fats: u8,
    /// Sectors per FAT
    sectors_per_fat: u32,
    /// Root directory cluster
    root_cluster: u32,
    /// First data sector (where cluster 2 starts)
    data_start_sector: u64,
    /// Total data clusters
    total_clusters: u32,
    /// Bytes per sector (always 512 for us)
    bytes_per_sector: u16,
    /// Volume label
    volume_label: String,
}

/// Global FAT32 state
static FAT32: Mutex<Option<Fat32State>> = Mutex::new(None);

/// Initialize the FAT32 filesystem by reading the boot sector.
pub fn init() -> Result<(), &'static str> {
    let mut sector = [0u8; 512];
    if !block::read_block(0, &mut sector) {
        return Err("failed to read boot sector");
    }

    // Validate boot signature
    if sector[510] != 0x55 || sector[511] != 0xAA {
        return Err("invalid boot signature");
    }

    // Parse BPB
    let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
    let sectors_per_cluster = sector[13];
    let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
    let num_fats = sector[16];
    let total_sectors_16 = u16::from_le_bytes([sector[19], sector[20]]);
    let sectors_per_fat_16 = u16::from_le_bytes([sector[22], sector[23]]);
    let total_sectors_32 = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);

    // FAT32 specific
    let sectors_per_fat = if sectors_per_fat_16 != 0 {
        sectors_per_fat_16 as u32
    } else {
        u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]])
    };
    let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]]);

    let total_sectors = if total_sectors_16 != 0 {
        total_sectors_16 as u64
    } else {
        total_sectors_32 as u64
    };

    // Calculate layout
    let fat_start = reserved_sectors as u64;
    let data_start = fat_start + (num_fats as u64 * sectors_per_fat as u64);
    let data_sectors = total_sectors - data_start;
    let total_clusters = (data_sectors / sectors_per_cluster as u64) as u32;

    // Read volume label from BPB (offset 71 for FAT32)
    let label_bytes = &sector[71..82];
    let volume_label = core::str::from_utf8(label_bytes)
        .unwrap_or("NO NAME")
        .trim()
        .into();

    log::info!("FAT32: volume='{}' clusters={} spc={} root_cluster={}",
        volume_label, total_clusters, sectors_per_cluster, root_cluster);
    log::info!("FAT32: reserved={} fat_sectors={} data_start={}",
        reserved_sectors, sectors_per_fat, data_start);

    let state = Fat32State {
        sectors_per_cluster,
        reserved_sectors,
        num_fats,
        sectors_per_fat,
        root_cluster,
        data_start_sector: data_start,
        total_clusters,
        bytes_per_sector,
        volume_label,
    };

    *FAT32.lock() = Some(state);
    Ok(())
}

// ============================================================================
// Low-Level FAT Operations
// ============================================================================

/// Convert a cluster number to its starting sector.
fn cluster_to_sector(state: &Fat32State, cluster: u32) -> u64 {
    state.data_start_sector + ((cluster - 2) as u64 * state.sectors_per_cluster as u64)
}

/// Read a FAT entry for a given cluster.
fn fat_read(state: &Fat32State, cluster: u32) -> u32 {
    let fat_offset = cluster * 4;
    let fat_sector = state.reserved_sectors as u64 + (fat_offset as u64 / 512);
    let offset_in_sector = (fat_offset % 512) as usize;

    let mut sector = [0u8; 512];
    if !block::read_block(fat_sector, &mut sector) {
        return FAT_BAD;
    }

    let entry = u32::from_le_bytes([
        sector[offset_in_sector],
        sector[offset_in_sector + 1],
        sector[offset_in_sector + 2],
        sector[offset_in_sector + 3],
    ]);

    entry & FAT_MASK
}

/// Write a FAT entry for a given cluster (updates both FATs).
fn fat_write(state: &Fat32State, cluster: u32, value: u32) -> bool {
    let fat_offset = cluster * 4;
    let fat_sector_offset = fat_offset as u64 / 512;
    let offset_in_sector = (fat_offset % 512) as usize;

    // Update both FAT copies
    for fat_idx in 0..state.num_fats {
        let fat_sector = state.reserved_sectors as u64
            + (fat_idx as u64 * state.sectors_per_fat as u64)
            + fat_sector_offset;

        let mut sector = [0u8; 512];
        if !block::read_block(fat_sector, &mut sector) {
            return false;
        }

        // Preserve the upper 4 bits
        let existing = u32::from_le_bytes([
            sector[offset_in_sector],
            sector[offset_in_sector + 1],
            sector[offset_in_sector + 2],
            sector[offset_in_sector + 3],
        ]);
        let new_value = (existing & 0xF0000000) | (value & FAT_MASK);
        let bytes = new_value.to_le_bytes();
        sector[offset_in_sector..offset_in_sector + 4].copy_from_slice(&bytes);

        if !block::write_block(fat_sector, &sector) {
            return false;
        }
    }

    true
}

/// Allocate a free cluster from the FAT.
/// Returns the cluster number or None if disk is full.
fn fat_alloc(state: &Fat32State) -> Option<u32> {
    // Linear scan from cluster 2
    for cluster in 2..state.total_clusters + 2 {
        let entry = fat_read(state, cluster);
        if entry == FAT_FREE {
            // Mark as end-of-chain
            if fat_write(state, cluster, 0x0FFFFFFF) {
                return Some(cluster);
            }
        }
    }
    None
}

/// Free a cluster chain starting from `cluster`.
fn fat_free_chain(state: &Fat32State, start: u32) {
    let mut cluster = start;
    loop {
        let next = fat_read(state, cluster);
        fat_write(state, cluster, FAT_FREE);
        if next >= FAT_EOC || next == FAT_FREE || next == FAT_BAD {
            break;
        }
        cluster = next;
    }
}

/// Follow a cluster chain and collect all clusters.
fn cluster_chain(state: &Fat32State, start: u32) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut cluster = start;
    loop {
        if cluster < 2 || cluster >= FAT_EOC {
            break;
        }
        chain.push(cluster);
        cluster = fat_read(state, cluster);
        // Prevent infinite loops
        if chain.len() > state.total_clusters as usize {
            break;
        }
    }
    chain
}

// ============================================================================
// Directory Operations
// ============================================================================

/// Read all directory entries from a directory starting at `dir_cluster`.
pub fn read_dir(dir_cluster: u32) -> Vec<DirEntry> {
    let lock = FAT32.lock();
    let state = match lock.as_ref() {
        Some(s) => s,
        None => return Vec::new(),
    };

    read_dir_internal(state, dir_cluster)
}

fn read_dir_internal(state: &Fat32State, dir_cluster: u32) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    let chain = cluster_chain(state, dir_cluster);
    let sectors_per_cluster = state.sectors_per_cluster as usize;
    let mut lfn_parts: Vec<(u8, String)> = Vec::new();

    for &cluster in &chain {
        let start_sector = cluster_to_sector(state, cluster);

        for sec_off in 0..sectors_per_cluster {
            let sector_lba = start_sector + sec_off as u64;
            let mut sector = [0u8; 512];
            if !block::read_block(sector_lba, &mut sector) {
                continue;
            }

            for i in 0..16 {
                // 16 entries per 512-byte sector
                let offset = i * 32;
                let raw = &sector[offset..offset + 32];

                // End of directory
                if raw[0] == 0x00 {
                    return entries;
                }

                // Deleted entry
                if raw[0] == 0xE5 {
                    lfn_parts.clear();
                    continue;
                }

                let attr = raw[11];

                // LFN entry
                if attr == ATTR_LFN {
                    let seq = raw[0] & 0x3F;
                    let mut name_chars = Vec::new();

                    // LFN characters in UCS-2: bytes 1-10, 14-25, 28-31
                    let lfn_offsets: &[(usize, usize)] = &[
                        (1, 10), (14, 25), (28, 31),
                    ];

                    for &(start, end) in lfn_offsets {
                        let mut pos = start;
                        while pos + 1 <= end {
                            let ch = u16::from_le_bytes([raw[pos], raw[pos + 1]]);
                            if ch == 0x0000 || ch == 0xFFFF {
                                break;
                            }
                            // Simple UCS-2 to ASCII conversion
                            if ch < 0x80 {
                                name_chars.push(ch as u8 as char);
                            } else {
                                name_chars.push('?');
                            }
                            pos += 2;
                        }
                    }

                    let part: String = name_chars.iter().collect();
                    lfn_parts.push((seq, part));
                    continue;
                }

                // Volume label — skip
                if attr & ATTR_VOLUME_ID != 0 {
                    lfn_parts.clear();
                    continue;
                }

                // Regular 8.3 entry
                let cluster_hi = u16::from_le_bytes([raw[20], raw[21]]) as u32;
                let cluster_lo = u16::from_le_bytes([raw[26], raw[27]]) as u32;
                let cluster = (cluster_hi << 16) | cluster_lo;
                let size = u32::from_le_bytes([raw[28], raw[29], raw[30], raw[31]]);
                let is_dir = attr & ATTR_DIRECTORY != 0;

                // Build name
                let name = if !lfn_parts.is_empty() {
                    // Sort LFN parts by sequence number and concatenate
                    lfn_parts.sort_by_key(|(seq, _)| *seq);
                    let full_name: String = lfn_parts.iter().map(|(_, s)| s.as_str()).collect();
                    lfn_parts.clear();
                    full_name
                } else {
                    // Convert 8.3 name
                    decode_83_name(&raw[0..11])
                };

                // Skip . and .. entries
                if name == "." || name == ".." {
                    continue;
                }

                entries.push(DirEntry {
                    name,
                    is_dir,
                    size,
                    cluster,
                    entry_sector: sector_lba,
                    entry_offset: offset,
                });
            }
        }
    }

    entries
}

/// Decode an 8.3 filename from raw bytes.
fn decode_83_name(raw: &[u8]) -> String {
    let base = core::str::from_utf8(&raw[0..8]).unwrap_or("").trim();
    let ext = core::str::from_utf8(&raw[8..11]).unwrap_or("").trim();

    if ext.is_empty() {
        base.to_lowercase()
    } else {
        let mut s = base.to_lowercase();
        s.push('.');
        s.push_str(&ext.to_lowercase());
        s
    }
}

/// Encode a filename to 8.3 format. Returns None if name is invalid.
fn encode_83_name(name: &str) -> Option<[u8; 11]> {
    let mut result = [0x20u8; 11]; // Space-filled

    let name_upper = name.to_uppercase();
    let parts: Vec<&str> = name_upper.splitn(2, '.').collect();

    let base = parts[0];
    if base.is_empty() || base.len() > 8 {
        return None;
    }

    for (i, b) in base.bytes().enumerate() {
        if i >= 8 { break; }
        result[i] = b;
    }

    if parts.len() > 1 {
        let ext = parts[1];
        if ext.len() > 3 {
            return None;
        }
        for (i, b) in ext.bytes().enumerate() {
            if i >= 3 { break; }
            result[8 + i] = b;
        }
    }

    Some(result)
}

/// Find a directory entry by name within a directory.
pub fn find_entry(dir_cluster: u32, name: &str) -> Option<DirEntry> {
    let entries = read_dir(dir_cluster);
    let name_lower = name.to_lowercase();
    entries.into_iter().find(|e| e.name.to_lowercase() == name_lower)
}

// ============================================================================
// File Operations
// ============================================================================

/// Read a file's contents into a Vec.
pub fn read_file(entry: &DirEntry) -> Vec<u8> {
    let lock = FAT32.lock();
    let state = match lock.as_ref() {
        Some(s) => s,
        None => return Vec::new(),
    };

    if entry.cluster < 2 {
        return Vec::new();
    }

    let chain = cluster_chain(state, entry.cluster);
    let cluster_size = state.sectors_per_cluster as usize * 512;
    let mut data = Vec::with_capacity(entry.size as usize);

    let mut remaining = entry.size as usize;

    for &cluster in &chain {
        let start_sector = cluster_to_sector(state, cluster);
        let to_read = remaining.min(cluster_size);

        for sec_off in 0..(state.sectors_per_cluster as usize) {
            if remaining == 0 {
                break;
            }
            let mut sector = [0u8; 512];
            if !block::read_block(start_sector + sec_off as u64, &mut sector) {
                return data;
            }
            let chunk = remaining.min(512);
            data.extend_from_slice(&sector[..chunk]);
            remaining -= chunk;
        }

        let _ = to_read; // suppress warning
    }

    data
}

/// Write data to a file. Creates or overwrites the file in the given directory.
///
/// Returns true on success.
pub fn write_file(dir_cluster: u32, name: &str, data: &[u8]) -> bool {
    let mut lock = FAT32.lock();
    let state = match lock.as_mut() {
        Some(s) => s,
        None => return false,
    };

    // Check if file already exists
    let entries = read_dir_internal(state, dir_cluster);
    let existing = entries.iter().find(|e| e.name.to_lowercase() == name.to_lowercase());

    if let Some(entry) = existing {
        // Free old cluster chain
        if entry.cluster >= 2 {
            fat_free_chain(state, entry.cluster);
        }

        // Allocate new clusters and write data
        let new_cluster = write_data_clusters(state, data);

        // Update directory entry
        update_dir_entry_cluster_size(state, entry.entry_sector, entry.entry_offset,
            new_cluster.unwrap_or(0), data.len() as u32);

        return true;
    }

    // Create new file
    let new_cluster = if !data.is_empty() {
        write_data_clusters(state, data)
    } else {
        Some(0) // Empty file, no clusters needed
    };

    let cluster = new_cluster.unwrap_or(0);
    create_dir_entry(state, dir_cluster, name, false, cluster, data.len() as u32)
}

/// Write data into newly allocated clusters. Returns the first cluster or None.
fn write_data_clusters(state: &Fat32State, data: &[u8]) -> Option<u32> {
    if data.is_empty() {
        return Some(0);
    }

    let cluster_size = state.sectors_per_cluster as usize * 512;
    let num_clusters = (data.len() + cluster_size - 1) / cluster_size;

    let mut clusters = Vec::new();
    for _ in 0..num_clusters {
        match fat_alloc(state) {
            Some(c) => clusters.push(c),
            None => {
                // Free already allocated clusters
                for &c in &clusters {
                    fat_write(state, c, FAT_FREE);
                }
                return None;
            }
        }
    }

    // Link clusters into a chain
    for i in 0..clusters.len() - 1 {
        fat_write(state, clusters[i], clusters[i + 1]);
    }
    // Last cluster is already marked EOC by fat_alloc

    // Write data to clusters
    let mut offset = 0;
    for &cluster in &clusters {
        let start_sector = cluster_to_sector(state, cluster);
        for sec_off in 0..state.sectors_per_cluster as usize {
            if offset >= data.len() {
                // Zero-fill remaining sectors
                let zero = [0u8; 512];
                block::write_block(start_sector + sec_off as u64, &zero);
            } else {
                let mut sector = [0u8; 512];
                let chunk = (data.len() - offset).min(512);
                sector[..chunk].copy_from_slice(&data[offset..offset + chunk]);
                if !block::write_block(start_sector + sec_off as u64, &sector) {
                    return None;
                }
                offset += chunk;
            }
        }
    }

    Some(clusters[0])
}

/// Update the cluster and size fields of an existing directory entry.
fn update_dir_entry_cluster_size(
    _state: &Fat32State,
    entry_sector: u64,
    entry_offset: usize,
    cluster: u32,
    size: u32,
) -> bool {
    let mut sector = [0u8; 512];
    if !block::read_block(entry_sector, &mut sector) {
        return false;
    }

    let raw = &mut sector[entry_offset..entry_offset + 32];
    // Cluster high (offset 20-21)
    raw[20] = (cluster >> 16) as u8;
    raw[21] = (cluster >> 24) as u8;
    // Cluster low (offset 26-27)
    raw[26] = cluster as u8;
    raw[27] = (cluster >> 8) as u8;
    // Size (offset 28-31)
    let size_bytes = size.to_le_bytes();
    raw[28..32].copy_from_slice(&size_bytes);

    block::write_block(entry_sector, &sector)
}

/// Create a new directory entry in a directory.
fn create_dir_entry(
    state: &Fat32State,
    dir_cluster: u32,
    name: &str,
    is_dir: bool,
    cluster: u32,
    size: u32,
) -> bool {
    let short_name = match encode_83_name(name) {
        Some(n) => n,
        None => {
            log::error!("FAT32: invalid filename '{}'", name);
            return false;
        }
    };

    let chain = cluster_chain(state, dir_cluster);
    let sectors_per_cluster = state.sectors_per_cluster as usize;

    // Find a free slot in the directory
    for &dir_clust in &chain {
        let start_sector = cluster_to_sector(state, dir_clust);
        for sec_off in 0..sectors_per_cluster {
            let sector_lba = start_sector + sec_off as u64;
            let mut sector = [0u8; 512];
            if !block::read_block(sector_lba, &mut sector) {
                continue;
            }

            for i in 0..16 {
                let offset = i * 32;
                // Free slot: first byte is 0x00 (end of dir) or 0xE5 (deleted)
                if sector[offset] == 0x00 || sector[offset] == 0xE5 {
                    // Build the directory entry
                    let mut entry = [0u8; 32];
                    entry[0..11].copy_from_slice(&short_name);
                    entry[11] = if is_dir { ATTR_DIRECTORY | ATTR_ARCHIVE } else { ATTR_ARCHIVE };
                    entry[20] = (cluster >> 16) as u8;
                    entry[21] = (cluster >> 24) as u8;
                    entry[26] = cluster as u8;
                    entry[27] = (cluster >> 8) as u8;
                    let size_bytes = size.to_le_bytes();
                    entry[28..32].copy_from_slice(&size_bytes);

                    sector[offset..offset + 32].copy_from_slice(&entry);

                    // If we overwrote a 0x00 (end of dir), mark the next one as end
                    if offset + 32 < 512 && i + 1 < 16 {
                        if sector[offset + 32] != 0xE5 {
                            // Leave existing entries alone
                        }
                    }

                    return block::write_block(sector_lba, &sector);
                }
            }
        }
    }

    // No free slot — need to extend the directory by allocating a new cluster
    let new_cluster = match fat_alloc(state) {
        Some(c) => c,
        None => return false,
    };

    // Link new cluster to the chain
    let last_cluster = *chain.last().unwrap_or(&dir_cluster);
    fat_write(state, last_cluster, new_cluster);

    // Zero the new cluster
    let start_sector = cluster_to_sector(state, new_cluster);
    let zero = [0u8; 512];
    for sec_off in 0..sectors_per_cluster {
        block::write_block(start_sector + sec_off as u64, &zero);
    }

    // Write entry at the start of the new cluster
    let mut sector = [0u8; 512];
    let mut entry = [0u8; 32];
    entry[0..11].copy_from_slice(&short_name);
    entry[11] = if is_dir { ATTR_DIRECTORY | ATTR_ARCHIVE } else { ATTR_ARCHIVE };
    entry[20] = (cluster >> 16) as u8;
    entry[21] = (cluster >> 24) as u8;
    entry[26] = cluster as u8;
    entry[27] = (cluster >> 8) as u8;
    let size_bytes = size.to_le_bytes();
    entry[28..32].copy_from_slice(&size_bytes);
    sector[0..32].copy_from_slice(&entry);

    block::write_block(start_sector, &sector)
}

/// Create a subdirectory.
pub fn mkdir(parent_cluster: u32, name: &str) -> bool {
    let mut lock = FAT32.lock();
    let state = match lock.as_mut() {
        Some(s) => s,
        None => return false,
    };

    // Check if already exists
    let entries = read_dir_internal(state, parent_cluster);
    if entries.iter().any(|e| e.name.to_lowercase() == name.to_lowercase()) {
        return false; // Already exists
    }

    // Allocate cluster for the new directory
    let dir_cluster = match fat_alloc(state) {
        Some(c) => c,
        None => return false,
    };

    // Zero the cluster
    let start_sector = cluster_to_sector(state, dir_cluster);
    let zero = [0u8; 512];
    for sec_off in 0..state.sectors_per_cluster as usize {
        block::write_block(start_sector + sec_off as u64, &zero);
    }

    // Create . and .. entries
    let mut sector = [0u8; 512];

    // . entry
    let mut dot_entry = [0u8; 32];
    dot_entry[0] = b'.';
    for i in 1..11 { dot_entry[i] = 0x20; }
    dot_entry[11] = ATTR_DIRECTORY;
    dot_entry[20] = (dir_cluster >> 16) as u8;
    dot_entry[21] = (dir_cluster >> 24) as u8;
    dot_entry[26] = dir_cluster as u8;
    dot_entry[27] = (dir_cluster >> 8) as u8;
    sector[0..32].copy_from_slice(&dot_entry);

    // .. entry
    let mut dotdot_entry = [0u8; 32];
    dotdot_entry[0] = b'.';
    dotdot_entry[1] = b'.';
    for i in 2..11 { dotdot_entry[i] = 0x20; }
    dotdot_entry[11] = ATTR_DIRECTORY;
    dotdot_entry[20] = (parent_cluster >> 16) as u8;
    dotdot_entry[21] = (parent_cluster >> 24) as u8;
    dotdot_entry[26] = parent_cluster as u8;
    dotdot_entry[27] = (parent_cluster >> 8) as u8;
    sector[32..64].copy_from_slice(&dotdot_entry);

    if !block::write_block(start_sector, &sector) {
        return false;
    }

    // Create directory entry in parent
    create_dir_entry(state, parent_cluster, name, true, dir_cluster, 0)
}

/// Delete a file or empty directory.
pub fn delete(dir_cluster: u32, name: &str) -> bool {
    let mut lock = FAT32.lock();
    let state = match lock.as_mut() {
        Some(s) => s,
        None => return false,
    };

    let entries = read_dir_internal(state, dir_cluster);
    let entry = match entries.iter().find(|e| e.name.to_lowercase() == name.to_lowercase()) {
        Some(e) => e.clone(),
        None => return false,
    };

    // If directory, check it's empty
    if entry.is_dir {
        let contents = read_dir_internal(state, entry.cluster);
        if !contents.is_empty() {
            return false; // Not empty
        }
    }

    // Free cluster chain
    if entry.cluster >= 2 {
        fat_free_chain(state, entry.cluster);
    }

    // Mark directory entry as deleted (set first byte to 0xE5)
    let mut sector = [0u8; 512];
    if !block::read_block(entry.entry_sector, &mut sector) {
        return false;
    }
    sector[entry.entry_offset] = 0xE5;
    block::write_block(entry.entry_sector, &sector)
}

/// Get the root directory cluster.
pub fn root_cluster() -> u32 {
    let lock = FAT32.lock();
    match lock.as_ref() {
        Some(s) => s.root_cluster,
        None => 0,
    }
}

/// Get the volume label.
pub fn volume_label() -> String {
    let lock = FAT32.lock();
    match lock.as_ref() {
        Some(s) => s.volume_label.clone(),
        None => String::from("(none)"),
    }
}

/// Check if FAT32 is mounted.
pub fn is_mounted() -> bool {
    FAT32.lock().is_some()
}

/// Get filesystem statistics.
pub fn stats() -> (u32, u32, u32) {
    let lock = FAT32.lock();
    let state = match lock.as_ref() {
        Some(s) => s,
        None => return (0, 0, 0),
    };

    let mut free_clusters = 0u32;
    let mut _used_clusters = 0u32;

    // Count free/used clusters (scan the FAT)
    for cluster in 2..state.total_clusters + 2 {
        let entry = fat_read(state, cluster);
        if entry == FAT_FREE {
            free_clusters += 1;
        } else {
            _used_clusters += 1;
        }
    }

    let cluster_size = state.sectors_per_cluster as u32 * state.bytes_per_sector as u32;
    (state.total_clusters, free_clusters, cluster_size)
}
