// IPv4 — Internet Protocol version 4
//
// Handles IPv4 packet parsing, construction, and routing.
//
// IPv4 header (20 bytes minimum):
//   ┌─────────┬─────────┬──────────────────────┐
//   │ Ver/IHL │  DSCP   │   Total Length        │
//   ├─────────┴─────────┼──────────────────────┤
//   │ Identification    │ Flags/Frag Offset     │
//   ├─────────┬─────────┼──────────────────────┤
//   │   TTL   │ Protocol│   Header Checksum     │
//   ├─────────┴─────────┴──────────────────────┤
//   │           Source IP Address                │
//   ├───────────────────────────────────────────┤
//   │         Destination IP Address             │
//   └───────────────────────────────────────────┘

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};

/// IP protocol numbers.
pub const PROTO_ICMP: u8 = 1;
pub const PROTO_UDP: u8 = 17;

/// IP identification counter.
static IP_ID: AtomicU16 = AtomicU16::new(1);

/// Parsed IPv4 header.
pub struct Ipv4Packet<'a> {
    pub src_ip: [u8; 4],
    pub dst_ip: [u8; 4],
    pub protocol: u8,
    pub ttl: u8,
    pub id: u16,
    pub header_len: usize,
    pub total_len: usize,
    pub payload: &'a [u8],
}

impl<'a> Ipv4Packet<'a> {
    /// Parse an IPv4 packet from raw bytes.
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 20 { return None; }

        let version = data[0] >> 4;
        if version != 4 { return None; }

        let ihl = (data[0] & 0x0F) as usize;
        let header_len = ihl * 4;
        if header_len < 20 || data.len() < header_len { return None; }

        let total_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        if data.len() < total_len { return None; }

        // Verify header checksum
        if checksum(&data[..header_len]) != 0 {
            return None;
        }

        let id = u16::from_be_bytes([data[4], data[5]]);
        let ttl = data[8];
        let protocol = data[9];

        let mut src_ip = [0u8; 4];
        let mut dst_ip = [0u8; 4];
        src_ip.copy_from_slice(&data[12..16]);
        dst_ip.copy_from_slice(&data[16..20]);

        Some(Self {
            src_ip,
            dst_ip,
            protocol,
            ttl,
            id,
            header_len,
            total_len,
            payload: &data[header_len..total_len],
        })
    }
}

/// Compute the Internet checksum (one's complement sum of 16-bit words).
pub fn checksum(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 1 < data.len() {
        sum += u16::from_be_bytes([data[i], data[i + 1]]) as u32;
        i += 2;
    }
    if i < data.len() {
        sum += (data[i] as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Build an IPv4 packet.
pub fn build_packet(dst_ip: &[u8; 4], protocol: u8, payload: &[u8]) -> Vec<u8> {
    let cfg = super::config();
    let total_len = 20 + payload.len();
    let id = IP_ID.fetch_add(1, Ordering::Relaxed);

    let mut pkt = vec![0u8; total_len];
    pkt[0] = 0x45; // Version 4, IHL 5
    pkt[1] = 0x00; // DSCP/ECN
    pkt[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    pkt[4..6].copy_from_slice(&id.to_be_bytes());
    pkt[6] = 0x40; // Don't Fragment
    pkt[7] = 0x00; // Fragment offset
    pkt[8] = 64;   // TTL
    pkt[9] = protocol;
    // Checksum set to 0 for computation
    pkt[10] = 0;
    pkt[11] = 0;
    pkt[12..16].copy_from_slice(&cfg.ip);
    pkt[16..20].copy_from_slice(dst_ip);
    pkt[20..].copy_from_slice(payload);

    // Compute and fill header checksum
    let cksum = checksum(&pkt[..20]);
    pkt[10..12].copy_from_slice(&cksum.to_be_bytes());

    pkt
}

/// Process an incoming IPv4 packet.
pub fn process_ipv4(data: &[u8]) {
    let pkt = match Ipv4Packet::parse(data) {
        Some(p) => p,
        None => return,
    };

    let cfg = super::config();

    // Only accept packets for our IP or broadcast
    if pkt.dst_ip != cfg.ip && pkt.dst_ip != [255, 255, 255, 255] {
        return;
    }

    match pkt.protocol {
        PROTO_ICMP => super::icmp::process_icmp(pkt.payload, &pkt.src_ip),
        PROTO_UDP  => super::udp::process_udp(pkt.payload, &pkt.src_ip, &pkt.dst_ip),
        _ => {} // Drop unsupported protocols
    }
}

/// Send an IPv4 packet to the given destination.
/// Handles ARP resolution and Ethernet framing.
pub fn send(dst_ip: &[u8; 4], protocol: u8, payload: &[u8]) {
    let cfg = super::config();
    let ip_packet = build_packet(dst_ip, protocol, payload);

    // Determine next-hop: if destination is on the same subnet, send directly.
    // Otherwise, send to the gateway.
    let next_hop = if is_same_subnet(&cfg.ip, dst_ip, &cfg.netmask) {
        *dst_ip
    } else {
        cfg.gateway
    };

    // Resolve MAC address for next-hop
    let dst_mac = match super::arp::resolve_blocking(&next_hop) {
        Some(mac) => mac,
        None => {
            log::warn!("net: ARP resolution failed for {}.{}.{}.{}",
                next_hop[0], next_hop[1], next_hop[2], next_hop[3]);
            return;
        }
    };

    let frame = super::ethernet::build_frame(
        &dst_mac,
        &cfg.mac,
        super::ethernet::EtherType::Ipv4,
        &ip_packet,
    );
    super::ethernet::send_raw(&frame);
}

/// Check if two IPs are on the same subnet.
fn is_same_subnet(a: &[u8; 4], b: &[u8; 4], mask: &[u8; 4]) -> bool {
    for i in 0..4 {
        if (a[i] & mask[i]) != (b[i] & mask[i]) {
            return false;
        }
    }
    true
}
