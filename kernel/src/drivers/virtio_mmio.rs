//! Virtio MMIO transport layer (v2)
//!
//! On QEMU virt, virtio-mmio transports are at base addresses
//! 0x0A00_0000 + n * 0x200 with SPI IRQ 48 + n.
//! Each device occupies a 0x200-byte register region.

use core::ptr;
use crate::sync::IrqMutex;
use crate::mm::physical;

extern crate alloc;
use alloc::vec::Vec;

// ---------------------------------------------------------------------------
// MMIO register offsets (virtio-mmio v2)
// ---------------------------------------------------------------------------
// MMIO register offsets — legacy (v1) and modern (v2) combined
// ---------------------------------------------------------------------------
const MAGIC:            usize = 0x000;
const VERSION:          usize = 0x004;
const DEVICE_ID:        usize = 0x008;
#[allow(dead_code)]
const VENDOR_ID:        usize = 0x00C;
const DEVICE_FEATURES:  usize = 0x010;
const DEVICE_FEATURES_SEL: usize = 0x014;
const DRIVER_FEATURES:  usize = 0x020;
const DRIVER_FEATURES_SEL: usize = 0x024;

// Legacy-only registers
const GUEST_PAGE_SIZE:  usize = 0x028; // v1 only
const QUEUE_SEL:        usize = 0x030;
const QUEUE_NUM_MAX:    usize = 0x034;
const QUEUE_NUM:        usize = 0x038;
#[allow(dead_code)]
const QUEUE_ALIGN:      usize = 0x03C; // v1 only
const QUEUE_PFN:        usize = 0x040; // v1 only
const QUEUE_NOTIFY:     usize = 0x050;
const INTERRUPT_STATUS: usize = 0x060;
const INTERRUPT_ACK:    usize = 0x064;
const STATUS:           usize = 0x070;

// Modern (v2) only registers
const QUEUE_READY:      usize = 0x044;
const QUEUE_DESC_LOW:   usize = 0x080;
const QUEUE_DESC_HIGH:  usize = 0x084;
const QUEUE_DRIVER_LOW: usize = 0x090;
const QUEUE_DRIVER_HIGH:usize = 0x094;
const QUEUE_DEVICE_LOW: usize = 0x0A0;
const QUEUE_DEVICE_HIGH:usize = 0x0A4;
const CONFIG:           usize = 0x100;

// Status bits
const STATUS_ACK:         u32 = 1;
const STATUS_DRIVER:      u32 = 2;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_DRIVER_OK:   u32 = 4;
const STATUS_FAILED:      u32 = 128;

// Virtqueue descriptor flags
pub const VRING_DESC_F_NEXT:  u16 = 1;
pub const VRING_DESC_F_WRITE: u16 = 2;

// Expected magic
const VIRTIO_MAGIC: u32 = 0x7472_6976; // "virt"

/// Virtio device IDs we care about
pub const VIRTIO_DEV_INPUT: u32 = 18;

// MMIO transport slots
const VIRTIO_MMIO_BASE: usize = 0x0A00_0000;
const VIRTIO_MMIO_STRIDE: usize = 0x200;
const MAX_SLOTS: usize = 32;

// ---------------------------------------------------------------------------
// Virtqueue structures (split virtqueue)
// ---------------------------------------------------------------------------

/// Virtqueue descriptor (16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VringDesc {
    pub addr:  u64,
    pub len:   u32,
    pub flags: u16,
    pub next:  u16,
}

/// Available ring header.
#[repr(C)]
pub struct VringAvail {
    pub flags: u16,
    pub idx:   u16,
    // pub ring: [u16; queue_size], -- follows in memory
}

/// Used ring element.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VringUsedElem {
    pub id:  u32,
    pub len: u32,
}

/// Used ring header.
#[repr(C)]
pub struct VringUsed {
    pub flags: u16,
    pub idx:   u16,
    // pub ring: [VringUsedElem; queue_size], -- follows in memory
}

