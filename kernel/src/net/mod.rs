// Network Subsystem
//
// This module provides the network stack for CantayaOS:
//   - Ethernet frame parsing and construction
//   - ARP (Address Resolution Protocol)
//   - IPv4 packet handling
//   - ICMP (ping)
//   - UDP datagrams
//
// Architecture:
//
//   ┌────────────────────────────┐
//   │   Shell / Applications     │
//   ├────────────────────────────┤
//   │   UDP / ICMP               │
//   ├────────────────────────────┤
//   │   IPv4                     │
//   ├────────────────────────────┤
//   │   ARP                      │
//   ├────────────────────────────┤
//   │   Ethernet                 │
//   ├────────────────────────────┤
//   │   virtio-net HAL Driver    │
//   └────────────────────────────┘

pub mod ethernet;
pub mod arp;
pub mod ipv4;
pub mod icmp;
pub mod udp;

extern crate alloc;

use spin::Mutex;
use core::sync::atomic::{AtomicBool, Ordering};

/// Network interface configuration
static NET_CONFIG: Mutex<NetConfig> = Mutex::new(NetConfig::new());
static NET_UP: AtomicBool = AtomicBool::new(false);

/// Network statistics
static NET_STATS: Mutex<NetStats> = Mutex::new(NetStats::new());

#[derive(Clone, Copy)]
pub struct NetConfig {
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub mac: [u8; 6],
}

impl NetConfig {
    const fn new() -> Self {
        Self {
            ip: [10, 0, 2, 15],           // QEMU default user-mode IP
            netmask: [255, 255, 255, 0],
            gateway: [10, 0, 2, 2],        // QEMU default gateway
            mac: [0; 6],
        }
    }
}

#[derive(Clone, Copy)]
pub struct NetStats {
    pub tx_packets: u64,
    pub rx_packets: u64,
    pub tx_bytes: u64,
    pub rx_bytes: u64,
    pub arp_requests: u64,
    pub arp_replies: u64,
    pub icmp_sent: u64,
    pub icmp_received: u64,
    pub udp_sent: u64,
    pub udp_received: u64,
}

impl NetStats {
    const fn new() -> Self {
        Self {
            tx_packets: 0, rx_packets: 0,
            tx_bytes: 0, rx_bytes: 0,
            arp_requests: 0, arp_replies: 0,
            icmp_sent: 0, icmp_received: 0,
            udp_sent: 0, udp_received: 0,
        }
    }
}

/// Initialize the networking subsystem.
pub fn init() {
    if !crate::hal::virtio_net::is_available() {
        log::info!("net: no network device available — networking disabled");
        return;
    }

    let mac = crate::hal::virtio_net::mac_address();
    {
        let mut cfg = NET_CONFIG.lock();
        cfg.mac = mac;
    }

    arp::init();
    NET_UP.store(true, Ordering::Release);

    log::info!(
        "net: initialized — IP {}.{}.{}.{} MAC {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        10, 0, 2, 15,
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );
}

/// Check if networking is available.
pub fn is_up() -> bool {
    NET_UP.load(Ordering::Acquire)
}

/// Get current network configuration.
pub fn config() -> NetConfig {
    *NET_CONFIG.lock()
}

/// Get network statistics.
pub fn stats() -> NetStats {
    *NET_STATS.lock()
}

/// Set IP address.
pub fn set_ip(ip: [u8; 4]) {
    NET_CONFIG.lock().ip = ip;
}

/// Set gateway.
pub fn set_gateway(gw: [u8; 4]) {
    NET_CONFIG.lock().gateway = gw;
}

/// Set netmask.
pub fn set_netmask(mask: [u8; 4]) {
    NET_CONFIG.lock().netmask = mask;
}

/// Increment TX stats.
pub fn stat_tx(bytes: usize) {
    let mut s = NET_STATS.lock();
    s.tx_packets += 1;
    s.tx_bytes += bytes as u64;
}

/// Increment RX stats.
pub fn stat_rx(bytes: usize) {
    let mut s = NET_STATS.lock();
    s.rx_packets += 1;
    s.rx_bytes += bytes as u64;
}

/// Poll for received packets and process them through the network stack.
/// Should be called periodically.
pub fn poll() {
    if !is_up() { return; }

    // Poll the virtio-net driver for new frames
    crate::hal::virtio_net::poll_rx();

    // Process received frames
    let mut frame_buf = [0u8; 1514];
    while let Some(len) = crate::hal::virtio_net::recv_frame(&mut frame_buf) {
        stat_rx(len);
        ethernet::process_frame(&frame_buf[..len]);
    }
}

/// Format an IP address as a string.
pub fn format_ip(ip: &[u8; 4]) -> alloc::string::String {
    use alloc::string::String;
    use core::fmt::Write;
    let mut s = String::new();
    write!(s, "{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]).ok();
    s
}

/// Format a MAC address as a string.
pub fn format_mac(mac: &[u8; 6]) -> alloc::string::String {
    use alloc::string::String;
    use core::fmt::Write;
    let mut s = String::new();
    write!(s, "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]).ok();
    s
}

/// Parse an IP address string like "10.0.2.15" into [u8; 4].
pub fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let mut parts = [0u8; 4];
    let mut idx = 0;
    for part in s.split('.') {
        if idx >= 4 { return None; }
        parts[idx] = part.parse::<u8>().ok()?;
        idx += 1;
    }
    if idx == 4 { Some(parts) } else { None }
}
