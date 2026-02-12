//! Virtio-blk driver (device ID 2)
//!
//! Provides sector-level read/write to a virtual block device via virtio-mmio.
//! Uses a single request virtqueue with 3-descriptor chains:
//!   descriptor 0: VirtioBlkReqHeader (device-readable)
//!   descriptor 1: data buffer (readable for write, writable for read)
//!   descriptor 2: 1-byte status (device-writable)

use crate::drivers::virtio_mmio::{self, Virtqueue, VIRTIO_DEV_BLK};
use crate::mm::physical;
use crate::sync::IrqMutex;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};

extern crate alloc;

/// Sector size in bytes
pub const SECTOR_SIZE: usize = 512;

/// virtio-blk request types
const VIRTIO_BLK_T_IN:  u32 = 0; // read
const VIRTIO_BLK_T_OUT: u32 = 1; // write

/// virtio-blk request header (16 bytes)
#[repr(C)]
#[derive(Clone, Copy)]
struct VirtioBlkReqHeader {
    req_type: u32,
    reserved: u32,
    sector:   u64,
}

/// Driver state
struct BlkDevice {
    #[allow(dead_code)]
    base: usize,
    queue: Virtqueue,
    capacity: u64, // in sectors
}

static BLK_DEV: IrqMutex<Option<BlkDevice>> = IrqMutex::new(None);

/// Set when an IRQ signals request completion.
static REQUEST_DONE: AtomicBool = AtomicBool::new(false);

/// Initialise the virtio-blk device at `base`.
pub fn init(base: usize) -> bool {
    crate::kprintln!("[virtio-blk] Initialising at {:#x}", base);

    // Feature bit 1 = VIRTIO_BLK_F_SIZE_MAX (we don't need any special features)
    if !virtio_mmio::init_device(base, 0) {
        crate::kprintln!("[virtio-blk] Feature negotiation failed");
        return false;
    }

    // Read capacity from config space (8 bytes at offset 0, little-endian)
    let cap_lo = virtio_mmio::read_config_u32(base, 0) as u64;
    let cap_hi = virtio_mmio::read_config_u32(base, 4) as u64;
    let capacity = cap_lo | (cap_hi << 32);
    crate::kprintln!("[virtio-blk] Capacity: {} sectors ({} MB)",
        capacity, capacity * SECTOR_SIZE as u64 / 1024 / 1024);

    // Set up virtqueue 0 (request queue)
    let queue = match Virtqueue::new(base, 0) {
        Some(q) => q,
        None => {
            crate::kprintln!("[virtio-blk] Failed to create request queue");
            return false;
        }
    };

    virtio_mmio::driver_ok(base);

    *BLK_DEV.lock() = Some(BlkDevice {
        base,
        queue,
        capacity,
    });

    crate::kprintln!("[virtio-blk] Device ready");
    true
}

/// Handle virtio-blk IRQ — signal completion.
pub fn handle_irq(base: usize) {
    virtio_mmio::ack_interrupt(base);
    REQUEST_DONE.store(true, Ordering::Release);
}

/// Read a single sector. `buf` must be exactly 512 bytes.
pub fn read_sector(sector: u64, buf: &mut [u8; SECTOR_SIZE]) -> bool {
    do_request(VIRTIO_BLK_T_IN, sector, buf)
}

/// Write a single sector. `data` must be exactly 512 bytes.
pub fn write_sector(sector: u64, data: &[u8; SECTOR_SIZE]) -> bool {
    // We need a mutable copy for the interface but for write
    // the device only reads from it.
    let mut tmp = *data;
    do_request(VIRTIO_BLK_T_OUT, sector, &mut tmp)
}

/// Get the disk capacity in sectors.
pub fn capacity() -> u64 {
    BLK_DEV.lock().as_ref().map(|d| d.capacity).unwrap_or(0)
}

/// Check if a block device is present.
pub fn is_available() -> bool {
    BLK_DEV.lock().is_some()
}

/// Perform a read or write request.
fn do_request(req_type: u32, sector: u64, buf: &mut [u8; SECTOR_SIZE]) -> bool {
    // Allocate DMA buffers (visible to device via identity mapping)
    let header_phys = match physical::alloc_frame() {
        Some(f) => f,
        None => return false,
    };
    let data_phys = match physical::alloc_frame() {
        Some(f) => f,
        None => return false,
    };
    let status_phys = match physical::alloc_frame() {
        Some(f) => f,
        None => return false,
    };

    // Write request header
    unsafe {
        let hdr = header_phys as *mut VirtioBlkReqHeader;
        ptr::write(hdr, VirtioBlkReqHeader {
            req_type,
            reserved: 0,
            sector,
        });
    }

    // For write requests, copy data to the DMA buffer
    if req_type == VIRTIO_BLK_T_OUT {
        unsafe {
            ptr::copy_nonoverlapping(buf.as_ptr(), data_phys as *mut u8, SECTOR_SIZE);
        }
    } else {
        unsafe {
            ptr::write_bytes(data_phys as *mut u8, 0, SECTOR_SIZE);
        }
    }

    // Clear status byte
    unsafe { *(status_phys as *mut u8) = 0xFF; }

    // Build the 3-descriptor chain
    let data_writable = req_type == VIRTIO_BLK_T_IN; // device writes for reads
    let chain = [
        (header_phys, 16u32, false),                        // header (device-readable)
        (data_phys, SECTOR_SIZE as u32, data_writable),     // data
        (status_phys, 1u32, true),                          // status (device-writable)
    ];

    REQUEST_DONE.store(false, Ordering::Release);

    let head = {
        let mut dev = BLK_DEV.lock();
        let dev = match dev.as_mut() {
            Some(d) => d,
            None => return false,
        };
        let head = match dev.queue.submit_chain(&chain) {
            Some(h) => h,
            None => return false,
        };
        dev.queue.notify();
        head
    };

    // Poll for completion (busy-wait with yield)
    let start = crate::hal::timer::uptime_ms();
    loop {
        if REQUEST_DONE.load(Ordering::Acquire) {
            break;
        }
        if crate::hal::timer::uptime_ms() - start > 3000 {
            crate::kprintln!("[virtio-blk] Request timeout!");
            return false;
        }
        crate::process::scheduler::yield_thread();
    }

    // Process used ring
    {
        let mut dev = BLK_DEV.lock();
        let dev = match dev.as_mut() {
            Some(d) => d,
            None => return false,
        };
        let _used = dev.queue.poll_used();
        dev.queue.free_chain(head);
    }

    // Check status byte
    let status = unsafe { *(status_phys as *const u8) };
    if status != 0 {
        crate::kprintln!("[virtio-blk] Request failed, status={}", status);
        return false;
    }

    // For read requests, copy data back from DMA buffer
    if req_type == VIRTIO_BLK_T_IN {
        unsafe {
            ptr::copy_nonoverlapping(data_phys as *const u8, buf.as_mut_ptr(), SECTOR_SIZE);
        }
    }

    true
}

/// Probe all discovered virtio-mmio devices and initialise any blk devices found.
pub fn probe_and_init() {
    let devices = crate::drivers::virtio_mmio::discovered_devices();
    for (base, dev_id, irq) in devices {
        if dev_id == VIRTIO_DEV_BLK {
            if init(base) {
                // Enable this device's IRQ in the GIC
                crate::hal::interrupts::enable_irq(irq);
                crate::kprintln!("[virtio-blk] Enabled IRQ {}", irq);
            }
        }
    }
}
