// ICMP — Internet Control Message Protocol
//
// Handles ICMP Echo Request/Reply (ping) messages.
//
// ICMP header:
//   ┌──────────┬──────────┬──────────────────────┐
//   │  Type    │  Code    │    Checksum           │
//   ├──────────┴──────────┴──────────────────────┤
//   │     Identifier      │  Sequence Number      │
//   ├─────────────────────┴──────────────────────┤
//   │              Data (variable)                │
//   └─────────────────────────────────────────────┘

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};
use spin::Mutex;

const ICMP_ECHO_REPLY: u8 = 0;
const ICMP_ECHO_REQUEST: u8 = 8;

/// ICMP sequence counter.
static ICMP_SEQ: AtomicU16 = AtomicU16::new(1);

/// Pending ping reply storage.
static PING_REPLY: Mutex<Option<PingReply>> = Mutex::new(None);

/// A received ICMP echo reply.
#[derive(Clone, Copy)]
pub struct PingReply {
    pub src_ip: [u8; 4],
    pub seq: u16,
    pub id: u16,
    pub data_len: usize,
    pub ttl: u8,
}

/// Process an incoming ICMP message.
pub fn process_icmp(data: &[u8], src_ip: &[u8; 4]) {
    if data.len() < 8 { return; }

    let icmp_type = data[0];
    let _code = data[1];

    // Verify ICMP checksum
    if super::ipv4::checksum(data) != 0 {
        return;
    }

    let mut stats = super::NET_STATS.lock();

    match icmp_type {
        ICMP_ECHO_REQUEST => {
            stats.icmp_received += 1;
            drop(stats);
            // Send echo reply
            send_echo_reply(src_ip, data);
        }
        ICMP_ECHO_REPLY => {
            stats.icmp_received += 1;
            drop(stats);
            let id = u16::from_be_bytes([data[4], data[5]]);
            let seq = u16::from_be_bytes([data[6], data[7]]);
            let reply = PingReply {
                src_ip: *src_ip,
                seq,
                id,
                data_len: data.len() - 8,
                ttl: 0, // Will be filled in by higher layer if needed
            };
            *PING_REPLY.lock() = Some(reply);
        }
        _ => {}
    }
}

/// Send an ICMP Echo Reply in response to a request.
fn send_echo_reply(dst_ip: &[u8; 4], request: &[u8]) {
    let mut reply = Vec::with_capacity(request.len());
    reply.push(ICMP_ECHO_REPLY); // Type 0
    reply.push(0); // Code 0
    reply.push(0); // Checksum (placeholder)
    reply.push(0);
    reply.extend_from_slice(&request[4..]); // Copy ID, seq, data

    // Compute ICMP checksum
    let cksum = super::ipv4::checksum(&reply);
    reply[2..4].copy_from_slice(&cksum.to_be_bytes());

    super::NET_STATS.lock().icmp_sent += 1;
    super::ipv4::send(dst_ip, super::ipv4::PROTO_ICMP, &reply);
}

/// Send an ICMP Echo Request (ping) to the specified IP.
/// Returns the sequence number used.
pub fn send_echo_request(dst_ip: &[u8; 4]) -> u16 {
    let seq = ICMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let id: u16 = 0xCA05; // "CantayaOS" identifier

    let mut icmp = Vec::with_capacity(64);
    icmp.push(ICMP_ECHO_REQUEST); // Type 8
    icmp.push(0); // Code 0
    icmp.push(0); // Checksum placeholder
    icmp.push(0);
    icmp.extend_from_slice(&id.to_be_bytes());
    icmp.extend_from_slice(&seq.to_be_bytes());

    // 56 bytes of payload data (standard ping size)
    for i in 0..56u8 {
        icmp.push(i);
    }

    // Compute checksum
    let cksum = super::ipv4::checksum(&icmp);
    icmp[2..4].copy_from_slice(&cksum.to_be_bytes());

    // Clear any old reply
    *PING_REPLY.lock() = None;

    super::NET_STATS.lock().icmp_sent += 1;
    super::ipv4::send(dst_ip, super::ipv4::PROTO_ICMP, &icmp);
    seq
}

/// Check if a ping reply has been received.
/// Returns and clears the reply.
pub fn take_ping_reply() -> Option<PingReply> {
    PING_REPLY.lock().take()
}

/// Ping a host. Sends echo request and waits up to `timeout_ms` for a reply.
/// Returns `Some((rtt_ms, reply))` on success.
pub fn ping(dst_ip: &[u8; 4], timeout_ms: u64) -> Option<(u64, PingReply)> {
    let _seq = send_echo_request(dst_ip);
    let start = crate::shell::ticks();

    loop {
        super::poll();

        if let Some(reply) = take_ping_reply() {
            let elapsed = crate::shell::ticks().wrapping_sub(start);
            let rtt = crate::hal::pit::ticks_to_ms(elapsed);
            return Some((rtt, reply));
        }

        let elapsed = crate::shell::ticks().wrapping_sub(start);
        if crate::hal::pit::ticks_to_ms(elapsed) > timeout_ms {
            return None;
        }
    }
}
