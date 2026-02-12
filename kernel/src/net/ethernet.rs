//! Ethernet frame parsing and construction.

pub const ETH_ALEN: usize = 6;
pub const ETH_HLEN: usize = 14;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const ETHERTYPE_IPV4: u16 = 0x0800;

/// Parse and dispatch an incoming Ethernet frame.
pub fn receive(frame: &[u8]) {
    if frame.len() < ETH_HLEN {
        return;
    }
    let ethertype = u16::from_be_bytes([frame[12], frame[13]]);
    let payload = &frame[ETH_HLEN..];

    match ethertype {
        ETHERTYPE_ARP => super::arp::receive(payload),
        ETHERTYPE_IPV4 => super::ipv4::receive(payload),
        _ => {} // ignore
    }
}

/// Build and send an Ethernet frame.
pub fn send(dst_mac: &[u8; 6], ethertype: u16, payload: &[u8]) -> bool {
    let src_mac = super::our_mac();
    let mut frame = alloc::vec![0u8; ETH_HLEN + payload.len()];
    frame[0..6].copy_from_slice(dst_mac);
    frame[6..12].copy_from_slice(&src_mac);
    frame[12..14].copy_from_slice(&ethertype.to_be_bytes());
    frame[ETH_HLEN..].copy_from_slice(payload);
    super::send_frame(&frame)
}
