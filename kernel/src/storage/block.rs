// Block Device Abstraction
//
// Provides a uniform interface for accessing block storage devices.
// Currently backed by virtio-blk, but designed to support multiple backends.

use crate::hal::virtio_blk;

/// Block size for our block devices (matches sector size)
pub const BLOCK_SIZE: usize = 512;

/// Initialize the block device layer.
pub fn init() {
    log::info!(
        "block: device ready, {} sectors ({} MiB)",
        virtio_blk::capacity_sectors(),
        virtio_blk::capacity_sectors() * 512 / (1024 * 1024)
    );
}

/// Read `count` blocks starting at `lba` into `buf`.
///
/// `buf` must be at least `count * BLOCK_SIZE` bytes.
pub fn read_blocks(lba: u64, count: usize, buf: &mut [u8]) -> bool {
    virtio_blk::read_sectors(lba, count, buf)
}

/// Write `count` blocks starting at `lba` from `buf`.
///
/// `buf` must be at least `count * BLOCK_SIZE` bytes.
pub fn write_blocks(lba: u64, count: usize, buf: &[u8]) -> bool {
    virtio_blk::write_sectors(lba, count, buf)
}

/// Get total device capacity in blocks.
pub fn capacity() -> u64 {
    virtio_blk::capacity_sectors()
}

/// Read a single block.
pub fn read_block(lba: u64, buf: &mut [u8; BLOCK_SIZE]) -> bool {
    read_blocks(lba, 1, buf)
}

/// Write a single block.
pub fn write_block(lba: u64, buf: &[u8; BLOCK_SIZE]) -> bool {
    write_blocks(lba, 1, buf)
}
