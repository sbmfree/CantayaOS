//! UDP — User Datagram Protocol (minimal, connectionless).

use crate::sync::IrqMutex;
use alloc::collections::VecDeque;
use alloc::vec::Vec;

extern crate alloc;

/// Received UDP datagram.
#[derive(Clone)]
pub struct UdpDatagram {
    pub src_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
    pub data: Vec<u8>,
}

/// Incoming datagram queue (simple global queue; a real OS would have per-socket buffers).
static RX_QUEUE: IrqMutex<VecDeque<UdpDatagram>> = IrqMutex::new(VecDeque::new());

/// Process a received UDP packet (IPv4 payload).
pub fn receive(src_ip: &[u8; 4], _dst_ip: &[u8; 4], data: &[u8]) {
    if data.len() < 8 {
        return;
    }

    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let length = u16::from_be_bytes([data[4], data[5]]) as usize;

    if data.len() < length || length < 8 {
        return;
    }

    let payload = &data[8..length];

    RX_QUEUE.lock().push_back(UdpDatagram {
        src_ip: *src_ip,
        src_port,
        dst_port,
        data: payload.to_vec(),
    });
}

/// Send a UDP datagram.
pub fn send(dst_ip: &[u8; 4], src_port: u16, dst_port: u16, payload: &[u8]) -> bool {
    let total = 8 + payload.len();
    let mut pkt = alloc::vec![0u8; total];
    pkt[0..2].copy_from_slice(&src_port.to_be_bytes());
    pkt[2..4].copy_from_slice(&dst_port.to_be_bytes());
    pkt[4..6].copy_from_slice(&(total as u16).to_be_bytes());
    // Checksum = 0 (optional for IPv4 UDP)
    pkt[6] = 0;
    pkt[7] = 0;
    pkt[8..].copy_from_slice(payload);

    super::ipv4::send(dst_ip, super::ipv4::PROTO_UDP, &pkt)
}

/// Poll for a received UDP datagram (any port).
pub fn poll() -> Option<UdpDatagram> {
    RX_QUEUE.lock().pop_front()
}

/// Poll for UDP datagrams on a specific port.
pub fn poll_port(port: u16) -> Option<UdpDatagram> {
    let mut queue = RX_QUEUE.lock();
    if let Some(pos) = queue.iter().position(|d| d.dst_port == port) {
        queue.remove(pos)
    } else {
        None
    }
}
