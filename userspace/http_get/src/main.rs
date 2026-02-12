#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::arch::asm;
use libcantaya::{println, debug_print, exit};

/// Simple HTTP GET client.
///
/// Usage: http_get <ip> <port> <path>
///   e.g. http_get 93.184.216.34 80 /index.html
///
/// Uses CantayaOS TCP socket syscalls.
#[unsafe(no_mangle)]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    if argc < 4 {
        println!("Usage: http_get <ip> <port> <path>");
        println!("  e.g. http_get 93.184.216.34 80 /index.html");
        exit(1);
    }

    let ip_str = unsafe { cstr_to_str(*argv.add(1)) };
    let port_str = unsafe { cstr_to_str(*argv.add(2)) };
    let path = unsafe { cstr_to_str(*argv.add(3)) };

    // Parse IP address (simple a.b.c.d)
    let ip = match parse_ipv4(ip_str) {
        Some(ip) => ip,
        None => {
            println!("http_get: invalid IP '{}'", ip_str);
            exit(1);
        }
    };

    let port = match parse_u16(port_str) {
        Some(p) => p,
        None => {
            println!("http_get: invalid port '{}'", port_str);
            exit(1);
        }
    };

    println!("[http_get] Connecting to {}.{}.{}.{}:{} ...", ip[0], ip[1], ip[2], ip[3], port);

    // Create TCP socket
    let fd = libcantaya::net_socket();
    if fd == 0 || fd == u32::MAX {
        println!("http_get: failed to create socket");
        exit(1);
    }

    // Connect
    let rc = libcantaya::net_connect(fd, &ip, port);
    if rc != 0 {
        println!("http_get: connect failed ({})", rc);
        exit(1);
    }

    println!("[http_get] Connected! Sending GET request...");

    // Build HTTP request
    let mut req_buf = [0u8; 512];
    let req_len = format_http_get(&mut req_buf, ip_str, path);

    let sent = libcantaya::net_send(fd, &req_buf[..req_len]);
    if sent == 0 {
        println!("http_get: send failed");
        exit(1);
    }

    println!("[http_get] Request sent ({} bytes). Waiting for response...", sent);

    // Read response
    let mut buf = [0u8; 2048];
    let mut total = 0usize;
    // Give the remote a moment to respond
    libcantaya::sleep(500);

    loop {
        let n = libcantaya::net_recv(fd, &mut buf);
        if n == 0 {
            break;
        }
        // Print the data we received
        if let Ok(s) = core::str::from_utf8(&buf[..n]) {
            debug_print(s);
        }
        total += n;
        if total > 8192 {
            // Limit output
            println!("\n[http_get] (truncated at 8KB)");
            break;
        }
    }

    println!("\n[http_get] Received {} bytes total.", total);

    libcantaya::net_close(fd);
    exit(0);
}

fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let bytes = s.as_bytes();
    let mut result = [0u8; 4];
    let mut octet = 0u32;
    let mut idx = 0usize;

    for &b in bytes {
        if b == b'.' {
            if idx >= 3 || octet > 255 {
                return None;
            }
            result[idx] = octet as u8;
            octet = 0;
            idx += 1;
        } else if b >= b'0' && b <= b'9' {
            octet = octet * 10 + (b - b'0') as u32;
        } else {
            return None;
        }
    }

    if idx != 3 || octet > 255 {
        return None;
    }
    result[3] = octet as u8;
    Some(result)
}

fn parse_u16(s: &str) -> Option<u16> {
    let mut val = 0u32;
    for &b in s.as_bytes() {
        if b < b'0' || b > b'9' {
            return None;
        }
        val = val * 10 + (b - b'0') as u32;
        if val > 65535 {
            return None;
        }
    }
    Some(val as u16)
}

/// Format a minimal HTTP/1.0 GET request into buf, return length written.
fn format_http_get(buf: &mut [u8], host: &str, path: &str) -> usize {
    let mut pos = 0usize;

    let parts: &[&[u8]] = &[
        b"GET ", path.as_bytes(), b" HTTP/1.0\r\nHost: ",
        host.as_bytes(), b"\r\nConnection: close\r\n\r\n",
    ];

    for part in parts {
        let end = pos + part.len();
        if end > buf.len() {
            break;
        }
        buf[pos..end].copy_from_slice(part);
        pos = end;
    }

    pos
}

unsafe fn cstr_to_str<'a>(ptr: *const u8) -> &'a str {
    let mut len = 0usize;
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { core::slice::from_raw_parts(ptr, len) };
    core::str::from_utf8_unchecked(slice)
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug_print("[http_get] PANIC!\n");
    loop { unsafe { asm!("wfi"); } }
}
