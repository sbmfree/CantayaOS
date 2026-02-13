// Virtio Network Device Driver
//
// This module implements a virtio-net driver using the legacy PCI transport.
// It provides Ethernet frame send/receive for the CantayaOS networking stack.
//
// The virtio-net device uses two virtqueues:
//   - Queue 0 (receiveq): incoming packets from the network
//   - Queue 1 (transmitq): outgoing packets to the network
//
// Each packet is prefixed with a virtio_net_hdr (10 bytes for legacy).
//
// Device-specific config (at BAR0 + 20):
//   Offset 0:  MAC address (6 bytes)
//   Offset 6:  Status (u16)
//
// Reference: virtio spec §5.1

use crate::hal::virtio::{self, Virtqueue, VRING_DESC_F_WRITE, VRING_DESC_F_NEXT};
use crate::memory::frame_allocator;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

/// Virtio-net legacy PCI device ID
const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;

/// Maximum Ethernet frame size (MTU 1500 + headers)
const MAX_FRAME_SIZE: usize = 1514;

/// Virtio net header size (legacy, no mergeable buffers)
const VIRTIO_NET_HDR_SIZE: usize = 10;

/// Max packet buffer size including virtio header
const PACKET_BUF_SIZE: usize = VIRTIO_NET_HDR_SIZE + MAX_FRAME_SIZE;

/// Number of receive buffers to pre-populate
const RX_RING_SIZE: usize = 16;

/// Feature bits
const VIRTIO_NET_F_MAC: u32 = 1 << 5;
const VIRTIO_NET_F_STATUS: u32 = 1 << 16;

/// Virtio net header (legacy, 10 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
}

/// Global virtio-net device instance
static VIRTIO_NET: Mutex<Option<VirtioNetDevice>> = Mutex::new(None);

/// Whether the device has been initialized
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// I/O base for interrupt handler (lock-free)
static IRQ_IO_BASE: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(0);

/// MAC address (set on init)
static MAC_ADDR: Mutex<[u8; 6]> = Mutex::new([0u8; 6]);

/// Received packet queue (ring buffer of received frames)
const RX_QUEUE_CAPACITY: usize = 32;
static RX_PACKET_QUEUE: Mutex<RxQueue> = Mutex::new(RxQueue::new());

struct RxQueue {
    packets: [[u8; MAX_FRAME_SIZE]; RX_QUEUE_CAPACITY],
    lengths: [usize; RX_QUEUE_CAPACITY],
    head: usize,    // next slot to read
    tail: usize,    // next slot to write
    count: usize,
}

