//! ICMP — Internet Control Message Protocol (echo request / reply).

use crate::sync::IrqMutex;
use alloc::collections::VecDeque;

extern crate alloc;

const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;

/// Pending ping replies — (src_ip, sequence, data_len)
static PING_REPLIES: IrqMutex<VecDeque<([u8; 4], u16, usize)>> = IrqMutex::new(VecDeque::new());

/// Process a received ICMP packet.
pub fn receive(src_ip: &[u8; 4], data: &[u8]) {
    if data.len() < 8 {
        return;
    }

    let icmp_type = data[0];
    let _icmp_code = data[1];

    match icmp_type {
        ICMP_ECHO_REQUEST => {
            // Reply with echo response
            send_echo_reply(src_ip, data);
        }
        ICMP_ECHO_REPLY => {
            let seq = u16::from_be_bytes([data[6], data[7]]);
            PING_REPLIES.lock().push_back((*src_ip, seq, data.len() - 8));
        }
        _ => {}
    }
}

/// Send an ICMP echo reply (swap src/dst, change type to 0).
fn send_echo_reply(dst_ip: &[u8; 4], request: &[u8]) {
    let mut reply = alloc::vec![0u8; request.len()];
    reply.copy_from_slice(request);
    reply[0] = ICMP_ECHO_REPLY;
    reply[1] = 0; // code
    // Recompute checksum
    reply[2] = 0;
    reply[3] = 0;
    let cksum = super::ipv4::checksum(&reply);
    reply[2..4].copy_from_slice(&cksum.to_be_bytes());

    super::ipv4::send(dst_ip, super::ipv4::PROTO_ICMP, &reply);
}

/// Send an ICMP echo request (ping).
pub fn send_echo_request(dst_ip: &[u8; 4], seq: u16, payload_len: usize) {
    let total = 8 + payload_len;
    let mut pkt = alloc::vec![0u8; total];
    pkt[0] = ICMP_ECHO_REQUEST;
    pkt[1] = 0;
    // Identifier
    pkt[4..6].copy_from_slice(&0x4F53u16.to_be_bytes()); // "OS"
    // Sequence
    pkt[6..8].copy_from_slice(&seq.to_be_bytes());
    // Fill payload with pattern
    for i in 0..payload_len {
        pkt[8 + i] = (i & 0xFF) as u8;
    }
    // Checksum
    pkt[2] = 0;
    pkt[3] = 0;
    let cksum = super::ipv4::checksum(&pkt);
    pkt[2..4].copy_from_slice(&cksum.to_be_bytes());

    super::ipv4::send(dst_ip, super::ipv4::PROTO_ICMP, &pkt);
}

/// Poll for a ping reply. Returns Some((src_ip, sequence, data_len)) if available.
pub fn poll_reply() -> Option<([u8; 4], u16, usize)> {
    PING_REPLIES.lock().pop_front()
}
