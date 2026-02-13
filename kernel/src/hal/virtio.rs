// Virtio PCI Transport — Legacy (Transitional) Interface
//
// This module implements the legacy virtio PCI transport as defined in the
// virtio specification. The legacy interface uses I/O port (PIO) access
// through PCI BAR0 for device configuration and virtqueue management.
//
// Legacy Register Layout (BAR0, I/O space):
//   Offset 0:  [R]   Device Features (32-bit)
//   Offset 4:  [W]   Guest Features (32-bit)
//   Offset 8:  [W]   Queue Address (PFN, guest-physical, page-aligned)
//   Offset 12: [R]   Queue Size (max descriptors)
//   Offset 14: [W]   Queue Select (choose which virtqueue)
//   Offset 16: [W]   Queue Notify (kick the device)
//   Offset 18: [R/W] Device Status
//   Offset 19: [R]   ISR Status (read clears)
//   Offset 20+:       Device-specific configuration
//
// Virtqueue Memory Layout (contiguous physical memory):
//   - Descriptor Table: 16 bytes per entry × queue_size
//   - Available Ring: 2 + 2 + 2*queue_size + 2 bytes
//   - [padding to next page boundary]
//   - Used Ring: 2 + 2 + 8*queue_size + 2 bytes
//
// Reference: https://docs.oasis-open.org/virtio/virtio/v1.1/virtio-v1.1.html

use crate::hal::port::{inb, outb, inw, outw, ind, outd};
use crate::hal::pci;
use crate::memory::frame_allocator;
use core::sync::atomic::{AtomicBool, Ordering};

/// Virtio PCI vendor ID
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

/// Legacy virtio register offsets from BAR0
const REG_DEVICE_FEATURES: u16 = 0;
const REG_GUEST_FEATURES: u16 = 4;
const REG_QUEUE_ADDRESS: u16 = 8;
const REG_QUEUE_SIZE: u16 = 12;
const REG_QUEUE_SELECT: u16 = 14;
const REG_QUEUE_NOTIFY: u16 = 16;
const REG_DEVICE_STATUS: u16 = 18;
const REG_ISR_STATUS: u16 = 19;
pub const REG_DEVICE_CONFIG: u16 = 20;

/// Device status flags
pub const STATUS_ACKNOWLEDGE: u8 = 1;
pub const STATUS_DRIVER: u8 = 2;
pub const STATUS_DRIVER_OK: u8 = 4;
pub const STATUS_FEATURES_OK: u8 = 8;
pub const STATUS_FAILED: u8 = 128;

/// Virtqueue descriptor flags
pub const VRING_DESC_F_NEXT: u16 = 1;     // Buffer continues in next descriptor
pub const VRING_DESC_F_WRITE: u16 = 2;    // Buffer is write-only (device → driver)

/// Page size for legacy virtio queue address
const VIRTIO_PCI_VRING_ALIGN: usize = 4096;

/// A virtio descriptor (16 bytes)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VirtqDesc {
    pub addr: u64,     // Physical address of the buffer
    pub len: u32,      // Length of the buffer
    pub flags: u16,    // VRING_DESC_F_* flags
    pub next: u16,     // Index of next descriptor (if NEXT flag set)
}

/// Available ring header
#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    // Followed by ring[queue_size] entries (u16 each)
    // and optionally used_event (u16)
}

/// Used ring header
#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    // Followed by ring[queue_size] VirtqUsedElem entries
}

/// Used ring element
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,       // Index of the descriptor chain head
    pub len: u32,      // Total bytes written by device
}

/// Virtqueue — manages a single virtio descriptor ring
pub struct Virtqueue {
    /// Base I/O port of the virtio device (BAR0)
    pub io_base: u16,
    /// Queue index (0 for requestq in virtio-blk)
    pub queue_idx: u16,
    /// Number of descriptors in the queue
    pub queue_size: u16,
    /// Physical address of the descriptor table
    pub desc_phys: u64,
    /// Physical address of the available ring
    pub avail_phys: u64,
    /// Physical address of the used ring
    pub used_phys: u64,
    /// Index into the free descriptor list
    free_head: u16,
    /// Number of free descriptors
    num_free: u16,
    /// Last seen used index (for polling)
    last_used_idx: u16,
}

/// Completion flag — set by the IRQ handler when an interrupt fires
pub static VIRTIO_IRQ_FIRED: AtomicBool = AtomicBool::new(false);

