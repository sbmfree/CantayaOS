// Virtio Block Device Driver
//
// This module implements a virtio-blk driver using the legacy PCI transport.
// It provides sector-level read/write access to a virtual block device.
//
// The virtio-blk device uses a single virtqueue (queue 0) for I/O requests.
// Each request is a chain of 3 descriptors:
//   1. Request header (VirtioBlkReqHeader) — device-readable
//   2. Data buffer — device-readable (write) or device-writable (read)
//   3. Status byte — device-writable
//
// Device-specific config (at BAR0 + 20):
//   Offset 0: capacity (u64) — total number of 512-byte sectors
//   Offset 8: size_max (u32) — max segment size
//   Offset 12: seg_max (u32) — max number of segments
//
// Reference: virtio spec §5.2

use crate::hal::virtio::{self, Virtqueue, VRING_DESC_F_NEXT, VRING_DESC_F_WRITE};
use crate::memory::frame_allocator;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// Virtio-blk PCI device ID (legacy/transitional)
const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;

/// Block request types
const VIRTIO_BLK_T_IN: u32 = 0;   // Read
const VIRTIO_BLK_T_OUT: u32 = 1;  // Write

/// Block request status values (returned by device)
const VIRTIO_BLK_S_OK: u8 = 0;
const VIRTIO_BLK_S_IOERR: u8 = 1;
const VIRTIO_BLK_S_UNSUPP: u8 = 2;

/// Sector size in bytes
pub const SECTOR_SIZE: usize = 512;

/// Request header sent to the device
#[repr(C)]
struct VirtioBlkReqHeader {
    req_type: u32,
    _reserved: u32,
    sector: u64,
}

/// Global virtio-blk device instance
static VIRTIO_BLK: Mutex<Option<VirtioBlkDevice>> = Mutex::new(None);

/// Whether the device has been initialized
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// I/O base stored separately so the interrupt handler doesn't need the lock
static IRQ_IO_BASE: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);

/// Virtio block device state
struct VirtioBlkDevice {
    /// I/O port base (BAR0)
    io_base: u16,
    /// Virtqueue 0 (requestq)
    vq: Virtqueue,
    /// Total capacity in sectors
    capacity: u64,
    /// Physical address of the request buffer (header + status)
    /// We pre-allocate a page for request metadata to avoid per-request allocation.
    req_page_phys: u64,
}

/// Initialize the virtio-blk driver.
///
/// Discovers the device via PCI, negotiates features, sets up the virtqueue,
/// and reads the device capacity.
pub fn init() -> bool {
    // Find the virtio-blk device
    let dev = match virtio::find_device(VIRTIO_BLK_DEVICE_ID) {
        Some(d) => d,
        None => {
            log::info!("virtio-blk: no device found (vendor={:#X}, device={:#X})", virtio::VIRTIO_VENDOR_ID, VIRTIO_BLK_DEVICE_ID);
            return false;
        }
    };

    log::info!(
        "virtio-blk: found device at {:02X}:{:02X}.{} IRQ={}",
        dev.bus, dev.device, dev.function, dev.interrupt_line
    );

    // Get I/O port base from BAR0
    let io_base = match virtio::bar0_ioport(&dev) {
        Some(port) => port,
        None => {
            log::error!("virtio-blk: BAR0 is not I/O port");
            return false;
        }
    };

    log::info!("virtio-blk: I/O base = {:#X}", io_base);

    // Standard init sequence (accept no optional features for simplicity)
    let _features = virtio::init_device(io_base, 0);

    // Set up virtqueue 0
    let vq = match Virtqueue::new(io_base, 0) {
        Some(q) => q,
        None => {
            log::error!("virtio-blk: failed to initialize virtqueue 0");
            virtio::set_status(io_base, virtio::STATUS_FAILED);
            return false;
        }
    };

    // Mark device ready
    virtio::driver_ok(io_base);

    // Read capacity from device config
    let capacity = virtio::read_config64(io_base, 0);
    log::info!(
        "virtio-blk: capacity = {} sectors ({} MiB)",
        capacity,
        capacity * 512 / (1024 * 1024)
    );

    // Allocate a page for request metadata (header + status)
    let req_page_phys = match frame_allocator::allocate_frame() {
        Some(p) => p,
        None => {
            log::error!("virtio-blk: failed to allocate request page");
            return false;
        }
    };

    // Zero the request page
    unsafe {
        core::ptr::write_bytes(req_page_phys as *mut u8, 0, 4096);
    }

    // Enable the virtio IRQ in the PIC and register in the IDT
    // Legacy virtio uses the PCI interrupt line field
    crate::hal::idt::register_virtio_irq(dev.interrupt_line);
    enable_virtio_irq(dev.interrupt_line);

    // Store I/O base for interrupt handler (lock-free access)
    IRQ_IO_BASE.store(io_base, Ordering::Release);

    let device = VirtioBlkDevice {
        io_base,
        vq,
        capacity,
        req_page_phys,
    };

    *VIRTIO_BLK.lock() = Some(device);
    INITIALIZED.store(true, Ordering::Release);

    log::info!("virtio-blk: driver initialized successfully");
    true
}