/// Runtime state for one virtqueue.
pub struct Virtqueue {
    pub queue_size: u16,
    pub desc_phys: usize,       // physical address of descriptor table
    pub avail_phys: usize,      // physical address of available ring
    pub used_phys: usize,       // physical address of used ring
    pub last_used_idx: u16,     // last processed used ring index
    pub free_head: u16,         // head of free descriptor chain
    pub num_free: u16,          // number of free descriptors
    base: usize,                // MMIO base of the parent device
    queue_idx: u16,             // which queue index within the device
}

impl Virtqueue {
    /// Allocate and initialise a split virtqueue for the device at `base`,
    /// queue index `qi`. Handles both legacy (v1) and modern (v2) transports.
    pub fn new(base: usize, qi: u16) -> Option<Self> {
        let version = unsafe { read32(base + VERSION) };
        unsafe {
            // Select queue
            write32(base + QUEUE_SEL, qi as u32);

            // Read max queue size
            let max = read32(base + QUEUE_NUM_MAX) as u16;
            if max == 0 { return None; }
            let sz = max.min(256); // cap to reasonable size

            // Set our queue size
            write32(base + QUEUE_NUM, sz as u32);

            let desc_phys;
            let avail_phys;
            let used_phys;

            if version == 1 {
                // --- Legacy (v1) layout ---
                // All three regions are allocated contiguously.
                // Layout: descriptors | avail ring | padding | used ring
                // Avail ring starts right after desc table.
                // Used ring starts at next 4K-aligned offset after avail ring.
                let page_size: usize = 0x1000;
                write32(base + GUEST_PAGE_SIZE, page_size as u32);

                let desc_size = sz as usize * 16;
                let avail_size = 6 + 2 * sz as usize;
                // used ring is at the next page boundary after desc+avail
                let used_offset = align_up(desc_size + avail_size, page_size);
                let used_size = 6 + 8 * sz as usize;
                let total = used_offset + used_size;
                let total_pages = (total + page_size - 1) / page_size;

                let ring_phys = physical::alloc_contiguous_frames(total_pages)?;
                ptr::write_bytes(ring_phys as *mut u8, 0, total_pages * page_size);

                desc_phys = ring_phys;
                avail_phys = ring_phys + desc_size;
                used_phys = ring_phys + used_offset;

                // Tell device the PFN (page frame number) of the ring
                write32(base + QUEUE_PFN, (ring_phys / page_size) as u32);
            } else {
                // --- Modern (v2) layout ---
                // Allocate descriptor table: sz * 16 bytes, page-aligned
                let desc_bytes = (sz as usize) * 16;
                let desc_pages = (desc_bytes + 0xFFF) / 0x1000;
                desc_phys = physical::alloc_contiguous_frames(desc_pages)?;
                ptr::write_bytes(desc_phys as *mut u8, 0, desc_pages * 0x1000);

                // Allocate available ring: 6 + 2 * sz bytes
                let avail_bytes = 6 + 2 * (sz as usize);
                let avail_pages = (avail_bytes + 0xFFF) / 0x1000;
                avail_phys = physical::alloc_contiguous_frames(avail_pages)?;
                ptr::write_bytes(avail_phys as *mut u8, 0, avail_pages * 0x1000);

                // Allocate used ring: 6 + 8 * sz bytes
                let used_bytes = 6 + 8 * (sz as usize);
                let used_pages = (used_bytes + 0xFFF) / 0x1000;
                used_phys = physical::alloc_contiguous_frames(used_pages)?;
                ptr::write_bytes(used_phys as *mut u8, 0, used_pages * 0x1000);

                // Write addresses to MMIO
                write32(base + QUEUE_DESC_LOW,  desc_phys as u32);
                write32(base + QUEUE_DESC_HIGH, (desc_phys >> 32) as u32);
                write32(base + QUEUE_DRIVER_LOW,  avail_phys as u32);
                write32(base + QUEUE_DRIVER_HIGH, (avail_phys >> 32) as u32);
                write32(base + QUEUE_DEVICE_LOW,  used_phys as u32);
                write32(base + QUEUE_DEVICE_HIGH, (used_phys >> 32) as u32);

                write32(base + QUEUE_READY, 1);
            }

            // Build free descriptor chain
            for i in 0..sz {
                let desc = (desc_phys as *mut VringDesc).add(i as usize);
                (*desc).next = if i + 1 < sz { i + 1 } else { 0 };
                (*desc).flags = VRING_DESC_F_NEXT;
            }

            Some(Virtqueue {
                queue_size: sz,
                desc_phys,
                avail_phys,
                used_phys,
                last_used_idx: 0,
                free_head: 0,
                num_free: sz,
                base,
                queue_idx: qi,
            })
        }
    }