impl RxQueue {
    const fn new() -> Self {
        Self {
            packets: [[0u8; MAX_FRAME_SIZE]; RX_QUEUE_CAPACITY],
            lengths: [0; RX_QUEUE_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, data: &[u8]) -> bool {
        if self.count >= RX_QUEUE_CAPACITY {
            return false; // full
        }
        let len = data.len().min(MAX_FRAME_SIZE);
        self.packets[self.tail][..len].copy_from_slice(&data[..len]);
        self.lengths[self.tail] = len;
        self.tail = (self.tail + 1) % RX_QUEUE_CAPACITY;
        self.count += 1;
        true
    }

    fn pop(&mut self, buf: &mut [u8]) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let len = self.lengths[self.head];
        let copy_len = len.min(buf.len());
        buf[..copy_len].copy_from_slice(&self.packets[self.head][..copy_len]);
        self.head = (self.head + 1) % RX_QUEUE_CAPACITY;
        self.count -= 1;
        Some(copy_len)
    }
}

/// Virtio network device state
struct VirtioNetDevice {
    io_base: u16,
    rx_vq: Virtqueue,
    tx_vq: Virtqueue,
    mac: [u8; 6],
    /// Physical address of RX buffer pool (RX_RING_SIZE * PACKET_BUF_SIZE)
    rx_bufs_phys: u64,
    /// Physical address of TX buffer (single, reused)
    tx_buf_phys: u64,
}

/// Initialize the virtio-net driver.
pub fn init() -> bool {
    let dev = match virtio::find_device(VIRTIO_NET_DEVICE_ID) {
        Some(d) => d,
        None => {
            log::info!("virtio-net: no device found");
            return false;
        }
    };

    log::info!(
        "virtio-net: found device at {:02X}:{:02X}.{} IRQ={}",
        dev.bus, dev.device, dev.function, dev.interrupt_line
    );

    let io_base = match virtio::bar0_ioport(&dev) {
        Some(port) => port,
        None => {
            log::error!("virtio-net: BAR0 is not I/O port");
            return false;
        }
    };

    log::info!("virtio-net: I/O base = {:#X}", io_base);

    // Accept MAC and STATUS features
    let accepted = VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS;
    let _features = virtio::init_device(io_base, accepted);

    // Read MAC address from device config
    let mut mac = [0u8; 6];
    unsafe {
        use crate::hal::port::inb;
        for i in 0..6 {
            mac[i] = inb(io_base + virtio::REG_DEVICE_CONFIG + i as u16);
        }
    }
    log::info!(
        "virtio-net: MAC = {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
    *MAC_ADDR.lock() = mac;

    // Set up receive virtqueue (queue 0)
    let rx_vq = match Virtqueue::new(io_base, 0) {
        Some(q) => q,
        None => {
            log::error!("virtio-net: failed to initialize RX virtqueue");
            virtio::set_status(io_base, virtio::STATUS_FAILED);
            return false;
        }
    };

    // Set up transmit virtqueue (queue 1)
    let tx_vq = match Virtqueue::new(io_base, 1) {
        Some(q) => q,
        None => {
            log::error!("virtio-net: failed to initialize TX virtqueue");
            virtio::set_status(io_base, virtio::STATUS_FAILED);
            return false;
        }
    };

    // Mark device ready
    virtio::driver_ok(io_base);

    // Allocate RX buffers (contiguous pages)
    let rx_total = RX_RING_SIZE * PACKET_BUF_SIZE;
    let rx_pages = (rx_total + 4095) / 4096;
    let rx_bufs_phys = match frame_allocator::allocate_contiguous_frames(rx_pages) {
        Some(p) => p,
        None => {
            log::error!("virtio-net: failed to allocate RX buffers");
            return false;
        }
    };
    unsafe { core::ptr::write_bytes(rx_bufs_phys as *mut u8, 0, rx_pages * 4096); }

    // Allocate TX buffer (1 page is enough)
    let tx_buf_phys = match frame_allocator::allocate_frame() {
        Some(p) => p,
        None => {
            log::error!("virtio-net: failed to allocate TX buffer");
            return false;
        }
    };
    unsafe { core::ptr::write_bytes(tx_buf_phys as *mut u8, 0, 4096); }

    // Enable virtio IRQ
    crate::hal::idt::register_virtio_irq(dev.interrupt_line);
    enable_virtio_irq(dev.interrupt_line);
    IRQ_IO_BASE.store(io_base, Ordering::Release);

    let mut device = VirtioNetDevice {
        io_base,
        rx_vq,
        tx_vq,
        mac,
        rx_bufs_phys,
        tx_buf_phys,
    };

    // Pre-populate RX queue with buffers
    populate_rx_buffers(&mut device);

    *VIRTIO_NET.lock() = Some(device);
    INITIALIZED.store(true, Ordering::Release);

    log::info!("virtio-net: driver initialized successfully");
    true
}

/// Pre-populate the RX virtqueue with receive buffers.
fn populate_rx_buffers(dev: &mut VirtioNetDevice) {
    for i in 0..RX_RING_SIZE {
        let buf_phys = dev.rx_bufs_phys + (i * PACKET_BUF_SIZE) as u64;
        if let Some(desc_idx) = dev.rx_vq.alloc_desc() {
            unsafe {
                let desc = dev.rx_vq.desc_ptr(desc_idx);
                (*desc).addr = buf_phys;
                (*desc).len = PACKET_BUF_SIZE as u32;
                (*desc).flags = VRING_DESC_F_WRITE; // device writes into this buffer
                (*desc).next = 0;
            }
            dev.rx_vq.submit(desc_idx);
        }
    }
}

/// Enable an IRQ in the PIC
fn enable_virtio_irq(irq_line: u8) {
    use crate::hal::port::{inb, outb};
    if irq_line >= 16 { return; }
    unsafe {
        if irq_line < 8 {
            let mask = inb(0x21);
            outb(0x21, mask & !(1 << irq_line));
        } else {
            let mask = inb(0xA1);
            outb(0xA1, mask & !(1 << (irq_line - 8)));
        }
    }
    log::info!("virtio-net: unmasked IRQ {}", irq_line);
}

/// Check if the driver is initialized.
pub fn is_available() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Get the MAC address.
pub fn mac_address() -> [u8; 6] {
    *MAC_ADDR.lock()
}

/// Send an Ethernet frame.
/// `frame` should be a complete Ethernet frame (dst_mac + src_mac + ethertype + payload).
pub fn send_frame(frame: &[u8]) -> bool {
    if !is_available() { return false; }
    if frame.len() > MAX_FRAME_SIZE { return false; }

    let mut lock = VIRTIO_NET.lock();
    let dev = match lock.as_mut() {
        Some(d) => d,
        None => return false,
    };

    // Build packet: virtio_net_hdr + frame
    let total_len = VIRTIO_NET_HDR_SIZE + frame.len();
    unsafe {
        let buf = dev.tx_buf_phys as *mut u8;
        // Zero the virtio net header
        core::ptr::write_bytes(buf, 0, VIRTIO_NET_HDR_SIZE);
        // Copy frame after header
        core::ptr::copy_nonoverlapping(frame.as_ptr(), buf.add(VIRTIO_NET_HDR_SIZE), frame.len());
    }

    // Allocate a descriptor
    let desc_idx = match dev.tx_vq.alloc_desc() {
        Some(d) => d,
        None => {
            // Try to reclaim used descriptors
            while let Some((head, _)) = dev.tx_vq.poll_used() {
                dev.tx_vq.free_chain(head);
            }
            match dev.tx_vq.alloc_desc() {
                Some(d) => d,
                None => return false,
            }
        }
    };

    unsafe {
        let desc = dev.tx_vq.desc_ptr(desc_idx);
        (*desc).addr = dev.tx_buf_phys;
        (*desc).len = total_len as u32;
        (*desc).flags = 0; // device-readable
        (*desc).next = 0;
    }

    dev.tx_vq.submit(desc_idx);

    // Wait for completion
    let (head, _) = dev.tx_vq.wait_used();
    dev.tx_vq.free_chain(head);

    true
}

/// Poll for received packets.
/// Called periodically (e.g., from the network stack or timer).
/// Drains the RX used ring and pushes frames into the RX_PACKET_QUEUE.
pub fn poll_rx() {
    if !is_available() { return; }

    let mut lock = VIRTIO_NET.lock();
    let dev = match lock.as_mut() {
        Some(d) => d,
        None => return,
    };

    while let Some((head, len)) = dev.rx_vq.poll_used() {
        if len > VIRTIO_NET_HDR_SIZE as u32 {
            let desc_idx = head;
            let buf_phys = unsafe {
                let desc = dev.rx_vq.desc_ptr(desc_idx);
                (*desc).addr
            };

            let frame_len = (len as usize) - VIRTIO_NET_HDR_SIZE;
            let frame_ptr = (buf_phys + VIRTIO_NET_HDR_SIZE as u64) as *const u8;

            let mut frame_buf = [0u8; MAX_FRAME_SIZE];
            let copy_len = frame_len.min(MAX_FRAME_SIZE);
            unsafe {
                core::ptr::copy_nonoverlapping(frame_ptr, frame_buf.as_mut_ptr(), copy_len);
            }

            // Push into the software RX queue
            let mut rxq = RX_PACKET_QUEUE.lock();
            rxq.push(&frame_buf[..copy_len]);
        }

        // Recycle the descriptor: re-submit it for future receives
        unsafe {
            let desc = dev.rx_vq.desc_ptr(head);
            (*desc).len = PACKET_BUF_SIZE as u32;
            (*desc).flags = VRING_DESC_F_WRITE;
            (*desc).next = 0;
        }
        dev.rx_vq.submit(head);
    }
}

/// Receive one Ethernet frame from the RX queue.
/// Returns the number of bytes copied, or None if no frame available.
pub fn recv_frame(buf: &mut [u8]) -> Option<usize> {
    let mut rxq = RX_PACKET_QUEUE.lock();
    rxq.pop(buf)
}

/// Handle a virtio-net interrupt (called from IDT handler).
pub fn handle_interrupt() {
    let io_base = IRQ_IO_BASE.load(Ordering::Acquire);
    if io_base != 0 {
        let _isr = virtio::read_isr(io_base);
        // Signal to poll_rx that data may be available
        virtio::VIRTIO_IRQ_FIRED.store(true, Ordering::Release);
    }
}
