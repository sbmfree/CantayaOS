//! ARP — Address Resolution Protocol.

use crate::sync::IrqMutex;
use alloc::vec::Vec;

extern crate alloc;

const ARP_HTYPE_ETHERNET: u16 = 1;
const ARP_PTYPE_IPV4: u16 = 0x0800;
const ARP_OP_REQUEST: u16 = 1;
const ARP_OP_REPLY: u16 = 2;

/// ARP cache entry.
#[derive(Clone)]
pub struct ArpCacheEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
}

static ARP_CACHE: IrqMutex<Vec<ArpCacheEntry>> = IrqMutex::new(Vec::new());

/// Look up MAC for an IP.
pub fn lookup(ip: &[u8; 4]) -> Option<[u8; 6]> {
    let cache = ARP_CACHE.lock();
    cache.iter().find(|e| &e.ip == ip).map(|e| e.mac)
}

/// Insert or update an ARP cache entry.
pub fn insert(ip: [u8; 4], mac: [u8; 6]) {
    let mut cache = ARP_CACHE.lock();
    if let Some(entry) = cache.iter_mut().find(|e| e.ip == ip) {
        entry.mac = mac;
    } else {
        cache.push(ArpCacheEntry { ip, mac });
    }
}

/// Process a received ARP packet (Ethernet payload).
pub fn receive(data: &[u8]) {
    if data.len() < 28 {
        return; // minimum ARP packet length
    }

    let htype = u16::from_be_bytes([data[0], data[1]]);
    let ptype = u16::from_be_bytes([data[2], data[3]]);
    let hlen = data[4];
    let plen = data[5];
    let op = u16::from_be_bytes([data[6], data[7]]);

    if htype != ARP_HTYPE_ETHERNET || ptype != ARP_PTYPE_IPV4 || hlen != 6 || plen != 4 {
        return;
    }

    let mut sender_mac = [0u8; 6];
    let mut sender_ip = [0u8; 4];
    let mut target_ip = [0u8; 4];
    sender_mac.copy_from_slice(&data[8..14]);
    sender_ip.copy_from_slice(&data[14..18]);
    // target_mac at 18..24 (not needed for request processing)
    target_ip.copy_from_slice(&data[24..28]);

    // Learn sender's MAC
    insert(sender_ip, sender_mac);

    let our_ip = super::our_ip();

    if op == ARP_OP_REQUEST && target_ip == our_ip {
        // Send ARP reply
        send_reply(&sender_mac, &sender_ip);
    }
}

/// Send an ARP reply to `target_mac`/`target_ip`.
fn send_reply(target_mac: &[u8; 6], target_ip: &[u8; 4]) {
    let our_mac = super::our_mac();
    let our_ip = super::our_ip();

    let mut pkt = [0u8; 28];
    pkt[0..2].copy_from_slice(&ARP_HTYPE_ETHERNET.to_be_bytes());
    pkt[2..4].copy_from_slice(&ARP_PTYPE_IPV4.to_be_bytes());
    pkt[4] = 6; // hlen
    pkt[5] = 4; // plen
    pkt[6..8].copy_from_slice(&ARP_OP_REPLY.to_be_bytes());
    pkt[8..14].copy_from_slice(&our_mac);
    pkt[14..18].copy_from_slice(&our_ip);
    pkt[18..24].copy_from_slice(target_mac);
    pkt[24..28].copy_from_slice(target_ip);

    super::ethernet::send(target_mac, super::ethernet::ETHERTYPE_ARP, &pkt);
}

/// Send an ARP request for `target_ip`.
pub fn send_request(target_ip: &[u8; 4]) {
    let our_mac = super::our_mac();
    let our_ip = super::our_ip();
    let broadcast: [u8; 6] = [0xFF; 6];

    let mut pkt = [0u8; 28];
    pkt[0..2].copy_from_slice(&ARP_HTYPE_ETHERNET.to_be_bytes());
    pkt[2..4].copy_from_slice(&ARP_PTYPE_IPV4.to_be_bytes());
    pkt[4] = 6;
    pkt[5] = 4;
    pkt[6..8].copy_from_slice(&ARP_OP_REQUEST.to_be_bytes());
    pkt[8..14].copy_from_slice(&our_mac);
    pkt[14..18].copy_from_slice(&our_ip);
    pkt[18..24].copy_from_slice(&[0u8; 6]); // target MAC unknown
    pkt[24..28].copy_from_slice(target_ip);

    super::ethernet::send(&broadcast, super::ethernet::ETHERTYPE_ARP, &pkt);
}

/// Get a copy of the ARP cache.
pub fn get_cache() -> Vec<ArpCacheEntry> {
    ARP_CACHE.lock().clone()
}