    /// Allocate a single descriptor from the free chain.
    pub fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 { return None; }
        let idx = self.free_head;
        unsafe {
            let desc = (self.desc_phys as *mut VringDesc).add(idx as usize);
            self.free_head = (*desc).next;
        }
        self.num_free -= 1;
        Some(idx)
    }

    /// Free a descriptor back to the chain.
    pub fn free_desc(&mut self, idx: u16) {
        unsafe {
            let desc = (self.desc_phys as *mut VringDesc).add(idx as usize);
            (*desc).next = self.free_head;
            (*desc).flags = VRING_DESC_F_NEXT;
        }
        self.free_head = idx;
        self.num_free += 1;
    }

    /// Submit a single buffer (one descriptor) to the device.
    pub fn submit_buf(&mut self, buf_phys: usize, len: u32, device_writable: bool) -> Option<u16> {
        let di = self.alloc_desc()?;
        unsafe {
            let desc = (self.desc_phys as *mut VringDesc).add(di as usize);
            (*desc).addr = buf_phys as u64;
            (*desc).len = len;
            (*desc).flags = if device_writable { VRING_DESC_F_WRITE } else { 0 };
            (*desc).next = 0;

            // Add to available ring
            let avail = self.avail_phys as *mut VringAvail;
            let avail_idx = ptr::read_volatile(&(*avail).idx);
            let ring = (self.avail_phys + 4) as *mut u16; // ring starts at offset 4
            ptr::write_volatile(ring.add((avail_idx % self.queue_size) as usize), di);
            // Memory barrier
            core::sync::atomic::fence(core::sync::atomic::Ordering::Release);
            ptr::write_volatile(&mut (*avail).idx, avail_idx.wrapping_add(1));
        }
        Some(di)
    }

    /// Notify the device that we've added buffers.
    pub fn notify(&self) {
        unsafe {
            // Memory barrier before notify
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            write32(self.base + QUEUE_NOTIFY, self.queue_idx as u32);
        }
    }

    /// Process used buffers. Returns an iterator of (descriptor_index, bytes_written).
    pub fn poll_used(&mut self) -> Vec<(u16, u32)> {
        let mut results = Vec::new();
        unsafe {
            let used = self.used_phys as *mut VringUsed;
            let used_idx = ptr::read_volatile(&(*used).idx);
            while self.last_used_idx != used_idx {
                let elem_ptr = (self.used_phys + 4) as *const VringUsedElem;
                let elem = ptr::read_volatile(
                    elem_ptr.add((self.last_used_idx % self.queue_size) as usize)
                );
                results.push((elem.id as u16, elem.len));
                self.last_used_idx = self.last_used_idx.wrapping_add(1);
            }
        }
        results
    }
}

// ---------------------------------------------------------------------------
// Per-slot device info
// ---------------------------------------------------------------------------

/// Discovered virtio-mmio device.
pub struct VirtioMmioDevice {
    pub base: usize,
    pub device_id: u32,
    pub slot: usize,
    pub irq: u32,
}

static DISCOVERED: IrqMutex<Vec<VirtioMmioDevice>> = IrqMutex::new(Vec::new());

