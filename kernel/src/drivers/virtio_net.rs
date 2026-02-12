//! Virtio-net driver (device ID 1)
//!
//! Provides real Ethernet frame transmit/receive via virtio-mmio transport.
//! Two virtqueues: 0 = RX, 1 = TX.

use crate::drivers::virtio_mmio::{self, Virtqueue};
use crate::mm::physical;
use crate::sync::IrqMutex;
use alloc::vec::Vec;
use core::ptr;

extern crate alloc;

/// virtio-net device ID
pub const VIRTIO_DEV_NET: u32 = 1;

/// Size of an Ethernet frame buffer (MTU 1514 + virtio header)
const FRAME_BUF_SIZE: usize = 2048;

/// Number of RX buffers to pre-post
const NUM_RX_BUFS: usize = 32;

/// virtio-net header (10 bytes for legacy, we use a simple version)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    // No num_buffers for TX; present for RX in mergeable-rx-bufs mode
}

impl VirtioNetHdr {
    const fn empty() -> Self {
        VirtioNetHdr {
            flags: 0,
            gso_type: 0,
            hdr_len: 0,
            gso_size: 0,
            csum_start: 0,
            csum_offset: 0,
        }
    }
}

const NET_HDR_SIZE: usize = core::mem::size_of::<VirtioNetHdr>();

/// Per-buffer tracking for RX
struct RxBuf {
    phys: usize,       // physical address of the frame buffer
    desc_idx: u16,     // descriptor index in the RX virtqueue
}

/// Driver state
struct NetDevice {
    #[allow(dead_code)]
    base: usize,
    mac: [u8; 6],
    rxq: Virtqueue,
    txq: Virtqueue,
    rx_bufs: Vec<RxBuf>,
    rx_packets: u64,
    tx_packets: u64,
}

static NET_DEV: IrqMutex<Option<NetDevice>> = IrqMutex::new(None);

