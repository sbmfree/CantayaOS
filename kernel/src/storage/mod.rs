// Storage Subsystem
//
// This module provides the storage stack for CantayaOS:
//   - Block device abstraction (trait-based)
//   - FAT32 filesystem driver (read-write)
//   - Virtual Filesystem (VFS) layer with path-based operations
//
// Architecture:
//
//   ┌────────────────────────────┐
//   │   Shell / Applications     │
//   ├────────────────────────────┤
//   │   VFS (path resolution)    │
//   ├────────────────────────────┤
//   │   FAT32 Driver             │
//   ├────────────────────────────┤
//   │   Block Device Abstraction │
//   ├────────────────────────────┤
//   │   virtio-blk HAL Driver    │
//   └────────────────────────────┘

pub mod block;
pub mod fat32;
pub mod vfs;

/// Initialize the storage subsystem.
///
/// Discovers and initializes disk devices, mounts the root filesystem.
pub fn init() {
    log::info!("storage: initializing storage subsystem...");

    // Step 1: Initialize the virtio-blk driver
    if !crate::hal::virtio_blk::init() {
        log::info!("storage: no block device available — filesystem disabled");
        return;
    }

    // Step 2: Initialize the block device wrapper
    block::init();

    // Step 3: Mount the FAT32 filesystem
    match fat32::init() {
        Ok(()) => {
            log::info!("storage: FAT32 filesystem mounted");
        }
        Err(e) => {
            log::error!("storage: FAT32 mount failed: {}", e);
            return;
        }
    }

    // Step 4: Initialize VFS with FAT32 as root
    vfs::init();
    log::info!("storage: VFS initialized — filesystem ready");
}