/// Enable the PCI interrupt for the virtio device in the PIC.
fn enable_virtio_irq(irq_line: u8) {
    use crate::hal::port::{inb, outb};

    if irq_line >= 16 {
        log::warn!("virtio-blk: invalid IRQ line {}", irq_line);
        return;
    }

    unsafe {
        if irq_line < 8 {
            // Master PIC (IRQ 0-7)
            let mask = inb(0x21);
            outb(0x21, mask & !(1 << irq_line));
        } else {
            // Slave PIC (IRQ 8-15)
            let mask = inb(0xA1);
            outb(0xA1, mask & !(1 << (irq_line - 8)));
        }
    }

    log::info!("virtio-blk: unmasked IRQ {}", irq_line);
}

/// Check if the driver is initialized and a disk is available.
pub fn is_available() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Get disk capacity in sectors.
pub fn capacity_sectors() -> u64 {
    let lock = VIRTIO_BLK.lock();
    match lock.as_ref() {
        Some(dev) => dev.capacity,
        None => 0,
    }
}

/// Read sectors from disk into a buffer.
///
/// - `lba`: starting sector (logical block address)
/// - `count`: number of sectors to read
/// - `buf`: output buffer (must be at least count * 512 bytes)
///
/// Returns true on success, false on error.
pub fn read_sectors(lba: u64, count: usize, buf: &mut [u8]) -> bool {
    if !is_available() {
        return false;
    }
    if buf.len() < count * SECTOR_SIZE {
        return false;
    }

    let mut lock = VIRTIO_BLK.lock();
    let dev = match lock.as_mut() {
        Some(d) => d,
        None => return false,
    };

    if lba + count as u64 > dev.capacity {
        log::error!("virtio-blk: read beyond capacity (lba={}, count={}, cap={})", lba, count, dev.capacity);
        return false;
    }

    // We process one sector at a time if count > 1 to keep descriptor chains simple.
    // For bulk reads, the caller can batch.
    for i in 0..count {
        if !do_request(dev, VIRTIO_BLK_T_IN, lba + i as u64, &mut buf[i * SECTOR_SIZE..(i + 1) * SECTOR_SIZE]) {
            return false;
        }
    }

    true
}

/// Write sectors from a buffer to disk.
///
/// - `lba`: starting sector
/// - `count`: number of sectors to write
/// - `buf`: input buffer (must be at least count * 512 bytes)
///
/// Returns true on success, false on error.
pub fn write_sectors(lba: u64, count: usize, buf: &[u8]) -> bool {
    if !is_available() {
        return false;
    }
    if buf.len() < count * SECTOR_SIZE {
        return false;
    }

    let mut lock = VIRTIO_BLK.lock();
    let dev = match lock.as_mut() {
        Some(d) => d,
        None => return false,
    };

    if lba + count as u64 > dev.capacity {
        log::error!("virtio-blk: write beyond capacity (lba={}, count={}, cap={})", lba, count, dev.capacity);
        return false;
    }

    for i in 0..count {
        // We need a mutable slice for do_request's unified interface,
        // but for writes the data buffer is device-readable (not written to).
        // We copy the sector data into our request page's data area.
        let sector_data = &buf[i * SECTOR_SIZE..(i + 1) * SECTOR_SIZE];
        // Copy to a staging area in the request page
        let staging = unsafe {
            core::slice::from_raw_parts_mut(
                (dev.req_page_phys + 256) as *mut u8,
                SECTOR_SIZE,
            )
        };
        staging.copy_from_slice(sector_data);

        // Use staging as the "buffer" for the request
        if !do_request_raw(dev, VIRTIO_BLK_T_OUT, lba + i as u64) {
            return false;
        }
    }

    true
}

