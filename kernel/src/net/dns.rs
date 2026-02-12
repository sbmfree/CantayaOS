//! Minimal DNS resolver — sends A-record queries over UDP to a configured
//! nameserver (default: QEMU user-mode DNS at 10.0.2.3).

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, Ordering};

extern crate alloc;

/// DNS server IP (QEMU user-mode networking exposes DNS at 10.0.2.3).
const DNS_SERVER: [u8; 4] = [10, 0, 2, 3];
const DNS_PORT: u16 = 53;
const LOCAL_PORT: u16 = 41000;

/// Transaction ID counter.
static TXID: AtomicU16 = AtomicU16::new(1);

/// Resolve a hostname to an IPv4 address. Returns None on failure/timeout.
pub fn resolve(hostname: &str) -> Option<[u8; 4]> {
    let txid = TXID.fetch_add(1, Ordering::Relaxed);

    // Build DNS query packet
    let query = build_query(txid, hostname);

    // Send via UDP
    super::udp::send(&DNS_SERVER, LOCAL_PORT + (txid % 1000), DNS_PORT, &query);

    // Wait up to 3 seconds for a reply
    let start = crate::hal::timer::uptime_ms();
    while crate::hal::timer::uptime_ms() - start < 3000 {
        // Check for UDP datagrams from DNS server on our port
        if let Some(dgram) = super::udp::poll_port(LOCAL_PORT + (txid % 1000)) {
            if dgram.src_port == DNS_PORT {
                if let Some(ip) = parse_response(&dgram.data, txid) {
                    return Some(ip);
                }
            }
        }
        crate::process::scheduler::yield_thread();
    }

    None
}

/// Build a DNS A-record query for `hostname`.
fn build_query(txid: u16, hostname: &str) -> Vec<u8> {
    let mut pkt = Vec::new();

    // Header (12 bytes)
    pkt.extend_from_slice(&txid.to_be_bytes());   // Transaction ID
    pkt.extend_from_slice(&[0x01, 0x00]);          // Flags: standard query, RD=1
    pkt.extend_from_slice(&[0x00, 0x01]);          // QDCOUNT = 1
    pkt.extend_from_slice(&[0x00, 0x00]);          // ANCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x00]);          // NSCOUNT = 0
    pkt.extend_from_slice(&[0x00, 0x00]);          // ARCOUNT = 0

    // Question section: encode hostname as DNS labels
    for label in hostname.split('.') {
        let len = label.len().min(63) as u8;
        pkt.push(len);
        pkt.extend_from_slice(&label.as_bytes()[..len as usize]);
    }
    pkt.push(0); // root label

    pkt.extend_from_slice(&[0x00, 0x01]); // QTYPE  = A (1)
    pkt.extend_from_slice(&[0x00, 0x01]); // QCLASS = IN (1)

    pkt
}

/// Parse a DNS response and extract the first A record. Returns None if
/// the response doesn't match or contains no A records.
fn parse_response(data: &[u8], expected_txid: u16) -> Option<[u8; 4]> {
    if data.len() < 12 {
        return None;
    }

    let txid = u16::from_be_bytes([data[0], data[1]]);
    if txid != expected_txid {
        return None;
    }

    let flags = u16::from_be_bytes([data[2], data[3]]);
    // Check QR bit (response) and RCODE == 0 (no error)
    if flags & 0x8000 == 0 {
        return None; // not a response
    }
    if flags & 0x000F != 0 {
        return None; // error
    }

    let qdcount = u16::from_be_bytes([data[4], data[5]]) as usize;
    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;

    if ancount == 0 {
        return None;
    }

    // Skip question section
    let mut pos = 12;
    for _ in 0..qdcount {
        pos = skip_name(data, pos)?;
        pos += 4; // QTYPE + QCLASS
        if pos > data.len() {
            return None;
        }
    }

    // Parse answer section — look for the first A record
    for _ in 0..ancount {
        if pos >= data.len() {
            return None;
        }
        pos = skip_name(data, pos)?;
        if pos + 10 > data.len() {
            return None;
        }
        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let _rclass = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        // skip TTL (4 bytes)
        let rdlength = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;

        if rtype == 1 && rdlength == 4 && pos + 4 <= data.len() {
            // A record
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&data[pos..pos + 4]);
            return Some(ip);
        }
        pos += rdlength;
    }

    None
}

/// Skip a DNS name (handles compression pointers). Returns position after the name.
fn skip_name(data: &[u8], mut pos: usize) -> Option<usize> {
    // Guard against infinite loops
    let mut jumps = 0;
    let mut end_pos: Option<usize> = None;

    loop {
        if pos >= data.len() || jumps > 10 {
            return None;
        }
        let len = data[pos];
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            // Compression pointer
            if end_pos.is_none() {
                end_pos = Some(pos + 2);
            }
            if pos + 1 >= data.len() {
                return None;
            }
            pos = ((len as usize & 0x3F) << 8) | data[pos + 1] as usize;
            jumps += 1;
        } else {
            pos += 1 + len as usize;
        }
    }

    Some(end_pos.unwrap_or(pos))
}