impl Virtqueue {
    /// Initialize a virtqueue for a given device.
    ///
    /// Allocates contiguous physical memory for the descriptor table,
    /// available ring, and used ring, then writes the queue address to the device.
    pub fn new(io_base: u16, queue_idx: u16) -> Option<Self> {
        unsafe {
            // Select the queue
            outw(io_base + REG_QUEUE_SELECT, queue_idx);

            // Read queue size (max descriptors)
            let queue_size = inw(io_base + REG_QUEUE_SIZE);
            if queue_size == 0 {
                log::error!("virtio: queue {} has size 0", queue_idx);
                return None;
            }

            log::info!("virtio: queue {} size = {}", queue_idx, queue_size);

            // Calculate memory layout sizes
            let desc_size = (queue_size as usize) * core::mem::size_of::<VirtqDesc>();
            let avail_size = 6 + 2 * (queue_size as usize); // flags(2) + idx(2) + ring(2*N) + used_event(2)
            let used_size = 6 + 8 * (queue_size as usize);  // flags(2) + idx(2) + ring(8*N) + avail_event(2)

            // Descriptor table + available ring need to be in the same page-aligned region
            let first_part = desc_size + avail_size;
            let first_part_aligned = (first_part + VIRTIO_PCI_VRING_ALIGN - 1) & !(VIRTIO_PCI_VRING_ALIGN - 1);
            let total_size = first_part_aligned + used_size;
            let total_pages = (total_size + 4095) / 4096;

            // Allocate contiguous physical memory
            let phys = frame_allocator::allocate_contiguous_frames(total_pages)?;

            // Zero the allocated memory
            let ptr = phys as *mut u8;
            core::ptr::write_bytes(ptr, 0, total_pages * 4096);

            let desc_phys = phys;
            let avail_phys = phys + desc_size as u64;
            let used_phys = phys + first_part_aligned as u64;

            // Initialize the free descriptor chain
            let desc_table = desc_phys as *mut VirtqDesc;
            for i in 0..(queue_size as usize) {
                let desc = &mut *desc_table.add(i);
                desc.next = if i + 1 < queue_size as usize { (i + 1) as u16 } else { 0 };
            }

            // Write queue address to device (PFN = physical address / 4096)
            outd(io_base + REG_QUEUE_ADDRESS, (phys / 4096) as u32);

            log::info!("virtio: queue {} at phys {:#X} ({} pages)", queue_idx, phys, total_pages);

            Some(Virtqueue {
                io_base,
                queue_idx,
                queue_size,
                desc_phys,
                avail_phys,
                used_phys,
                free_head: 0,
                num_free: queue_size,
                last_used_idx: 0,
            })
        }
    }

    /// Allocate a descriptor from the free list, returning its index.
    pub fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }
        let idx = self.free_head;
        let desc = self.desc_ptr(idx);
        unsafe {
            self.free_head = (*desc).next;
        }
        self.num_free -= 1;
        Some(idx)
    }

    /// Free a descriptor back to the free list.
    pub fn free_desc(&mut self, idx: u16) {
        let desc = self.desc_ptr(idx);
        unsafe {
            (*desc).next = self.free_head;
            (*desc).flags = 0;
        }
        self.free_head = idx;
        self.num_free += 1;
    }

    /// Free an entire descriptor chain starting from `head`.
    pub fn free_chain(&mut self, head: u16) {
        let mut idx = head;
        loop {
            let desc = self.desc_ptr(idx);
            let flags;
            let next;
            unsafe {
                flags = (*desc).flags;
                next = (*desc).next;
            }
            self.free_desc(idx);
            if flags & VRING_DESC_F_NEXT == 0 {
                break;
            }
            idx = next;
        }
    }

    /// Get a raw pointer to a descriptor by index.
    pub fn desc_ptr(&self, idx: u16) -> *mut VirtqDesc {
        unsafe { (self.desc_phys as *mut VirtqDesc).add(idx as usize) }
    }

    /// Submit a descriptor chain (given by the head index) to the available ring
    /// and notify the device.
    pub fn submit(&mut self, head: u16) {
        unsafe {
            let avail = self.avail_phys as *mut VirtqAvail;
            let avail_idx = core::ptr::read_volatile(&(*avail).idx);
            let ring_slot = avail_idx % self.queue_size;

            // Write the head descriptor index into the ring
            let ring_base = (self.avail_phys + 4) as *mut u16;
            core::ptr::write_volatile(ring_base.add(ring_slot as usize), head);

            // Memory barrier — ensure descriptor writes are visible before updating idx
            core::sync::atomic::fence(Ordering::Release);

            // Increment the available index
            core::ptr::write_volatile(&mut (*avail).idx, avail_idx.wrapping_add(1));

            // Memory barrier before notify
            core::sync::atomic::fence(Ordering::SeqCst);

            // Notify the device
            outw(self.io_base + REG_QUEUE_NOTIFY, self.queue_idx);
        }
    }

    /// Poll the used ring for completed requests.
    /// Returns Some((desc_head, bytes_written)) if a request completed.
    pub fn poll_used(&mut self) -> Option<(u16, u32)> {
        unsafe {
            let used = self.used_phys as *mut VirtqUsed;
            core::sync::atomic::fence(Ordering::Acquire);
            let used_idx = core::ptr::read_volatile(&(*used).idx);

            if self.last_used_idx == used_idx {
                return None; // Nothing new
            }

            let ring_slot = self.last_used_idx % self.queue_size;
            let elem_ptr = (self.used_phys + 4) as *const VirtqUsedElem;
            let elem = core::ptr::read_volatile(elem_ptr.add(ring_slot as usize));

            self.last_used_idx = self.last_used_idx.wrapping_add(1);

            Some((elem.id as u16, elem.len))
        }
    }

    /// Wait for the device to consume a request (busy-poll).
    /// Returns (desc_head, bytes_written).
    pub fn wait_used(&mut self) -> (u16, u32) {
        loop {
            if let Some(result) = self.poll_used() {
                return result;
            }
            // Yield a tiny bit — hint to the CPU
            core::hint::spin_loop();
        }
    }
}