/// Execute a single block I/O request (read).
/// Uses a staging area in req_page_phys for DMA, then copies data to/from the user buffer.
fn do_request(dev: &mut VirtioBlkDevice, req_type: u32, sector: u64, buf: &mut [u8]) -> bool {
    let header_phys = dev.req_page_phys;
    let status_phys = dev.req_page_phys + 128; // Status byte at offset 128
    let data_phys = dev.req_page_phys + 512;   // Data staging area at offset 512

    // Write request header
    unsafe {
        let header = header_phys as *mut VirtioBlkReqHeader;
        (*header).req_type = req_type;
        (*header)._reserved = 0;
        (*header).sector = sector;

        // Clear status
        core::ptr::write_volatile(status_phys as *mut u8, 0xFF);
    }

    // Allocate 3 descriptors
    let d0 = match dev.vq.alloc_desc() { Some(d) => d, None => return false };
    let d1 = match dev.vq.alloc_desc() { Some(d) => d, None => { dev.vq.free_desc(d0); return false } };
    let d2 = match dev.vq.alloc_desc() { Some(d) => d, None => { dev.vq.free_desc(d0); dev.vq.free_desc(d1); return false } };

    // Set up descriptor chain
    unsafe {
        // Descriptor 0: request header (device-readable)
        let desc0 = dev.vq.desc_ptr(d0) as *mut virtio::VirtqDesc;
        (*desc0).addr = header_phys;
        (*desc0).len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;
        (*desc0).flags = VRING_DESC_F_NEXT;
        (*desc0).next = d1;

        // Descriptor 1: data buffer — use DMA-safe staging area
        let desc1 = dev.vq.desc_ptr(d1) as *mut virtio::VirtqDesc;
        (*desc1).addr = data_phys;
        (*desc1).len = SECTOR_SIZE as u32;
        (*desc1).flags = VRING_DESC_F_NEXT | VRING_DESC_F_WRITE; // Device writes to our buffer
        (*desc1).next = d2;

        // Descriptor 2: status byte (device-writable)
        let desc2 = dev.vq.desc_ptr(d2) as *mut virtio::VirtqDesc;
        (*desc2).addr = status_phys;
        (*desc2).len = 1;
        (*desc2).flags = VRING_DESC_F_WRITE;
        (*desc2).next = 0;
    }

    // Submit and wait
    dev.vq.submit(d0);
    let (_head, _len) = dev.vq.wait_used();

    // Check status
    let status = unsafe { core::ptr::read_volatile(status_phys as *const u8) };

    // Copy data from staging area to user buffer (for reads)
    if status == VIRTIO_BLK_S_OK {
        unsafe {
            let staging = core::slice::from_raw_parts(data_phys as *const u8, SECTOR_SIZE);
            buf[..SECTOR_SIZE].copy_from_slice(staging);
        }
    }

    // Free descriptors
    dev.vq.free_chain(d0);

    if status != VIRTIO_BLK_S_OK {
        log::error!("virtio-blk: request failed (type={}, sector={}, status={})", req_type, sector, status);
        return false;
    }

    true
}

/// Execute a single block I/O request (write) using the staging area.
fn do_request_raw(dev: &mut VirtioBlkDevice, req_type: u32, sector: u64) -> bool {
    let header_phys = dev.req_page_phys;
    let data_phys = dev.req_page_phys + 256; // Data staged here by write_sectors
    let status_phys = dev.req_page_phys + 128;

    // Write request header
    unsafe {
        let header = header_phys as *mut VirtioBlkReqHeader;
        (*header).req_type = req_type;
        (*header)._reserved = 0;
        (*header).sector = sector;

        core::ptr::write_volatile(status_phys as *mut u8, 0xFF);
    }

    let d0 = match dev.vq.alloc_desc() { Some(d) => d, None => return false };
    let d1 = match dev.vq.alloc_desc() { Some(d) => d, None => { dev.vq.free_desc(d0); return false } };
    let d2 = match dev.vq.alloc_desc() { Some(d) => d, None => { dev.vq.free_desc(d0); dev.vq.free_desc(d1); return false } };

    unsafe {
        // Descriptor 0: header (device-readable)
        let desc0 = dev.vq.desc_ptr(d0) as *mut virtio::VirtqDesc;
        (*desc0).addr = header_phys;
        (*desc0).len = core::mem::size_of::<VirtioBlkReqHeader>() as u32;
        (*desc0).flags = VRING_DESC_F_NEXT;
        (*desc0).next = d1;

        // Descriptor 1: data (device-readable for writes)
        let desc1 = dev.vq.desc_ptr(d1) as *mut virtio::VirtqDesc;
        (*desc1).addr = data_phys;
        (*desc1).len = SECTOR_SIZE as u32;
        (*desc1).flags = VRING_DESC_F_NEXT; // No WRITE flag — device reads this
        (*desc1).next = d2;

        // Descriptor 2: status (device-writable)
        let desc2 = dev.vq.desc_ptr(d2) as *mut virtio::VirtqDesc;
        (*desc2).addr = status_phys;
        (*desc2).len = 1;
        (*desc2).flags = VRING_DESC_F_WRITE;
        (*desc2).next = 0;
    }

    dev.vq.submit(d0);
    let (_head, _len) = dev.vq.wait_used();

    let status = unsafe { core::ptr::read_volatile(status_phys as *const u8) };
    dev.vq.free_chain(d0);

    if status != VIRTIO_BLK_S_OK {
        log::error!("virtio-blk: write failed (sector={}, status={})", sector, status);
        return false;
    }

    true
}

/// Handle a virtio interrupt (called from IDT handler).
/// Must NOT acquire VIRTIO_BLK lock — it may already be held by read/write_sectors.
pub fn handle_interrupt() {
    let io_base = IRQ_IO_BASE.load(Ordering::Acquire);
    if io_base != 0 {
        // Read and clear the ISR status register
        let _isr = virtio::read_isr(io_base);
        // Signal that an interrupt occurred
        virtio::VIRTIO_IRQ_FIRED.store(true, Ordering::Release);
    }
}
