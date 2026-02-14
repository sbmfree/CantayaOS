// CantayaOS Shell — Network Commands

extern crate alloc;

use alloc::string::String;
use crate::graphics::console;
use core::fmt::Write;

pub(crate) fn cmd_ping(args: &str) {
    if args.is_empty() {
        console::println("Usage: ping <ip_address>");
        console::println("  Example: ping 10.0.2.2");
        return;
    }

    if !crate::net::is_up() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("Error: Network is not available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let target = match crate::net::parse_ip(args.trim()) {
        Some(ip) => ip,
        None => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Error: Invalid IP address format. Use x.x.x.x");
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    let mut s = String::new();
    write!(s, "\nPinging {}.{}.{}.{} with 64 bytes of data:",
        target[0], target[1], target[2], target[3]).ok();
    console::println(&s);

    let mut sent = 0u32;
    let mut received = 0u32;
    let mut min_rtt = u64::MAX;
    let mut max_rtt = 0u64;
    let mut total_rtt = 0u64;

    for i in 0..4u32 {
        sent += 1;

        match crate::net::icmp::ping(&target, 3000) {
            Some((rtt, reply)) => {
                received += 1;
                if rtt < min_rtt { min_rtt = rtt; }
                if rtt > max_rtt { max_rtt = rtt; }
                total_rtt += rtt;

                s.clear();
                write!(s, "Reply from {}.{}.{}.{}: bytes={} time={}ms seq={}",
                    reply.src_ip[0], reply.src_ip[1], reply.src_ip[2], reply.src_ip[3],
                    reply.data_len + 8, rtt, i + 1).ok();
                console::println(&s);
            }
            None => {
                console::println("Request timed out.");
            }
        }

        // Wait ~1 second between pings (except after the last one)
        if i < 3 {
            let start = crate::shell::ticks();
            loop {
                crate::net::poll();
                let elapsed = crate::shell::ticks().wrapping_sub(start);
                if crate::hal::pit::ticks_to_ms(elapsed) >= 1000 {
                    break;
                }
            }
        }
    }

    s.clear();
    write!(s, "\nPing statistics for {}.{}.{}.{}:",
        target[0], target[1], target[2], target[3]).ok();
    console::println(&s);

    let lost = sent - received;
    let pct = if sent > 0 { (lost as u64 * 100) / sent as u64 } else { 0 };
    s.clear();
    write!(s, "    Packets: Sent = {}, Received = {}, Lost = {} ({}% loss)",
        sent, received, lost, pct).ok();
    console::println(&s);

    if received > 0 {
        let avg_rtt = total_rtt / received as u64;
        s.clear();
        write!(s, "Approximate round trip times in milli-seconds:").ok();
        console::println(&s);
        s.clear();
        write!(s, "    Minimum = {}ms, Maximum = {}ms, Average = {}ms",
            min_rtt, max_rtt, avg_rtt).ok();
        console::println(&s);
    }
}

pub(crate) fn cmd_ip(args: &str) {
    if !crate::net::is_up() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("Error: Network is not available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let cfg = crate::net::config();

    if args.is_empty() {
        // Display current config (like ipconfig)
        console::set_color(0xFF, 0xFF, 0x55);
        console::println("\nCantayaOS IP Configuration\n");
        console::set_color(0xFF, 0xFF, 0xFF);
        console::println("Ethernet adapter virtio-net:\n");

        let mut s = String::new();
        write!(s, "   Physical Address. . . . . : {}", crate::net::format_mac(&cfg.mac)).ok();
        console::println(&s);

        s.clear();
        write!(s, "   IPv4 Address. . . . . . . : {}", crate::net::format_ip(&cfg.ip)).ok();
        console::println(&s);

        s.clear();
        write!(s, "   Subnet Mask . . . . . . . : {}", crate::net::format_ip(&cfg.netmask)).ok();
        console::println(&s);

        s.clear();
        write!(s, "   Default Gateway . . . . . : {}", crate::net::format_ip(&cfg.gateway)).ok();
        console::println(&s);
        console::println("");
        return;
    }

    // Parse set commands: ip set <ip|gateway|mask> <value>
    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.len() == 3 && parts[0] == "set" {
        let value = match crate::net::parse_ip(parts[2]) {
            Some(v) => v,
            None => {
                console::println("Error: Invalid IP address format.");
                return;
            }
        };
        match parts[1] {
            "ip" | "address" => {
                crate::net::set_ip(value);
                let mut s = String::new();
                write!(s, "IP address set to {}", crate::net::format_ip(&value)).ok();
                console::println(&s);
            }
            "gateway" | "gw" => {
                crate::net::set_gateway(value);
                let mut s = String::new();
                write!(s, "Gateway set to {}", crate::net::format_ip(&value)).ok();
                console::println(&s);
            }
            "mask" | "netmask" => {
                crate::net::set_netmask(value);
                let mut s = String::new();
                write!(s, "Netmask set to {}", crate::net::format_ip(&value)).ok();
                console::println(&s);
            }
            _ => {
                console::println("Usage: ip set <ip|gateway|mask> <value>");
            }
        }
    } else {
        console::println("Usage: ip                          Show IP configuration");
        console::println("       ip set ip <address>         Set IP address");
        console::println("       ip set gateway <address>    Set default gateway");
        console::println("       ip set mask <mask>          Set subnet mask");
    }
}

pub(crate) fn cmd_arp() {
    if !crate::net::is_up() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("Error: Network is not available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let entries = crate::net::arp::get_cache();

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nARP Cache:");
    console::set_color(0xFF, 0xFF, 0xFF);

    if entries.is_empty() {
        console::println("  (empty)");
    } else {
        console::println("  Internet Address      Physical Address       Type");
        console::println("  ────────────────────  ──────────────────────  ──────");
        for entry in &entries {
            let mut s = String::new();
            write!(s, "  {:>15}      {}      dynamic",
                crate::net::format_ip(&entry.ip),
                crate::net::format_mac(&entry.mac)).ok();
            console::println(&s);
        }
    }
    console::println("");
}

pub(crate) fn cmd_netstat() {
    if !crate::net::is_up() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("Error: Network is not available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let stats = crate::net::stats();
    let cfg = crate::net::config();

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nNetwork Statistics:\n");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();
    write!(s, "  Interface:  virtio-net").ok();
    console::println(&s);

    s.clear();
    write!(s, "  MAC:        {}", crate::net::format_mac(&cfg.mac)).ok();
    console::println(&s);

    s.clear();
    write!(s, "  IPv4:       {}", crate::net::format_ip(&cfg.ip)).ok();
    console::println(&s);

    console::println("");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("  Packet Statistics:");
    console::set_color(0xFF, 0xFF, 0xFF);

    s.clear();
    write!(s, "    TX Packets:     {}", stats.tx_packets).ok();
    console::println(&s);

    s.clear();
    write!(s, "    RX Packets:     {}", stats.rx_packets).ok();
    console::println(&s);

    s.clear();
    write!(s, "    TX Bytes:       {}", stats.tx_bytes).ok();
    console::println(&s);

    s.clear();
    write!(s, "    RX Bytes:       {}", stats.rx_bytes).ok();
    console::println(&s);

    console::println("");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("  Protocol Statistics:");
    console::set_color(0xFF, 0xFF, 0xFF);

    s.clear();
    write!(s, "    ARP Requests:   {}", stats.arp_requests).ok();
    console::println(&s);

    s.clear();
    write!(s, "    ARP Replies:    {}", stats.arp_replies).ok();
    console::println(&s);

    s.clear();
    write!(s, "    ICMP Sent:      {}", stats.icmp_sent).ok();
    console::println(&s);

    s.clear();
    write!(s, "    ICMP Received:  {}", stats.icmp_received).ok();
    console::println(&s);

    s.clear();
    write!(s, "    UDP Sent:       {}", stats.udp_sent).ok();
    console::println(&s);

    s.clear();
    write!(s, "    UDP Received:   {}", stats.udp_received).ok();
    console::println(&s);
    console::println("");
}