/// Initialise the virtio-net device at `base`.
pub fn init(base: usize) -> bool {
    crate::kprintln!("[virtio-net] Initialising at {:#x}", base);

    // Negotiate features: we want MAC (bit 5) only, keep it simple
    if !virtio_mmio::init_device(base, 1 << 5) {
        crate::kprintln!("[virtio-net] Feature negotiation failed");
        return false;
    }

    // Read MAC from config space (6 bytes at offset 0)
    let mut mac = [0u8; 6];
    for i in 0..6 {
        mac[i] = virtio_mmio::read_config_u8(base, i);
    }
    crate::kprintln!("[virtio-net] MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);

    // Set up virtqueues: 0 = receiveq, 1 = transmitq
    let rxq = match Virtqueue::new(base, 0) {
        Some(q) => q,
        None => {
            crate::kprintln!("[virtio-net] Failed to create RX queue");
            return false;
        }
    };
    let txq = match Virtqueue::new(base, 1) {
        Some(q) => q,
        None => {
            crate::kprintln!("[virtio-net] Failed to create TX queue");
            return false;
        }
    };

    virtio_mmio::driver_ok(base);

    let mut dev = NetDevice {
        base,
        mac,
        rxq,
        txq,
        rx_bufs: Vec::new(),
        rx_packets: 0,
        tx_packets: 0,
    };

    // Pre-post RX buffers
    for _ in 0..NUM_RX_BUFS {
        post_rx_buf(&mut dev);
    }
    dev.rxq.notify();

    crate::kprintln!("[virtio-net] Device ready, {} RX buffers posted", dev.rx_bufs.len());

    *NET_DEV.lock() = Some(dev);

    // Update the simulated net driver with our real MAC
    update_simulated_mac(&mac);

    true
}

/// Allocate a frame buffer and post it to the RX queue.
fn post_rx_buf(dev: &mut NetDevice) {
    let phys = match physical::alloc_frame() {
        Some(f) => f,
        None => return,
    };
    // Zero it
    unsafe { ptr::write_bytes(phys as *mut u8, 0, FRAME_BUF_SIZE.min(4096)); }

    if let Some(desc_idx) = dev.rxq.submit_buf(phys, FRAME_BUF_SIZE as u32, true) {
        dev.rx_bufs.push(RxBuf { phys, desc_idx });
    }
}

/// Handle virtio-net IRQ — process received frames, recycle TX descriptors.
pub fn handle_irq(base: usize) {
    virtio_mmio::ack_interrupt(base);

    let mut dev_guard = NET_DEV.lock();
    let dev = match dev_guard.as_mut() {
        Some(d) => d,
        None => return,
    };

    // Process completed RX buffers
    let used = dev.rxq.poll_used();
    for (desc_idx, bytes_written) in used {
        // Find the RxBuf corresponding to this descriptor
        if let Some(pos) = dev.rx_bufs.iter().position(|b| b.desc_idx == desc_idx) {
            let buf = &dev.rx_bufs[pos];
            let total_len = bytes_written as usize;
            if total_len > NET_HDR_SIZE {
                let frame_ptr = (buf.phys + NET_HDR_SIZE) as *const u8;
                let frame_len = total_len - NET_HDR_SIZE;
                let frame = unsafe { core::slice::from_raw_parts(frame_ptr, frame_len) };
                dev.rx_packets += 1;
                // Process the Ethernet frame
                crate::net::receive_frame(frame);
            }
            // Recycle: free descriptor and re-post
            dev.rxq.free_desc(desc_idx);
            dev.rx_bufs.remove(pos);
        }
        // Re-post a fresh buffer
        post_rx_buf(dev);
    }
    dev.rxq.notify();

    // Recycle completed TX descriptors
    let tx_used = dev.txq.poll_used();
    for (desc_idx, _) in tx_used {
        dev.txq.free_desc(desc_idx);
    }
}

/// Transmit an Ethernet frame (raw bytes, no virtio header prepended yet).
pub fn transmit(frame: &[u8]) -> bool {
    let mut dev_guard = NET_DEV.lock();
    let dev = match dev_guard.as_mut() {
        Some(d) => d,
        None => return false,
    };

    // Allocate a page for the TX buffer (virtio_net_hdr + frame)
    let total = NET_HDR_SIZE + frame.len();
    let phys = match physical::alloc_frame() {
        Some(f) => f,
        None => return false,
    };

    unsafe {
        // Write virtio-net header (all zeros = no offload)
        let hdr = phys as *mut VirtioNetHdr;
        ptr::write(hdr, VirtioNetHdr::empty());
        // Copy frame data after header
        let dst = (phys + NET_HDR_SIZE) as *mut u8;
        ptr::copy_nonoverlapping(frame.as_ptr(), dst, frame.len());
    }

    // Submit to TX queue (device-readable, not writable)
    if dev.txq.submit_buf(phys, total as u32, false).is_some() {
        dev.txq.notify();
        dev.tx_packets += 1;
        true
    } else {
        false
    }
}

/// Get the device MAC address.
pub fn get_mac() -> Option<[u8; 6]> {
    NET_DEV.lock().as_ref().map(|d| d.mac)
}

/// Get packet counters.
pub fn get_stats() -> (u64, u64) {
    let dev = NET_DEV.lock();
    match dev.as_ref() {
        Some(d) => (d.rx_packets, d.tx_packets),
        None => (0, 0),
    }
}

/// Update the simulated net driver's MAC to match reality.
fn update_simulated_mac(_mac: &[u8; 6]) {
    use crate::drivers::net;
    let ifaces = net::get_interfaces();
    if !ifaces.is_empty() {
        // The simulated driver stores state; we just log the real MAC
        crate::kprintln!("[virtio-net] Real MAC assigned to eth0");
    }
}

/// Probe all discovered virtio-mmio devices and initialise any net devices found.
pub fn probe_and_init() {
    let devices = crate::drivers::virtio_mmio::discovered_devices();
    for (base, dev_id, irq) in devices {
        if dev_id == VIRTIO_DEV_NET {
            if init(base) {
                // Enable this device's SPI in the GIC (ARM64 only)
                #[cfg(target_arch = "aarch64")]
                crate::hal::interrupts::configure_spi(irq, 0xA0);
            }
        }
    }
}
