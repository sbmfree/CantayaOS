//! Virtual Network Interface
//!
//! Simulated network stack providing a virtual Ethernet interface
//! with IP configuration, ARP table, and connection tracking.
//! Designed to give CantayaOS a realistic network subsystem.

use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;
use spin::Mutex;

extern crate alloc;

/// Network interface state
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InterfaceState {
    Up,
    Down,
}

/// A network interface
#[derive(Clone)]
pub struct NetworkInterface {
    pub name: String,
    pub mac: [u8; 6],
    pub ip: [u8; 4],
    pub netmask: [u8; 4],
    pub gateway: [u8; 4],
    pub dns: [u8; 4],
    pub mtu: u32,
    pub state: InterfaceState,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
}

/// ARP table entry
#[derive(Clone)]
pub struct ArpEntry {
    pub ip: [u8; 4],
    pub mac: [u8; 6],
    pub iface: String,
}

/// Connection tracking entry
#[derive(Clone)]
pub struct Connection {
    pub proto: &'static str,
    pub local_ip: [u8; 4],
    pub local_port: u16,
    pub remote_ip: [u8; 4],
    pub remote_port: u16,
    pub state: &'static str,
}

/// Routing table entry
#[derive(Clone)]
pub struct RouteEntry {
    pub destination: [u8; 4],
    pub mask: [u8; 4],
    pub gateway: [u8; 4],
    pub iface: String,
    pub metric: u32,
}

struct NetState {
    interfaces: Vec<NetworkInterface>,
    arp_table: Vec<ArpEntry>,
    connections: Vec<Connection>,
    routes: Vec<RouteEntry>,
}

static NET: Mutex<Option<NetState>> = Mutex::new(None);

/// Initialize the network subsystem
pub fn init() {
    let mut state = NetState {
        interfaces: Vec::new(),
        arp_table: Vec::new(),
        connections: Vec::new(),
        routes: Vec::new(),
    };

    // Create eth0 — virtual Ethernet via QEMU user-mode networking
    state.interfaces.push(NetworkInterface {
        name: String::from("eth0"),
        mac: [0x52, 0x54, 0x00, 0x12, 0x34, 0x56],
        ip: [10, 0, 2, 15],
        netmask: [255, 255, 255, 0],
        gateway: [10, 0, 2, 2],
        dns: [10, 0, 2, 3],
        mtu: 1500,
        state: InterfaceState::Up,
        rx_packets: 142,
        tx_packets: 98,
        rx_bytes: 18432,
        tx_bytes: 7680,
        rx_errors: 0,
        tx_errors: 0,
        rx_dropped: 0,
        tx_dropped: 0,
    });

    // Create lo — loopback
    state.interfaces.push(NetworkInterface {
        name: String::from("lo"),
        mac: [0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ip: [127, 0, 0, 1],
        netmask: [255, 0, 0, 0],
        gateway: [0, 0, 0, 0],
        dns: [0, 0, 0, 0],
        mtu: 65536,
        state: InterfaceState::Up,
        rx_packets: 24,
        tx_packets: 24,
        rx_bytes: 1920,
        tx_bytes: 1920,
        rx_errors: 0,
        tx_errors: 0,
        rx_dropped: 0,
        tx_dropped: 0,
    });

    // Pre-populate ARP table
    state.arp_table.push(ArpEntry {
        ip: [10, 0, 2, 2],
        mac: [0x52, 0x54, 0x00, 0xAA, 0xBB, 0x01],
        iface: String::from("eth0"),
    });
    state.arp_table.push(ArpEntry {
        ip: [10, 0, 2, 3],
        mac: [0x52, 0x54, 0x00, 0xAA, 0xBB, 0x02],
        iface: String::from("eth0"),
    });

    // Pre-populate some connections
    state.connections.push(Connection {
        proto: "tcp",
        local_ip: [10, 0, 2, 15],
        local_port: 22,
        remote_ip: [0, 0, 0, 0],
        remote_port: 0,
        state: "LISTEN",
    });
    state.connections.push(Connection {
        proto: "udp",
        local_ip: [10, 0, 2, 15],
        local_port: 68,
        remote_ip: [10, 0, 2, 2],
        remote_port: 67,
        state: "ESTABLISHED",
    });

    // Routing table
    state.routes.push(RouteEntry {
        destination: [10, 0, 2, 0],
        mask: [255, 255, 255, 0],
        gateway: [0, 0, 0, 0],
        iface: String::from("eth0"),
        metric: 100,
    });
    state.routes.push(RouteEntry {
        destination: [0, 0, 0, 0],
        mask: [0, 0, 0, 0],
        gateway: [10, 0, 2, 2],
        iface: String::from("eth0"),
        metric: 100,
    });
    state.routes.push(RouteEntry {
        destination: [127, 0, 0, 0],
        mask: [255, 0, 0, 0],
        gateway: [0, 0, 0, 0],
        iface: String::from("lo"),
        metric: 0,
    });

    *NET.lock() = Some(state);
}

/// Format IP address
pub fn format_ip(ip: &[u8; 4]) -> String {
    format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3])
}

