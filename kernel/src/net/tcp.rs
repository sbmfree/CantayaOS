//! TCP — Transmission Control Protocol
//!
//! Minimal TCP implementation supporting:
//!   - Active open (connect)
//!   - Send / receive data
//!   - Passive listen (bind + accept)
//!   - Graceful close (FIN)
//!
//! This is intentionally simplified — no congestion control,
//! no out-of-order reassembly, no window scaling.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU16, AtomicU32, Ordering};

use crate::sync::IrqMutex;

// ---------------------------------------------------------------------------
// TCP header flags
// ---------------------------------------------------------------------------
const FIN: u8 = 0x01;
const SYN: u8 = 0x02;
const RST: u8 = 0x04;
const PSH: u8 = 0x08;
const ACK: u8 = 0x10;

// ---------------------------------------------------------------------------
// Connection state
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    LastAck,
    TimeWait,
}

/// Four-tuple identifying a connection
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TcpKey {
    pub local_ip: [u8; 4],
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
}

/// A TCP connection (Transmission Control Block)
pub struct Tcb {
    pub state: TcpState,
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    // Send sequence space
    pub snd_una: u32,  // oldest unacknowledged
    pub snd_nxt: u32,  // next to send
    pub snd_wnd: u16,  // remote window
    // Receive sequence space
    pub rcv_nxt: u32,  // next expected
    pub rcv_wnd: u16,  // our window
    // Buffers
    pub rx_buf: VecDeque<u8>,
    pub tx_buf: VecDeque<u8>,
    // Backlog for listening sockets
    pub accept_queue: VecDeque<TcpKey>,
    pub closed: bool,
}