/// Probe all 32 slots and return discovered devices.
pub fn probe() -> Vec<(usize, u32, u32)> {
    let mut found = Vec::new();
    for slot in 0..MAX_SLOTS {
        let base = VIRTIO_MMIO_BASE + slot * VIRTIO_MMIO_STRIDE;
        unsafe {
            let magic = read32(base + MAGIC);
            if magic != VIRTIO_MAGIC { continue; }
            let version = read32(base + VERSION);
            if version < 1 { continue; }
            let dev_id = read32(base + DEVICE_ID);
            if dev_id == 0 { continue; }

            let irq = 48 + slot as u32;
            found.push((base, dev_id, irq));

            DISCOVERED.lock().push(VirtioMmioDevice {
                base,
                device_id: dev_id,
                slot,
                irq,
            });
        }
    }
    found
}

/// Perform the standard virtio device initialisation sequence.
/// After this, the caller should set up virtqueues and finish with `driver_ok()`.
pub fn init_device(base: usize, wanted_features: u32) -> bool {
    let version = unsafe { read32(base + VERSION) };
    unsafe {
        // 1. Reset
        write32(base + STATUS, 0);

        // 2. Acknowledge
        let mut status = STATUS_ACK;
        write32(base + STATUS, status);

        // 3. Driver
        status |= STATUS_DRIVER;
        write32(base + STATUS, status);

        if version >= 2 {
            // Modern path: use feature selection registers
            write32(base + DEVICE_FEATURES_SEL, 0);
            let _dev_features = read32(base + DEVICE_FEATURES);
            write32(base + DRIVER_FEATURES_SEL, 0);
            write32(base + DRIVER_FEATURES, wanted_features);

            // Features OK
            status |= STATUS_FEATURES_OK;
            write32(base + STATUS, status);

            let s = read32(base + STATUS);
            if s & STATUS_FEATURES_OK == 0 {
                write32(base + STATUS, STATUS_FAILED);
                return false;
            }
        } else {
            // Legacy path: no feature sel registers, directly read/write features
            let _dev_features = read32(base + DEVICE_FEATURES);
            write32(base + DRIVER_FEATURES, wanted_features);
            // Legacy has no FEATURES_OK step
        }
    }
    true
}

/// Finish device init — tell device we're ready.
pub fn driver_ok(base: usize) {
    unsafe {
        let s = read32(base + STATUS);
        write32(base + STATUS, s | STATUS_DRIVER_OK);
    }
}

/// Read a 32-bit value from device config space (offset relative to config base).
pub fn read_config_u8(base: usize, offset: usize) -> u8 {
    unsafe { ptr::read_volatile((base + CONFIG + offset) as *const u8) }
}

/// Acknowledge interrupt (clears INTERRUPT_STATUS).
pub fn ack_interrupt(base: usize) -> u32 {
    unsafe {
        let s = read32(base + INTERRUPT_STATUS);
        write32(base + INTERRUPT_ACK, s);
        s
    }
}

/// Handle an IRQ for slot `slot_index` — delegates to the appropriate driver.
pub fn handle_irq(slot_index: usize) {
    let base = VIRTIO_MMIO_BASE + slot_index * VIRTIO_MMIO_STRIDE;
    let dev_id = unsafe { read32(base + DEVICE_ID) };

    match dev_id {
        VIRTIO_DEV_INPUT => crate::drivers::virtio_input::handle_irq(base),
        _ => {
            ack_interrupt(base);
        }
    }
}

// ---------------------------------------------------------------------------
// MMIO helpers
// ---------------------------------------------------------------------------

#[inline]
unsafe fn read32(addr: usize) -> u32 {
    ptr::read_volatile(addr as *const u32)
}

#[inline]
unsafe fn write32(addr: usize, val: u32) {
    ptr::write_volatile(addr as *mut u32, val);
}

/// Align `val` up to `align` (must be power-of-two).
#[inline]
const fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}