// ============================================================================
// Device Discovery & Initialization Helpers
// ============================================================================

/// Find a virtio PCI device by its subsystem/device ID.
///
/// Legacy virtio devices have vendor 0x1AF4 and device IDs 0x1000-0x103F.
/// - 0x1001 = network (transitional)
/// - 0x1001 = block (transitional)  
/// Wait, legacy PCI device IDs:
///   0x1000 = network
///   0x1001 = block
///   0x1002 = memory balloon
///   0x1003 = console
///   0x1005 = entropy
///
/// Modern virtio-pci devices use IDs 0x1040+subsystem_id.
pub fn find_device(target_device_id: u16) -> Option<pci::PciDevice> {
    let devices = pci::device_list();
    for dev in &devices {
        if dev.vendor_id == VIRTIO_VENDOR_ID && dev.device_id == target_device_id {
            return Some(*dev);
        }
    }
    None
}

/// Read the I/O port base address from PCI BAR0 for a virtio device.
pub fn bar0_ioport(dev: &pci::PciDevice) -> Option<u16> {
    let bar0 = dev.bars[0];
    // Legacy virtio uses I/O port BAR (bit 0 = 1 means I/O)
    if bar0 & 1 == 0 {
        log::warn!("virtio: BAR0 is MMIO, expected I/O port");
        return None;
    }
    Some((bar0 & 0xFFFFFFFC) as u16)
}

/// Reset a virtio device by writing 0 to the status register.
pub fn reset(io_base: u16) {
    unsafe {
        outb(io_base + REG_DEVICE_STATUS, 0);
    }
}

/// Read the current device status.
pub fn read_status(io_base: u16) -> u8 {
    unsafe { inb(io_base + REG_DEVICE_STATUS) }
}

/// Set device status bits (OR with current).
pub fn set_status(io_base: u16, status: u8) {
    let current = read_status(io_base);
    unsafe {
        outb(io_base + REG_DEVICE_STATUS, current | status);
    }
}

/// Read device-offered features.
pub fn read_device_features(io_base: u16) -> u32 {
    unsafe { ind(io_base + REG_DEVICE_FEATURES) }
}

/// Write the set of features the guest/driver accepts.
pub fn write_guest_features(io_base: u16, features: u32) {
    unsafe { outd(io_base + REG_GUEST_FEATURES, features); }
}

/// Read and clear the ISR status register.
pub fn read_isr(io_base: u16) -> u8 {
    unsafe { inb(io_base + REG_ISR_STATUS) }
}

/// Read a 32-bit value from device-specific configuration (offset from REG_DEVICE_CONFIG).
pub fn read_config32(io_base: u16, offset: u16) -> u32 {
    unsafe { ind(io_base + REG_DEVICE_CONFIG + offset) }
}

/// Read a 64-bit value from device-specific configuration (two 32-bit reads).
pub fn read_config64(io_base: u16, offset: u16) -> u64 {
    let lo = read_config32(io_base, offset) as u64;
    let hi = read_config32(io_base, offset + 4) as u64;
    lo | (hi << 32)
}

/// Standard virtio device initialization sequence (legacy):
///   1. Reset
///   2. Set ACKNOWLEDGE
///   3. Set DRIVER
///   4. Read features, negotiate, write guest features
///   5. Set FEATURES_OK (if applicable)
///   6. Setup virtqueues
///   7. Set DRIVER_OK
pub fn init_device(io_base: u16, accepted_features: u32) -> u32 {
    // Reset
    reset(io_base);

    // Acknowledge
    set_status(io_base, STATUS_ACKNOWLEDGE);
    set_status(io_base, STATUS_DRIVER);

    // Feature negotiation
    let device_features = read_device_features(io_base);
    let negotiated = device_features & accepted_features;
    write_guest_features(io_base, negotiated);

    log::info!("virtio: device features={:#X}, negotiated={:#X}", device_features, negotiated);

    negotiated
}

/// Mark the device as ready (call after virtqueue setup).
pub fn driver_ok(io_base: u16) {
    set_status(io_base, STATUS_DRIVER_OK);
}