impl Tcb {
    fn new(local_port: u16) -> Self {
        Self {
            state: TcpState::Closed,
            local_port,
            remote_ip: [0; 4],
            remote_port: 0,
            snd_una: 0,
            snd_nxt: 0,
            snd_wnd: 8192,
            rcv_nxt: 0,
            rcv_wnd: 8192,
            rx_buf: VecDeque::new(),
            tx_buf: VecDeque::new(),
            accept_queue: VecDeque::new(),
            closed: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------
static CONNECTIONS: IrqMutex<BTreeMap<TcpKey, Tcb>> = IrqMutex::new(BTreeMap::new());
/// Listening sockets: local_port → key template
static LISTENERS: IrqMutex<BTreeMap<u16, TcpKey>> = IrqMutex::new(BTreeMap::new());
static NEXT_LOCAL_PORT: AtomicU16 = AtomicU16::new(49152);
static ISN_COUNTER: AtomicU32 = AtomicU32::new(1000);

pub const PROTO_TCP: u8 = 6;

fn alloc_ephemeral_port() -> u16 {
    NEXT_LOCAL_PORT.fetch_add(1, Ordering::Relaxed)
}

fn generate_isn() -> u32 {
    ISN_COUNTER.fetch_add(64000, Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Create a TCP socket (returns a key). Not yet connected.
pub fn socket() -> TcpKey {
    let port = alloc_ephemeral_port();
    let local_ip = super::our_ip();
    TcpKey {
        local_ip,
        local_port: port,
        remote_ip: [0; 4],
        remote_port: 0,
    }
}

/// Actively connect to a remote host (3-way handshake).
/// Returns the established key, or None on failure.
pub fn connect(dst_ip: [u8; 4], dst_port: u16) -> Option<TcpKey> {
    let local_ip = super::our_ip();
    let local_port = alloc_ephemeral_port();
    let key = TcpKey { local_ip, local_port, remote_ip: dst_ip, remote_port: dst_port };

    let isn = generate_isn();
    let mut tcb = Tcb::new(local_port);
    tcb.remote_ip = dst_ip;
    tcb.remote_port = dst_port;
    tcb.snd_nxt = isn + 1;
    tcb.snd_una = isn;
    tcb.state = TcpState::SynSent;

    CONNECTIONS.lock().insert(key, tcb);

    // Send SYN
    send_segment(&key, SYN, isn, 0, &[]);

    // Wait for SYN-ACK (up to 5 seconds)
    let deadline = crate::hal::timer::uptime_ms() + 5000;
    loop {
        if crate::hal::timer::uptime_ms() > deadline {
            CONNECTIONS.lock().remove(&key);
            return None;
        }
        {
            let conns = CONNECTIONS.lock();
            if let Some(tcb) = conns.get(&key) {
                if tcb.state == TcpState::Established {
                    return Some(key);
                }
                if tcb.state == TcpState::Closed {
                    drop(conns);
                    CONNECTIONS.lock().remove(&key);
                    return None;
                }
            }
        }
        crate::process::scheduler::yield_thread();
    }
}

/// Bind + listen on a local port.
pub fn listen(port: u16) -> TcpKey {
    let local_ip = super::our_ip();
    let key = TcpKey { local_ip, local_port: port, remote_ip: [0; 4], remote_port: 0 };

    let mut tcb = Tcb::new(port);
    tcb.state = TcpState::Listen;
    CONNECTIONS.lock().insert(key, tcb);
    LISTENERS.lock().insert(port, key);
    key
}

/// Accept a connection from the listen backlog.
/// Blocks until a connection is available.
pub fn accept(listen_key: &TcpKey) -> Option<TcpKey> {
    let deadline = crate::hal::timer::uptime_ms() + 30_000;
    loop {
        {
            let mut conns = CONNECTIONS.lock();
            if let Some(tcb) = conns.get_mut(listen_key) {
                if let Some(child_key) = tcb.accept_queue.pop_front() {
                    return Some(child_key);
                }
            }
        }
        if crate::hal::timer::uptime_ms() > deadline {
            return None;
        }
        crate::process::scheduler::yield_thread();
    }
}

/// Send data on an established connection.
pub fn send(key: &TcpKey, data: &[u8]) -> bool {
    let mut conns = CONNECTIONS.lock();
    if let Some(tcb) = conns.get_mut(key) {
        if tcb.state != TcpState::Established {
            return false;
        }
        // Copy data to TX buffer
        for &b in data {
            tcb.tx_buf.push_back(b);
        }
        // Flush immediately
        let seq = tcb.snd_nxt;
        let ack = tcb.rcv_nxt;
        let payload: Vec<u8> = tcb.tx_buf.drain(..).collect();
        tcb.snd_nxt = seq.wrapping_add(payload.len() as u32);
        drop(conns);
        send_segment(key, ACK | PSH, seq, ack, &payload);
        true
    } else {
        false
    }
}

/// Receive data from an established connection.
/// Returns number of bytes copied into buf. Non-blocking if no data.
pub fn recv(key: &TcpKey, buf: &mut [u8]) -> usize {
    let mut conns = CONNECTIONS.lock();
    if let Some(tcb) = conns.get_mut(key) {
        let n = buf.len().min(tcb.rx_buf.len());
        for i in 0..n {
            buf[i] = tcb.rx_buf.pop_front().unwrap();
        }
        n
    } else {
        0
    }
}

/// Receive data, blocking until at least 1 byte or connection closed.
pub fn recv_blocking(key: &TcpKey, buf: &mut [u8], timeout_ms: u64) -> usize {
    let deadline = crate::hal::timer::uptime_ms() + timeout_ms;
    loop {
        let n = recv(key, buf);
        if n > 0 { return n; }
        // Check if closed
        {
            let conns = CONNECTIONS.lock();
            if let Some(tcb) = conns.get(key) {
                if tcb.closed || matches!(tcb.state, TcpState::CloseWait | TcpState::Closed | TcpState::TimeWait) {
                    return 0;
                }
            } else {
                return 0;
            }
        }
        if crate::hal::timer::uptime_ms() > deadline {
            return 0;
        }
        crate::process::scheduler::yield_thread();
    }
}

/// Close a TCP connection gracefully.
pub fn close(key: &TcpKey) {
    let mut conns = CONNECTIONS.lock();
    if let Some(tcb) = conns.get_mut(key) {
        match tcb.state {
            TcpState::Established => {
                let seq = tcb.snd_nxt;
                let ack = tcb.rcv_nxt;
                tcb.snd_nxt = seq.wrapping_add(1);
                tcb.state = TcpState::FinWait1;
                drop(conns);
                send_segment(key, FIN | ACK, seq, ack, &[]);
            }
            TcpState::CloseWait => {
                let seq = tcb.snd_nxt;
                let ack = tcb.rcv_nxt;
                tcb.snd_nxt = seq.wrapping_add(1);
                tcb.state = TcpState::LastAck;
                drop(conns);
                send_segment(key, FIN | ACK, seq, ack, &[]);
            }
            TcpState::Listen | TcpState::SynSent => {
                tcb.state = TcpState::Closed;
                tcb.closed = true;
            }
            _ => {}
        }
    }
}

/// Check if a connection has data available.
pub fn has_data(key: &TcpKey) -> bool {
    let conns = CONNECTIONS.lock();
    if let Some(tcb) = conns.get(key) {
        !tcb.rx_buf.is_empty()
    } else {
        false
    }
}

/// Get connection state.
pub fn state(key: &TcpKey) -> TcpState {
    let conns = CONNECTIONS.lock();
    conns.get(key).map(|t| t.state).unwrap_or(TcpState::Closed)
}

// ---------------------------------------------------------------------------
// Receive path — called from ipv4::receive
// ---------------------------------------------------------------------------

/// Process a received TCP segment (IPv4 payload).
pub fn receive(src_ip: &[u8; 4], dst_ip: &[u8; 4], data: &[u8]) {
    if data.len() < 20 { return; }

    let src_port = u16::from_be_bytes([data[0], data[1]]);
    let dst_port = u16::from_be_bytes([data[2], data[3]]);
    let seq = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    let ack_num = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    let data_offset = ((data[12] >> 4) as usize) * 4;
    let flags = data[13];
    let window = u16::from_be_bytes([data[14], data[15]]);

    if data_offset > data.len() { return; }
    let payload = &data[data_offset..];

    let key = TcpKey {
        local_ip: *dst_ip,
        local_port: dst_port,
        remote_ip: *src_ip,
        remote_port: src_port,
    };

    let mut conns = CONNECTIONS.lock();

    // First check for an existing connection
    if let Some(tcb) = conns.get_mut(&key) {
        process_segment(tcb, &key, seq, ack_num, flags, window, payload);
        return;
    }

    // Check for listener
    let listen_key = TcpKey {
        local_ip: *dst_ip,
        local_port: dst_port,
        remote_ip: [0; 4],
        remote_port: 0,
    };

    if flags & SYN != 0 && flags & ACK == 0 {
        if let Some(listen_tcb) = conns.get_mut(&listen_key) {
            // Incoming SYN on a listening port
            let isn = generate_isn();
            let mut child = Tcb::new(dst_port);
            child.remote_ip = *src_ip;
            child.remote_port = src_port;
            child.rcv_nxt = seq.wrapping_add(1);
            child.snd_nxt = isn + 1;
            child.snd_una = isn;
            child.snd_wnd = window;
            child.state = TcpState::SynReceived;

            listen_tcb.accept_queue.push_back(key);
            conns.insert(key, child);
            drop(conns);
            // Send SYN-ACK
            send_segment(&key, SYN | ACK, isn, seq.wrapping_add(1), &[]);
            return;
        }
    }

    // No matching connection — send RST
    drop(conns);
    if flags & RST == 0 {
        send_segment(&key, RST | ACK, 0, seq.wrapping_add(payload.len() as u32).wrapping_add(if flags & SYN != 0 { 1 } else { 0 }), &[]);
    }
}

fn process_segment(tcb: &mut Tcb, key: &TcpKey, seq: u32, ack_num: u32, flags: u8, window: u16, payload: &[u8]) {
    if flags & RST != 0 {
        tcb.state = TcpState::Closed;
        tcb.closed = true;
        return;
    }

    tcb.snd_wnd = window;

    match tcb.state {
        TcpState::SynSent => {
            if flags & SYN != 0 && flags & ACK != 0 {
                tcb.rcv_nxt = seq.wrapping_add(1);
                tcb.snd_una = ack_num;
                tcb.state = TcpState::Established;
                // Send ACK
                let ack_key = *key;
                let snd_nxt = tcb.snd_nxt;
                let rcv_nxt = tcb.rcv_nxt;
                // Can't drop the lock here (called from within locked context)
                // so we'll queue the ACK to be sent after
                PENDING_ACKS.lock().push_back((ack_key, snd_nxt, rcv_nxt));
            }
        }
        TcpState::SynReceived => {
            if flags & ACK != 0 {
                tcb.snd_una = ack_num;
                tcb.state = TcpState::Established;
            }
        }
        TcpState::Established => {
            if flags & ACK != 0 {
                tcb.snd_una = ack_num;
            }

            // Receive data
            if !payload.is_empty() && seq == tcb.rcv_nxt {
                for &b in payload {
                    tcb.rx_buf.push_back(b);
                }
                tcb.rcv_nxt = seq.wrapping_add(payload.len() as u32);
                // Send ACK
                PENDING_ACKS.lock().push_back((*key, tcb.snd_nxt, tcb.rcv_nxt));
            }

            // FIN
            if flags & FIN != 0 {
                tcb.rcv_nxt = tcb.rcv_nxt.wrapping_add(1);
                tcb.state = TcpState::CloseWait;
                tcb.closed = true;
                PENDING_ACKS.lock().push_back((*key, tcb.snd_nxt, tcb.rcv_nxt));
            }
        }
        TcpState::FinWait1 => {
            if flags & ACK != 0 {
                tcb.snd_una = ack_num;
                if flags & FIN != 0 {
                    tcb.rcv_nxt = seq.wrapping_add(1);
                    tcb.state = TcpState::TimeWait;
                    tcb.closed = true;
                    PENDING_ACKS.lock().push_back((*key, tcb.snd_nxt, tcb.rcv_nxt));
                } else {
                    tcb.state = TcpState::FinWait2;
                }
            }
        }
        TcpState::FinWait2 => {
            if flags & FIN != 0 {
                tcb.rcv_nxt = seq.wrapping_add(1);
                tcb.state = TcpState::TimeWait;
                tcb.closed = true;
                PENDING_ACKS.lock().push_back((*key, tcb.snd_nxt, tcb.rcv_nxt));
            }
        }
        TcpState::LastAck => {
            if flags & ACK != 0 {
                tcb.state = TcpState::Closed;
                tcb.closed = true;
            }
        }
        _ => {}
    }
}

// Pending ACKs to send outside of lock context
static PENDING_ACKS: IrqMutex<VecDeque<(TcpKey, u32, u32)>> = IrqMutex::new(VecDeque::new());

/// Flush any pending ACKs — call this after receive processing.
pub fn flush_pending_acks() {
    loop {
        let item = PENDING_ACKS.lock().pop_front();
        match item {
            Some((key, seq, ack)) => send_segment(&key, ACK, seq, ack, &[]),
            None => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Send path
// ---------------------------------------------------------------------------

fn send_segment(key: &TcpKey, flags: u8, seq: u32, ack: u32, payload: &[u8]) {
    let header_len = 20u8; // no options
    let total = header_len as usize + payload.len();
    let mut seg = alloc::vec![0u8; total];

    // Source port
    seg[0..2].copy_from_slice(&key.local_port.to_be_bytes());
    // Destination port
    seg[2..4].copy_from_slice(&key.remote_port.to_be_bytes());
    // Sequence number
    seg[4..8].copy_from_slice(&seq.to_be_bytes());
    // Acknowledgment number
    seg[8..12].copy_from_slice(&ack.to_be_bytes());
    // Data offset (5 * 4 = 20 bytes) + reserved
    seg[12] = (header_len / 4) << 4;
    // Flags
    seg[13] = flags;
    // Window
    seg[14..16].copy_from_slice(&8192u16.to_be_bytes());
    // Checksum (computed below)
    seg[16..18].copy_from_slice(&[0, 0]);
    // Urgent pointer
    seg[18..20].copy_from_slice(&[0, 0]);
    // Payload
    if !payload.is_empty() {
        seg[20..].copy_from_slice(payload);
    }

    // TCP checksum with pseudo-header
    let cksum = tcp_checksum(&key.local_ip, &key.remote_ip, &seg);
    seg[16..18].copy_from_slice(&cksum.to_be_bytes());

    super::ipv4::send(&key.remote_ip, PROTO_TCP, &seg);
}

fn tcp_checksum(src_ip: &[u8; 4], dst_ip: &[u8; 4], tcp_segment: &[u8]) -> u16 {
    let mut sum: u32 = 0;

    // Pseudo header
    sum += u16::from_be_bytes([src_ip[0], src_ip[1]]) as u32;
    sum += u16::from_be_bytes([src_ip[2], src_ip[3]]) as u32;
    sum += u16::from_be_bytes([dst_ip[0], dst_ip[1]]) as u32;
    sum += u16::from_be_bytes([dst_ip[2], dst_ip[3]]) as u32;
    sum += PROTO_TCP as u32; // protocol
    sum += tcp_segment.len() as u32; // TCP length

    // TCP segment
    let mut i = 0;
    while i + 1 < tcp_segment.len() {
        sum += u16::from_be_bytes([tcp_segment[i], tcp_segment[i + 1]]) as u32;
        i += 2;
    }
    if i < tcp_segment.len() {
        sum += (tcp_segment[i] as u32) << 8;
    }

    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !(sum as u16)
}
