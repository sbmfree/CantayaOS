// ARP — Address Resolution Protocol
//
// Maintains an ARP cache and handles ARP request/reply packets.
//
// ARP packet layout (for IPv4 over Ethernet):
//   ┌──────────────────────┬──────────────────────┐
//   │ Hw Type (2)          │ Proto Type (2)       │
//   ├──────────────────────┼──────────────────────┤
//   │ Hw Addr Len (1)      │ Proto Addr Len (1)   │
//   ├──────────────────────┴──────────────────────┤
//   │ Operation (2)                               │
//   ├─────────────────────────────────────────────┤
//   │ Sender HW Addr (6)                          │
//   ├─────────────────────────────────────────────┤
//   │ Sender Proto Addr (4)                       │
//   ├─────────────────────────────────────────────┤
//   │ Target HW Addr (6)                          │
//   ├─────────────────────────────────────────────┤
//   │ Target Proto Addr (4)                       │
//   └─────────────────────────────────────────────┘

extern crate alloc;

use alloc::vec::Vec;
use spin::Mutex;

const ARP_HW_ETHERNET: u16 = 1;
const ARP_PROTO_IPV4: u16 = 0x0800;
const ARP_OP_REQUEST: u16 = 1;
const ARP_OP_REPLY: u16 = 2;

/// Maximum ARP cache entries.
const ARP_CACHE_SIZE: usize = 64;

/// ARP cache entry.
#[derive(Clone, Copy)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub valid: bool,
}

impl ArpEntry {
    const fn empty() -> Self {
        Self { ip: [0; 4], mac: [0; 6], valid: false }
    }
}

static ARP_CACHE: Mutex<[ArpEntry; ARP_CACHE_SIZE]> =
    Mutex::new([ArpEntry::empty(); ARP_CACHE_SIZE]);

/// Initialize ARP subsystem — seed gateway entry as 00:00:00:00:00:00.
pub fn init() {
    // The gateway ARP entry will be populated by the first ARP reply.
}

/// Look up a MAC address for the given IP in the ARP cache.
pub fn lookup(ip: &[u8; 4]) -> Option<[u8; 6]> {
    let cache = ARP_CACHE.lock();
    for entry in cache.iter() {
        if entry.valid && entry.ip == *ip {
            return Some(entry.mac);
        }
    }
    None
}

/// Insert or update an ARP cache entry.
pub fn insert(ip: [u8; 4], mac: [u8; 6]) {
    let mut cache = ARP_CACHE.lock();
    // Update existing entry
    for entry in cache.iter_mut() {
        if entry.valid && entry.ip == ip {
            entry.mac = mac;
            return;
        }
    }
    // Find free slot
    for entry in cache.iter_mut() {
        if !entry.valid {
            entry.ip = ip;
            entry.mac = mac;
            entry.valid = true;
            return;
        }
    }
    // Cache full — overwrite first entry (simple eviction)
    cache[0].ip = ip;
    cache[0].mac = mac;
    cache[0].valid = true;
}

/// Get all valid ARP cache entries.
pub fn get_cache() -> Vec<ArpEntry> {
    let cache = ARP_CACHE.lock();
    cache.iter().filter(|e| e.valid).copied().collect()
}

/// Process an incoming ARP packet.
pub fn process_arp(payload: &[u8], _src_mac: &[u8; 6]) {
    if payload.len() < 28 { return; }

    let hw_type = u16::from_be_bytes([payload[0], payload[1]]);
    let proto_type = u16::from_be_bytes([payload[2], payload[3]]);
    let hw_len = payload[4];
    let proto_len = payload[5];
    let operation = u16::from_be_bytes([payload[6], payload[7]]);

    // We only handle Ethernet + IPv4
    if hw_type != ARP_HW_ETHERNET || proto_type != ARP_PROTO_IPV4 || hw_len != 6 || proto_len != 4 {
        return;
    }

    let mut sender_mac = [0u8; 6];
    let mut sender_ip = [0u8; 4];
    let mut _target_mac = [0u8; 6];
    let mut target_ip = [0u8; 4];

    sender_mac.copy_from_slice(&payload[8..14]);
    sender_ip.copy_from_slice(&payload[14..18]);
    _target_mac.copy_from_slice(&payload[18..24]);
    target_ip.copy_from_slice(&payload[24..28]);

    // Always learn from incoming ARP packets
    insert(sender_ip, sender_mac);

    let cfg = super::config();
    let mut stats = super::NET_STATS.lock();

    match operation {
        ARP_OP_REQUEST => {
            stats.arp_requests += 1;
            // If they're asking for our IP, send a reply
            if target_ip == cfg.ip {
                drop(stats);
                send_reply(&sender_mac, &sender_ip, &cfg.mac, &cfg.ip);
            }
        }
        ARP_OP_REPLY => {
            stats.arp_replies += 1;
            // Already learned the mapping above
        }
        _ => {}
    }
}

