//! IPv4 — minimal IP processing (no fragmentation).

pub const PROTO_ICMP: u8 = 1;
pub const PROTO_TCP: u8 = 6;
pub const PROTO_UDP: u8 = 17;

/// Process a received IPv4 packet (Ethernet payload).
pub fn receive(data: &[u8]) {
    if data.len() < 20 {
        return;
    }

    let version_ihl = data[0];
    let _version = version_ihl >> 4;
    let ihl = (version_ihl & 0x0F) as usize;
    let header_len = ihl * 4;

    if data.len() < header_len {
        return;
    }

    let total_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    if data.len() < total_len {
        return;
    }

    let protocol = data[9];
    let mut src_ip = [0u8; 4];
    let mut dst_ip = [0u8; 4];
    src_ip.copy_from_slice(&data[12..16]);
    dst_ip.copy_from_slice(&data[16..20]);

    let payload = &data[header_len..total_len];

    match protocol {
        PROTO_ICMP => super::icmp::receive(&src_ip, payload),
        PROTO_TCP => {
            super::tcp::receive(&src_ip, &dst_ip, payload);
            super::tcp::flush_pending_acks();
        }
        PROTO_UDP => super::udp::receive(&src_ip, &dst_ip, payload),
        _ => {} // ignore
    }
}

/// Send an IPv4 packet. Resolves dst MAC via ARP (or uses gateway).
pub fn send(dst_ip: &[u8; 4], protocol: u8, payload: &[u8]) -> bool {
    let src_ip = super::our_ip();
    let total_len = 20 + payload.len();

    let mut pkt = alloc::vec![0u8; total_len];
    // Version (4) + IHL (5) = 0x45
    pkt[0] = 0x45;
    // DSCP / ECN
    pkt[1] = 0;
    // Total length
    pkt[2..4].copy_from_slice(&(total_len as u16).to_be_bytes());
    // Identification
    static ID: core::sync::atomic::AtomicU16 = core::sync::atomic::AtomicU16::new(1);
    let id = ID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    pkt[4..6].copy_from_slice(&id.to_be_bytes());
    // Flags + Fragment offset (Don't Fragment)
    pkt[6] = 0x40;
    pkt[7] = 0;
    // TTL
    pkt[8] = 64;
    // Protocol
    pkt[9] = protocol;
    // Header checksum (computed below)
    pkt[10] = 0;
    pkt[11] = 0;
    // Source IP
    pkt[12..16].copy_from_slice(&src_ip);
    // Destination IP
    pkt[16..20].copy_from_slice(dst_ip);
    // Payload
    pkt[20..].copy_from_slice(payload);

    // Compute header checksum
    let cksum = checksum(&pkt[0..20]);
    pkt[10..12].copy_from_slice(&cksum.to_be_bytes());

    // Determine next-hop MAC
    let next_hop = if is_local(dst_ip) { *dst_ip } else { *super::OUR_GW.lock() };
    let dst_mac = match super::arp::lookup(&next_hop) {
        Some(mac) => mac,
        None => {
            // Send ARP request and use broadcast as fallback (QEMU will handle it)
            super::arp::send_request(&next_hop);
            // For QEMU user-mode networking, the gateway always responds;
            // use broadcast for now — the host-side tap will accept it.
            [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]
        }
    };

    super::ethernet::send(&dst_mac, super::ethernet::ETHERTYPE_IPV4, &pkt)
}

/// Compute IP checksum.
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
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}

/// Check if `ip` is on our local subnet.
fn is_local(ip: &[u8; 4]) -> bool {
    let mask = *super::OUR_MASK.lock();
    let our = super::our_ip();
    for i in 0..4 {
        if (ip[i] & mask[i]) != (our[i] & mask[i]) {
            return false;
        }
    }
    true
}
