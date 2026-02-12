//! Minimal network stack: Ethernet → ARP / IPv4 → ICMP / UDP
//!
//! All processing is done in IRQ context (virtio-net IRQ) or from
//! kernel threads calling `send_*` helpers.

pub mod ethernet;
pub mod arp;
pub mod ipv4;
pub mod icmp;
pub mod udp;
pub mod tcp;
pub mod dns;

use crate::sync::IrqMutex;

/// Our IPv4 address (configurable, default 10.0.2.15 for QEMU user-mode)
static OUR_IP: IrqMutex<[u8; 4]> = IrqMutex::new([10, 0, 2, 15]);
/// Subnet mask
static OUR_MASK: IrqMutex<[u8; 4]> = IrqMutex::new([255, 255, 255, 0]);
/// Gateway
static OUR_GW: IrqMutex<[u8; 4]> = IrqMutex::new([10, 0, 2, 2]);

/// Get our IP address.
pub fn our_ip() -> [u8; 4] {
    *OUR_IP.lock()
}

/// Get our MAC.
pub fn our_mac() -> [u8; 6] {
    crate::drivers::virtio_net::get_mac().unwrap_or([0x52, 0x54, 0x00, 0x12, 0x34, 0x56])
}

/// Called from virtio-net IRQ with a raw Ethernet frame (no virtio header).
pub fn receive_frame(frame: &[u8]) {
    ethernet::receive(frame);
}

/// Transmit raw Ethernet frame.
pub fn send_frame(frame: &[u8]) -> bool {
    crate::drivers::virtio_net::transmit(frame)
}