/// Send an ARP reply.
fn send_reply(dst_mac: &[u8; 6], dst_ip: &[u8; 4], src_mac: &[u8; 6], src_ip: &[u8; 4]) {
    let mut arp = [0u8; 28];
    // Hardware type: Ethernet
    arp[0..2].copy_from_slice(&ARP_HW_ETHERNET.to_be_bytes());
    // Protocol type: IPv4
    arp[2..4].copy_from_slice(&ARP_PROTO_IPV4.to_be_bytes());
    // Lengths
    arp[4] = 6; // HW addr len
    arp[5] = 4; // Proto addr len
    // Operation: Reply
    arp[6..8].copy_from_slice(&ARP_OP_REPLY.to_be_bytes());
    // Sender
    arp[8..14].copy_from_slice(src_mac);
    arp[14..18].copy_from_slice(src_ip);
    // Target
    arp[18..24].copy_from_slice(dst_mac);
    arp[24..28].copy_from_slice(dst_ip);

    let frame = super::ethernet::build_frame(
        dst_mac, src_mac,
        super::ethernet::EtherType::Arp,
        &arp,
    );
    super::ethernet::send_raw(&frame);
}

/// Send an ARP request to resolve an IP address.
pub fn send_request(target_ip: &[u8; 4]) {
    let cfg = super::config();
    let mut arp = [0u8; 28];
    arp[0..2].copy_from_slice(&ARP_HW_ETHERNET.to_be_bytes());
    arp[2..4].copy_from_slice(&ARP_PROTO_IPV4.to_be_bytes());
    arp[4] = 6;
    arp[5] = 4;
    arp[6..8].copy_from_slice(&ARP_OP_REQUEST.to_be_bytes());
    arp[8..14].copy_from_slice(&cfg.mac);
    arp[14..18].copy_from_slice(&cfg.ip);
    arp[18..24].copy_from_slice(&[0x00; 6]); // unknown target MAC
    arp[24..28].copy_from_slice(target_ip);

    let frame = super::ethernet::build_frame(
        &super::ethernet::BROADCAST_MAC,
        &cfg.mac,
        super::ethernet::EtherType::Arp,
        &arp,
    );
    super::ethernet::send_raw(&frame);
}

/// Resolve an IP to a MAC address, sending ARP request if needed.
/// Returns None if the address is not in the cache and a request was sent.
pub fn resolve(ip: &[u8; 4]) -> Option<[u8; 6]> {
    if let Some(mac) = lookup(ip) {
        return Some(mac);
    }
    // Not in cache — send ARP request
    send_request(ip);
    None
}

/// Resolve with retries and polling. Blocks up to ~2 seconds.
pub fn resolve_blocking(ip: &[u8; 4]) -> Option<[u8; 6]> {
    // Check cache first
    if let Some(mac) = lookup(ip) {
        return Some(mac);
    }

    // Send ARP request and poll
    for _attempt in 0..4 {
        send_request(ip);
        // Poll for ~500ms
        let start = crate::shell::ticks();
        loop {
            super::poll();
            if let Some(mac) = lookup(ip) {
                return Some(mac);
            }
            let elapsed = crate::shell::ticks().wrapping_sub(start);
            if crate::hal::pit::ticks_to_ms(elapsed) > 500 {
                break;
            }
        }
    }
    None
}
