// Ethernet Frame Parsing and Construction
//
// Handles Ethernet II frames:
//   ┌──────────┬──────────┬───────────┬─────────────┐
//   │ Dst MAC  │ Src MAC  │ EtherType │   Payload   │
//   │  6 bytes │  6 bytes │  2 bytes  │ 46-1500 B   │
//   └──────────┴──────────┴───────────┴─────────────┘

extern crate alloc;

use alloc::vec::Vec;

/// Well-known EtherType values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum EtherType {
    Ipv4 = 0x0800,
    Arp  = 0x0806,
    Unknown(u16),
}

impl EtherType {
    pub fn from_u16(v: u16) -> Self {
        match v {
            0x0800 => EtherType::Ipv4,
            0x0806 => EtherType::Arp,
            _      => EtherType::Unknown(v),
        }
    }
    pub fn to_u16(self) -> u16 {
        match self {
            EtherType::Ipv4 => 0x0800,
            EtherType::Arp  => 0x0806,
            EtherType::Unknown(v) => v,
        }
    }
}

/// Parsed Ethernet frame header + payload slice offsets.
pub struct EthernetFrame<'a> {
    pub dst_mac: [u8; 6],
    pub src_mac: [u8; 6],
    pub ethertype: EtherType,
    pub payload: &'a [u8],
}

impl<'a> EthernetFrame<'a> {
    /// Parse an Ethernet frame from raw bytes.
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        if data.len() < 14 {
            return None;
        }
        let mut dst_mac = [0u8; 6];
        let mut src_mac = [0u8; 6];
        dst_mac.copy_from_slice(&data[0..6]);
        src_mac.copy_from_slice(&data[6..12]);
        let ethertype = EtherType::from_u16(u16::from_be_bytes([data[12], data[13]]));
        Some(Self {
            dst_mac,
            src_mac,
            ethertype,
            payload: &data[14..],
        })
    }
}

/// Build a complete Ethernet frame.
pub fn build_frame(dst_mac: &[u8; 6], src_mac: &[u8; 6], ethertype: EtherType, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(14 + payload.len());
    frame.extend_from_slice(dst_mac);
    frame.extend_from_slice(src_mac);
    let et = ethertype.to_u16();
    frame.push((et >> 8) as u8);
    frame.push(et as u8);
    frame.extend_from_slice(payload);
    frame
}

/// Broadcast MAC address.
pub const BROADCAST_MAC: [u8; 6] = [0xFF; 6];

/// Process a received Ethernet frame through the network stack.
pub fn process_frame(data: &[u8]) {
    let frame = match EthernetFrame::parse(data) {
        Some(f) => f,
        None => return,
    };

    let cfg = super::config();

    // Only accept frames destined for us or broadcast
    if frame.dst_mac != cfg.mac && frame.dst_mac != BROADCAST_MAC {
        return;
    }

    match frame.ethertype {
        EtherType::Arp => {
            super::arp::process_arp(frame.payload, &frame.src_mac);
        }
        EtherType::Ipv4 => {
            super::ipv4::process_ipv4(frame.payload);
        }
        _ => {} // Drop unknown EtherTypes
    }
}

/// Send a raw Ethernet frame through the virtio-net device.
pub fn send_raw(frame: &[u8]) {
    if !super::is_up() { return; }
    super::stat_tx(frame.len());
    crate::hal::virtio_net::send_frame(frame);
}
