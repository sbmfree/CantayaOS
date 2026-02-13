// UDP — User Datagram Protocol
//
// Basic UDP send/receive support.
//
// UDP header:
//   ┌────────────────────┬──────────────────────┐
//   │   Source Port       │  Destination Port    │
//   ├────────────────────┼──────────────────────┤
//   │     Length          │     Checksum         │
//   ├────────────────────┴──────────────────────┤
//   │              Data (variable)              │
//   └───────────────────────────────────────────┘

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

/// Maximum number of buffered datagrams per socket.
const MAX_BUFFERED: usize = 16;

/// A received UDP datagram.
#[derive(Clone)]
pub struct UdpDatagram {
    pub src_ip: [u8; 4],
    pub src_port: u16,
    pub dst_port: u16,
    pub data: Vec<u8>,
}

/// Socket receive queues, keyed by local port number.
static SOCKETS: Mutex<BTreeMap<u16, Vec<UdpDatagram>>> = Mutex::new(BTreeMap::new());

/// Bind a UDP port to receive datagrams.
pub fn bind(port: u16) {
    let mut sockets = SOCKETS.lock();
    sockets.entry(port).or_insert_with(Vec::new);
}

/// Unbind a UDP port.
pub fn unbind(port: u16) {
    let mut sockets = SOCKETS.lock();
    sockets.remove(&port);
}

/// Receive a datagram from a bound port. Returns None if nothing available.
pub fn recv(port: u16) -> Option<UdpDatagram> {
    let mut sockets = SOCKETS.lock();
    if let Some(queue) = sockets.get_mut(&port) {
        if !queue.is_empty() {
            return Some(queue.remove(0));
        }
    }
    None
}

/// Send a UDP datagram.
pub fn send(dst_ip: &[u8; 4], dst_port: u16, src_port: u16, data: &[u8]) {
    let udp_len = 8 + data.len();
    let mut udp = Vec::with_capacity(udp_len);

    udp.extend_from_slice(&src_port.to_be_bytes());
    udp.extend_from_slice(&dst_port.to_be_bytes());
    udp.extend_from_slice(&(udp_len as u16).to_be_bytes());
    // Checksum = 0 (optional in IPv4)
    udp.push(0);
    udp.push(0);
    udp.extend_from_slice(data);

    super::NET_STATS.lock().udp_sent += 1;
    super::ipv4::send(dst_ip, super::ipv4::PROTO_UDP, &udp);
}

/// Process an incoming UDP datagram.
pub fn process_udp(data: &[u8], src_ip: &[u8; 4], _dst_ip: &[u8; 4]) {
    if data.len() < 8 { return; }

    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let length = u16::from_be_bytes([data[4], data[5]]) as usize;

    if length < 8 || data.len() < length { return; }

    let payload = &data[8..length];

    super::NET_STATS.lock().udp_received += 1;

    let datagram = UdpDatagram {
        src_ip: *src_ip,
        src_port,
        dst_port,
        data: payload.to_vec(),
    };

    let mut sockets = SOCKETS.lock();
    if let Some(queue) = sockets.get_mut(&dst_port) {
        if queue.len() < MAX_BUFFERED {
            queue.push(datagram);
        }
    }
    // If no socket is bound, drop the datagram
}