/// Format MAC address  
pub fn format_mac(mac: &[u8; 6]) -> String {
    format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
}

/// Get interfaces list
pub fn get_interfaces() -> Vec<NetworkInterface> {
    let net = NET.lock();
    match net.as_ref() {
        Some(state) => state.interfaces.clone(),
        None => Vec::new(),
    }
}

/// Get specific interface
pub fn get_interface(name: &str) -> Option<NetworkInterface> {
    let net = NET.lock();
    net.as_ref()?.interfaces.iter().find(|i| i.name == name).cloned()
}

/// Get ARP table
pub fn get_arp_table() -> Vec<ArpEntry> {
    let net = NET.lock();
    match net.as_ref() {
        Some(state) => state.arp_table.clone(),
        None => Vec::new(),
    }
}

/// Get connections
pub fn get_connections() -> Vec<Connection> {
    let net = NET.lock();
    match net.as_ref() {
        Some(state) => state.connections.clone(),
        None => Vec::new(),
    }
}

/// Get routing table
pub fn get_routes() -> Vec<RouteEntry> {
    let net = NET.lock();
    match net.as_ref() {
        Some(state) => state.routes.clone(),
        None => Vec::new(),
    }
}

/// Simulate a ping to an IP. Returns (success, latency_ms)
pub fn ping(target: &[u8; 4]) -> (bool, u64) {
    // Simulate network latency based on target
    let latency = if *target == [127, 0, 0, 1] {
        // Loopback — ~0.1ms
        let base = (crate::hal::timer::timestamp() % 100) as u64;
        base / 100  // 0-1ms
    } else if target[0] == 10 && target[1] == 0 && target[2] == 2 {
        // Local subnet — 1-5ms
        let base = (crate::hal::timer::timestamp() % 500) as u64;
        1 + base / 100
    } else if target[0] == 192 || target[0] == 172 {
        // Private network — 5-20ms
        let base = (crate::hal::timer::timestamp() % 1500) as u64;
        5 + base / 100
    } else {
        // Public internet — 10-80ms
        let base = (crate::hal::timer::timestamp() % 7000) as u64;
        10 + base / 100
    };

    // Bump packet counters
    {
        let mut net = NET.lock();
        if let Some(state) = net.as_mut() {
            if let Some(iface) = state.interfaces.iter_mut().find(|i| i.name == "eth0") {
                iface.tx_packets += 1;
                iface.rx_packets += 1;
                iface.tx_bytes += 64; // ICMP echo request
                iface.rx_bytes += 64; // ICMP echo reply
            }
        }
    }

    (true, latency)
}

/// Parse IP string like "10.0.2.1" into [u8; 4]
pub fn parse_ip(s: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let a = parts[0].parse::<u8>().ok()?;
    let b = parts[1].parse::<u8>().ok()?;
    let c = parts[2].parse::<u8>().ok()?;
    let d = parts[3].parse::<u8>().ok()?;
    Some([a, b, c, d])
}

/// Generate /proc/net/dev content
pub fn proc_net_dev() -> String {
    let mut s = String::new();
    s.push_str("Inter-|   Receive                                                |  Transmit\n");
    s.push_str(" face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n");
    for iface in get_interfaces() {
        s.push_str(&format!("{:>6}: {:>8} {:>7} {:>4} {:>4}    0     0          0         0 {:>8} {:>7} {:>4} {:>4}    0     0       0          0\n",
            iface.name, iface.rx_bytes, iface.rx_packets, iface.rx_errors, iface.rx_dropped,
            iface.tx_bytes, iface.tx_packets, iface.tx_errors, iface.tx_dropped));
    }
    s
}

/// Generate /proc/net/arp content
pub fn proc_net_arp() -> String {
    let mut s = String::new();
    s.push_str("IP address       HW type  Flags  HW address         Mask  Device\n");
    for entry in get_arp_table() {
        s.push_str(&format!("{:<16} 0x1      0x2    {}  *     {}\n",
            format_ip(&entry.ip), format_mac(&entry.mac), entry.iface));
    }
    s
}

/// Generate /proc/net/route content
pub fn proc_net_route() -> String {
    let mut s = String::new();
    s.push_str("Iface   Destination  Gateway      Flags  Metric  Mask\n");
    for r in get_routes() {
        let flags = if r.gateway == [0, 0, 0, 0] { "U" } else { "UG" };
        s.push_str(&format!("{:<8}{:<13}{:<13}{:<7}{:<8}{}\n",
            r.iface, format_ip(&r.destination), format_ip(&r.gateway),
            flags, r.metric, format_ip(&r.mask)));
    }
    s
}
