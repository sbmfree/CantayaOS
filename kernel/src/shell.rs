// CantayaOS Kernel Shell (csh)
//
// An interactive command-line shell that runs in kernel mode during early boot.
// This is analogous to the Windows Recovery Console or Linux's init shell.
//
// Features:
//   - Command history (up/down arrows, 16 entries)
//   - Tab completion
//   - Colored output with changeable color schemes
//   - Memory/CPU/PCI/IRQ inspection
//   - Hex memory dump
//   - System information dashboard

extern crate alloc;

use alloc::string::String;
use crate::hal::keyboard::{self, KeyCode};
use crate::graphics::console;
use core::fmt::Write;

/// Maximum command line length
const MAX_CMD_LEN: usize = 256;

/// Command history depth
const HISTORY_SIZE: usize = 16;

/// System tick counter (incremented by the timer IRQ handler)
static TICK_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// IRQ counters for `interrupts` command
static IRQ_TIMER_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static IRQ_KEYBOARD_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// Increment the tick counter — called from the timer IRQ handler
pub fn timer_tick() {
    TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    IRQ_TIMER_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

/// Increment the keyboard IRQ counter
pub fn keyboard_irq() {
    IRQ_KEYBOARD_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

/// Get the current tick count
pub fn ticks() -> u64 {
    TICK_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// Hostname (configurable, persisted)
static HOSTNAME: spin::Mutex<[u8; 64]> = spin::Mutex::new([0u8; 64]);
static HOSTNAME_LEN: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(7);

fn get_hostname() -> String {
    let buf = HOSTNAME.lock();
    let len = HOSTNAME_LEN.load(core::sync::atomic::Ordering::Relaxed);
    if len == 0 {
        return String::from("cantaya");
    }
    core::str::from_utf8(&buf[..len]).unwrap_or("cantaya").into()
}

fn set_hostname_value(name: &str) {
    let mut buf = HOSTNAME.lock();
    let len = name.len().min(63);
    buf[..len].copy_from_slice(&name.as_bytes()[..len]);
    HOSTNAME_LEN.store(len, core::sync::atomic::Ordering::Relaxed);
}

/// Environment variables
use alloc::collections::BTreeMap;
use alloc::string::ToString;
static ENV_VARS: spin::Mutex<Option<BTreeMap<String, String>>> = spin::Mutex::new(None);

fn env_init() {
    let mut env = ENV_VARS.lock();
    if env.is_none() {
        let mut map = BTreeMap::new();
        map.insert("OS".into(), "CantayaOS".into());
        map.insert("ARCH".into(), "x86_64".into());
        map.insert("SHELL".into(), "csh".into());
        map.insert("USER".into(), "Root".into());
        map.insert("HOME".into(), "/Users/Root".into());
        map.insert("USERPROFILE".into(), "/Users/Root".into());
        map.insert("SYSTEMROOT".into(), "/Windows".into());
        map.insert("WINDIR".into(), "/Windows".into());
        map.insert("SYSTEMDRIVE".into(), "C:".into());
        map.insert("COMPUTERNAME".into(), "CANTAYAOS".into());
        map.insert("TEMP".into(), "/Windows/Temp".into());
        map.insert("TMP".into(), "/Windows/Temp".into());
        map.insert("PROGRAMFILES".into(), "/Programs".into());
        map.insert("PATH".into(), "/Windows/System32;/Windows;/Programs".into());
        let mut ver = String::new();
        write!(ver, "{}", env!("CARGO_PKG_VERSION")).ok();
        map.insert("VERSION".into(), ver);
        *env = Some(map);
    }
}

fn env_get(key: &str) -> Option<String> {
    env_init();
    let env = ENV_VARS.lock();
    env.as_ref().and_then(|m| m.get(key).cloned())
}

fn env_set(key: &str, value: &str) {
    env_init();
    let mut env = ENV_VARS.lock();
    if let Some(ref mut m) = *env {
        m.insert(key.into(), value.into());
    }
}

fn env_remove(key: &str) {
    env_init();
    let mut env = ENV_VARS.lock();
    if let Some(ref mut m) = *env {
        m.remove(key);
    }
}

/// Command aliases — user-defined shortcut names for commands
static ALIASES: spin::Mutex<Option<BTreeMap<String, String>>> = spin::Mutex::new(None);

fn aliases_init() {
    let mut a = ALIASES.lock();
    if a.is_none() {
        let mut map = BTreeMap::new();
        // Default aliases
        map.insert("ll".into(), "dir".into());
        map.insert("cls".into(), "clear".into());
        map.insert("quit".into(), "halt".into());
        map.insert("exit".into(), "halt".into());
        *a = Some(map);
    }
}

fn alias_get(name: &str) -> Option<String> {
    aliases_init();
    let a = ALIASES.lock();
    a.as_ref().and_then(|m| m.get(name).cloned())
}

fn alias_set(name: &str, value: &str) {
    aliases_init();
    let mut a = ALIASES.lock();
    if let Some(ref mut m) = *a {
        m.insert(name.into(), value.into());
    }
}

fn alias_remove(name: &str) {
    aliases_init();
    let mut a = ALIASES.lock();
    if let Some(ref mut m) = *a {
        m.remove(name);
    }
}

/// Static command history accessible from the `history` command
static CMD_HISTORY: spin::Mutex<History> = spin::Mutex::new(History::new());

/// Command history ring buffer
struct History {
    entries: [[u8; MAX_CMD_LEN]; HISTORY_SIZE],
    lengths: [usize; HISTORY_SIZE],
    count: usize,
    write_idx: usize,
    browse_idx: usize,
    browsing: bool,
}

impl History {
    const fn new() -> Self {
        Self {
            entries: [[0u8; MAX_CMD_LEN]; HISTORY_SIZE],
            lengths: [0; HISTORY_SIZE],
            count: 0,
            write_idx: 0,
            browse_idx: 0,
            browsing: false,
        }
    }

    fn push(&mut self, buf: &[u8], len: usize) {
        if len == 0 { return; }
        // Don't duplicate the last entry
        if self.count > 0 {
            let prev = if self.write_idx == 0 { HISTORY_SIZE - 1 } else { self.write_idx - 1 };
            if self.lengths[prev] == len && self.entries[prev][..len] == buf[..len] {
                return;
            }
        }
        self.entries[self.write_idx][..len].copy_from_slice(&buf[..len]);
        self.lengths[self.write_idx] = len;
        self.write_idx = (self.write_idx + 1) % HISTORY_SIZE;
        if self.count < HISTORY_SIZE { self.count += 1; }
        self.browsing = false;
    }

    fn start_browse(&mut self) {
        self.browse_idx = self.write_idx;
        self.browsing = true;
    }

    fn go_up(&mut self) -> Option<(&[u8], usize)> {
        if self.count == 0 { return None; }
        if !self.browsing { self.start_browse(); }

        let candidate = if self.browse_idx == 0 { HISTORY_SIZE - 1 } else { self.browse_idx - 1 };

        // Check bounds
        let oldest = if self.count < HISTORY_SIZE { 0 } else { self.write_idx };
        if candidate == oldest && self.browse_idx == oldest {
            return None;
        }

        self.browse_idx = candidate;
        let len = self.lengths[self.browse_idx];
        if len == 0 { return None; }
        Some((&self.entries[self.browse_idx][..len], len))
    }

    fn go_down(&mut self) -> Option<(&[u8], usize)> {
        if !self.browsing || self.count == 0 { return None; }

        let next = (self.browse_idx + 1) % HISTORY_SIZE;
        if next == self.write_idx {
            self.browsing = false;
            return Some((&[], 0));
        }
        self.browse_idx = next;
        let len = self.lengths[self.browse_idx];
        Some((&self.entries[self.browse_idx][..len], len))
    }
}

/// Run the interactive kernel shell (never returns).
pub fn run() -> ! {
    // Load saved theme before the banner so it renders with the right colors
    load_saved_theme();

    // Play startup chime
    crate::hal::speaker::startup_chime();

    // Run startup script from disk
    run_autoexec();

    print_banner();

    let mut cmd_buf = [0u8; MAX_CMD_LEN];
    let mut cmd_len: usize = 0;

    print_prompt();

    // Track last status bar update tick for periodic refresh
    let mut last_status_tick: u64 = 0;
    // Cursor blink state
    let mut cursor_visible = true;
    let mut last_cursor_tick: u64 = 0;
    const CURSOR_BLINK_INTERVAL: u64 = 530; // ~530ms per phase (standard blink rate)

    loop {
        // Periodically refresh the status bar (every ~1000 ticks = 1 second)
        let now = ticks();
        if now.wrapping_sub(last_status_tick) >= 1000 {
            last_status_tick = now;
            update_status_bar();
        }

        // Blink cursor
        if now.wrapping_sub(last_cursor_tick) >= CURSOR_BLINK_INTERVAL {
            last_cursor_tick = now;
            cursor_visible = !cursor_visible;
            console::draw_cursor(cursor_visible);
        }

        // Non-blocking key poll — if no key, HLT until next interrupt and retry
        let event = match keyboard::try_read_char() {
            Some(e) => e,
            None => {
                unsafe { core::arch::asm!("hlt"); }
                continue;
            }
        };

        // Key pressed — ensure cursor is visible & reset blink timer
        if !cursor_visible {
            console::draw_cursor(false); // erase old cursor before typing
        }
        cursor_visible = true;
        last_cursor_tick = now;

        match event.ascii {
            // Enter — execute command
            b'\n' => {
                console::print("\n");
                if cmd_len > 0 {
                    CMD_HISTORY.lock().push(&cmd_buf, cmd_len);
                    let cmd = core::str::from_utf8(&cmd_buf[..cmd_len]).unwrap_or("");
                    execute_command(cmd);
                    cmd_len = 0;
                }
                print_prompt();
            }

            // Backspace
            0x08 => {
                if cmd_len > 0 {
                    cmd_len -= 1;
                    console::backspace();
                }
            }

            // Ctrl+C — cancel current line
            0x03 => {
                console::print("^C\n");
                cmd_len = 0;
                print_prompt();
            }

            // Ctrl+L — clear screen
            0x0C => {
                console::clear();
                cmd_len = 0;
                print_prompt();
            }

            // Printable ASCII
            c @ 0x20..=0x7E => {
                if cmd_len < MAX_CMD_LEN - 1 {
                    cmd_buf[cmd_len] = c;
                    cmd_len += 1;
                    let ch = c as char;
                    let mut buf = [0u8; 4];
                    console::print(ch.encode_utf8(&mut buf));
                }
            }

            // Tab — auto-complete
            b'\t' => {
                if cmd_len > 0 {
                    let partial = core::str::from_utf8(&cmd_buf[..cmd_len]).unwrap_or("");
                    if let Some(match_cmd) = tab_complete(partial) {
                        for _ in 0..cmd_len { console::backspace(); }
                        let bytes = match_cmd.as_bytes();
                        cmd_buf[..bytes.len()].copy_from_slice(bytes);
                        cmd_len = bytes.len();
                        console::print(match_cmd);
                    }
                }
            }

            // Non-printable — handle special keys
            _ => {
                match event.key {
                    KeyCode::Up => {
                        let mut hist = CMD_HISTORY.lock();
                        if let Some((entry, len)) = hist.go_up() {
                            let mut tmp = [0u8; MAX_CMD_LEN];
                            tmp[..len].copy_from_slice(entry);
                            drop(hist);
                            for _ in 0..cmd_len { console::backspace(); }
                            cmd_buf[..len].copy_from_slice(&tmp[..len]);
                            cmd_len = len;
                            if let Ok(s) = core::str::from_utf8(&cmd_buf[..cmd_len]) {
                                console::print(s);
                            }
                        }
                    }
                    KeyCode::Down => {
                        let mut hist = CMD_HISTORY.lock();
                        if let Some((entry, len)) = hist.go_down() {
                            let mut tmp = [0u8; MAX_CMD_LEN];
                            tmp[..len].copy_from_slice(entry);
                            drop(hist);
                            for _ in 0..cmd_len { console::backspace(); }
                            cmd_buf[..len].copy_from_slice(&tmp[..len]);
                            cmd_len = len;
                            if let Ok(s) = core::str::from_utf8(&cmd_buf[..cmd_len]) {
                                console::print(s);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Simple tab completion — returns the first command matching the prefix.
fn tab_complete(prefix: &str) -> Option<&'static str> {
    const COMMANDS: &[&str] = &[
        "acpi", "alias", "append", "arp", "banner", "beep", "bootinfo", "cal",
        "calc", "cat", "cd", "chdir", "clear", "cls", "color", "copy", "cp",
        "cpu", "date", "del", "desktop", "dir", "disk", "dmesg", "drivers",
        "echo", "edit", "env", "exec", "find", "fortune", "free", "grep",
        "halt", "head", "help", "hexdump", "history", "hostname", "interrupts",
        "ip", "ipconfig", "irq", "kill", "ls", "lsblk", "lspci", "md", "mem",
        "memory", "memmap", "mkdir", "more", "mv", "neofetch", "netstat",
        "panic", "pci", "ping", "poweroff", "priority", "ps", "pwd", "reboot",
        "rename", "rm", "run", "screenfetch", "set", "shutdown", "sleep",
        "sort", "source", "spawn", "stat", "sysinfo", "tail", "tasks", "tick",
        "time", "top", "touch", "tree", "type", "unalias", "uniq", "unset",
        "uptime", "ver", "version", "vol", "wc", "which", "whoami", "write",
        "xxd", "yield",
    ];
    COMMANDS.iter().find(|cmd| cmd.starts_with(prefix)).copied()
}

fn print_prompt() {
    update_status_bar();
    // Windows-style prompt: C:\path>
    let cwd = crate::storage::vfs::cwd();
    // Convert Unix-style path to Windows-style: / → C:\
    let win_path = unix_to_win_path(&cwd);
    console::set_color(0xFF, 0xFF, 0xFF);
    console::print(&win_path);
    console::print(">");
}

/// Convert a Unix-style VFS path to a Windows-style display path.
///
/// Examples:
///   "/"                  → "C:\\"
///   "/Users/Root"        → "C:\\Users\\Root"
///   "/Windows/System32"  → "C:\\Windows\\System32"
fn unix_to_win_path(path: &str) -> String {
    if path == "/" {
        return String::from("C:\\");
    }
    let mut result = String::from("C:");
    for ch in path.chars() {
        if ch == '/' {
            result.push('\\');
        } else {
            result.push(ch);
        }
    }
    result
}

/// Update the persistent status bar at the bottom of the screen.
fn update_status_bar() {
    let tick_count = ticks();
    let ms = crate::hal::pit::ticks_to_ms(tick_count);
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    let tasks = crate::core_kernel::scheduler::active_task_count();
    let ctx = crate::core_kernel::scheduler::context_switch_count();
    let free_mib = crate::memory::frame_allocator::free_frame_count() * 4 / 1024;

    let mut bar = String::new();
    if hours > 0 {
        write!(bar, " Up {}h{:02}m{:02}s", hours, minutes % 60, seconds % 60).ok();
    } else if minutes > 0 {
        write!(bar, " Up {}m{:02}s", minutes, seconds % 60).ok();
    } else {
        write!(bar, " Up {}s", seconds).ok();
    }
    write!(bar, " | Tasks: {} | Ctx: {} | Free: {} MiB", tasks, ctx, free_mib).ok();

    // Pad to fill the bar
    let (cols, _) = console::dimensions();
    while bar.len() < cols {
        bar.push(' ');
    }

    console::draw_status_bar(&bar);
}

fn print_banner() {
    console::set_color(0x00, 0xAA, 0xFF);
    console::println("  ____            _                    ___  ____  ");
    console::println(" / ___|__ _ _ __ | |_ __ _ _   _  __ / _ \\/ ___| ");
    console::println("| |   / _` | '_ \\| __/ _` | | | |/ _` | | \\___ \\ ");
    console::println("| |__| (_| | | | | || (_| | |_| | (_| | |  ___) |");
    console::println(" \\____\\__,_|_| |_|\\__\\__,_|\\__, |\\__,_|_| |____/ ");
    console::println("                           |___/                  ");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("");

    let mut s = String::new();
    write!(s, "CantayaOS [Version {}]", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("(c) CantayaOS Contributors. All rights reserved.");
    console::println("");

    let total_mem_mib = crate::memory::frame_allocator::free_frame_count() * 4 / 1024;
    let mut info = String::new();
    write!(info, "{} MiB usable RAM | x86_64 | FAT32 volume CANTAYAOS", total_mem_mib).ok();
    console::set_color(0xAA, 0xAA, 0xAA);
    console::println(&info);
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("Type 'help' for available commands.\n");
}

/// Expand %VAR% style environment variables in a command string.
fn expand_variables(input: &str) -> String {
    env_init();
    let mut result = String::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            // Find closing %
            if let Some(end) = input[i+1..].find('%') {
                let var_name = &input[i+1..i+1+end];
                if let Some(value) = env_get(var_name) {
                    result.push_str(&value);
                } else {
                    // Keep the original %VAR% if not found
                    result.push('%');
                    result.push_str(var_name);
                    result.push('%');
                }
                i += end + 2;
                continue;
            }
        }
        result.push(bytes[i] as char);
        i += 1;
    }
    result
}

/// Execute a shell command.
fn execute_command(input: &str) {
    let input = input.trim();
    if input.is_empty() { return; }

    // Expand environment variables (%VAR% style)
    let expanded = expand_variables(input);
    let input = expanded.trim();

    // Support command chaining with ';'
    if input.contains(';') && !input.contains('|') {
        for part in input.split(';') {
            let part = part.trim();
            if !part.is_empty() {
                execute_with_redirect(part);
            }
        }
        return;
    }

    // Support piping with '|'
    if input.contains('|') {
        execute_pipeline(input);
        return;
    }

    execute_with_redirect(input);
}

/// Handle output redirection (> and >>).
fn execute_with_redirect(input: &str) {
    use crate::storage::vfs;

    // Check for append redirection >>
    if let Some(pos) = input.find(">>") {
        let cmd_part = input[..pos].trim();
        let file_part = input[pos+2..].trim();
        if !file_part.is_empty() && !cmd_part.is_empty() {
            console::start_capture();
            execute_single_command(cmd_part);
            let output = console::stop_capture();
            // Append to file
            let path = resolve_path(file_part);
            let existing = vfs::read_file(&path).unwrap_or_default();
            let mut new_data = existing;
            new_data.extend_from_slice(output.as_bytes());
            vfs::write_file(&path, &new_data);
            return;
        }
    }

    // Check for overwrite redirection >
    if let Some(pos) = input.find('>') {
        // Make sure it's not >>
        if pos + 1 < input.len() && input.as_bytes()[pos + 1] != b'>' {
            let cmd_part = input[..pos].trim();
            let file_part = input[pos+1..].trim();
            if !file_part.is_empty() && !cmd_part.is_empty() {
                console::start_capture();
                execute_single_command(cmd_part);
                let output = console::stop_capture();
                let path = resolve_path(file_part);
                vfs::write_file(&path, output.as_bytes());
                return;
            }
        }
    }

    execute_single_command(input);
}

/// Execute a pipeline of commands (cmd1 | cmd2 | cmd3).
fn execute_pipeline(input: &str) {
    let commands: alloc::vec::Vec<&str> = input.split('|').collect();
    if commands.is_empty() { return; }

    // Execute first command with capture
    let first = commands[0].trim();
    if first.is_empty() { return; }

    console::start_capture();
    execute_single_command(first);
    let mut pipe_data = console::stop_capture();

    // For each subsequent command, feed the captured output as "piped input"
    for cmd_str in &commands[1..] {
        let cmd_str = cmd_str.trim();
        if cmd_str.is_empty() { continue; }

        // Parse pipe-receiving commands that can accept piped input
        let (cmd, args) = match cmd_str.find(' ') {
            Some(pos) => (&cmd_str[..pos], cmd_str[pos + 1..].trim()),
            None => (cmd_str, ""),
        };

        let result = match cmd {
            "grep" => pipe_grep(&pipe_data, args),
            "head" => pipe_head(&pipe_data, args),
            "tail" => pipe_tail(&pipe_data, args),
            "wc" => pipe_wc(&pipe_data),
            "sort" => pipe_sort(&pipe_data),
            "uniq" => pipe_uniq(&pipe_data),
            "more" => { pipe_more(&pipe_data); String::new() }
            _ => {
                // For unsupported pipe targets, just print the input
                console::print(&pipe_data);
                String::new()
            }
        };
        pipe_data = result;
    }

    // Print final output if any
    if !pipe_data.is_empty() {
        console::print(&pipe_data);
    }
}

/// Pipe-compatible grep: filter lines matching pattern.
fn pipe_grep(input: &str, pattern: &str) -> String {
    let mut result = String::new();
    for line in input.lines() {
        if line.contains(pattern) {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Pipe-compatible head: first N lines.
fn pipe_head(input: &str, args: &str) -> String {
    let n: usize = args.trim().parse().unwrap_or(10);
    let mut result = String::new();
    for (i, line) in input.lines().enumerate() {
        if i >= n { break; }
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Pipe-compatible tail: last N lines.
fn pipe_tail(input: &str, args: &str) -> String {
    let n: usize = args.trim().parse().unwrap_or(10);
    let lines: alloc::vec::Vec<&str> = input.lines().collect();
    let start = if lines.len() > n { lines.len() - n } else { 0 };
    let mut result = String::new();
    for line in &lines[start..] {
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Pipe-compatible wc: count lines/words/chars.
fn pipe_wc(input: &str) -> String {
    let lines = input.lines().count();
    let words = input.split_whitespace().count();
    let chars = input.len();
    let mut s = String::new();
    write!(s, "  {} lines, {} words, {} bytes", lines, words, chars).ok();
    s.push('\n');
    s
}

/// Pipe-compatible sort: sort lines alphabetically.
fn pipe_sort(input: &str) -> String {
    let mut lines: alloc::vec::Vec<&str> = input.lines().collect();
    lines.sort();
    let mut result = String::new();
    for line in lines {
        result.push_str(line);
        result.push('\n');
    }
    result
}

/// Pipe-compatible uniq: remove consecutive duplicate lines.
fn pipe_uniq(input: &str) -> String {
    let mut result = String::new();
    let mut prev: Option<&str> = None;
    for line in input.lines() {
        if prev != Some(line) {
            result.push_str(line);
            result.push('\n');
        }
        prev = Some(line);
    }
    result
}

/// Pipe-compatible more: display text one page at a time.
fn pipe_more(input: &str) {
    let (_, rows) = console::dimensions();
    let page_size = rows - 2; // Leave room for prompt
    let lines: alloc::vec::Vec<&str> = input.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let end = (i + page_size).min(lines.len());
        for line in &lines[i..end] {
            console::println(line);
        }
        i = end;
        if i < lines.len() {
            console::set_color(0x00, 0x00, 0x00);
            console::print("-- More -- (SPACE=next page, Q=quit)");
            console::set_color(0xFF, 0xFF, 0xFF);
            loop {
                if let Some(key) = keyboard::try_read_char() {
                    if key.pressed {
                        if key.ascii == b' ' { break; }
                        if key.ascii == b'q' || key.ascii == b'Q' {
                            console::println("");
                            return;
                        }
                        if key.key == KeyCode::Enter { 
                            // Advance one line
                            i = i.wrapping_sub(page_size - 1);
                            break;
                        }
                    }
                }
            }
            // Clear the "More" prompt
            console::print("\r");
            let blank = "                                     ";
            console::print(blank);
            console::print("\r");
        }
    }
}

fn execute_single_command(input: &str) {
    let input = input.trim();
    if input.is_empty() { return; }

    let (cmd, args) = match input.find(' ') {
        Some(pos) => (&input[..pos], input[pos + 1..].trim()),
        None => (input, ""),
    };

    match cmd {
        "help" | "?" => cmd_help(),
        "ver" | "version" => cmd_version(),
        "mem" | "memory" => cmd_memory(),
        "cpu" => cmd_cpu(),
        "uptime" => cmd_uptime(),
        "clear" | "cls" => cmd_clear(),
        "echo" => cmd_echo(args),
        "halt" | "shutdown" | "poweroff" => cmd_halt(),
        "reboot" => cmd_reboot(),
        "tasks" | "ps" => cmd_tasks(),
        "spawn" => cmd_spawn(args),
        "kill" => cmd_kill(args),
        "yield" => cmd_yield(),
        "tick" => cmd_tick(),
        "interrupts" | "irq" => cmd_interrupts(),
        "hexdump" => cmd_hexdump(args),
        "pci" | "lspci" => cmd_pci(),
        "color" => cmd_color(args),
        "sysinfo" => cmd_sysinfo(),
        "date" => cmd_date(),
        "desktop" => cmd_desktop(),
        "acpi" => cmd_acpi(),
        "sleep" => cmd_sleep(args),
        "priority" => cmd_priority(args),
        "bootinfo" => cmd_bootinfo(),
        "memmap" => cmd_memmap(),
        "panic" => cmd_panic(),
        // Filesystem
        "ls" | "dir" => cmd_ls(args),
        "cat" | "type" => cmd_cat(args),
        "write" => cmd_write(args),
        "mkdir" | "md" => cmd_mkdir(args),
        "rm" | "del" => cmd_rm(args),
        "cp" | "copy" => cmd_cp(args),
        "disk" => cmd_disk(),
        "cd" | "chdir" => cmd_cd(args),
        "pwd" => cmd_pwd(),
        "vol" => cmd_vol(),
        // New commands
        "beep" => cmd_beep(args),
        "hostname" => cmd_hostname(args),
        "whoami" => cmd_whoami(),
        "history" => cmd_history(),
        "grep" => cmd_grep(args),
        "find" => cmd_find(args),
        "wc" => cmd_wc(args),
        "head" => cmd_head(args),
        "tail" => cmd_tail(args),
        "touch" => cmd_touch(args),
        "stat" => cmd_stat(args),
        "tree" => cmd_tree(args),
        "cal" => cmd_cal(),
        "fortune" => cmd_fortune(),
        "banner" => cmd_banner(args),
        "env" | "set" => cmd_env(args),
        "unset" => cmd_unset(args),
        "edit" => cmd_edit(args),
        "append" => cmd_append(args),
        "rename" | "mv" => cmd_rename(args),
        "xxd" => cmd_xxd(args),
        // Networking
        "ping" => cmd_ping(args),
        "ip" | "ipconfig" => cmd_ip(args),
        "arp" => cmd_arp(),
        "netstat" => cmd_netstat(),
        // Shell power-ups
        "alias" => cmd_alias(args),
        "unalias" => cmd_unalias(args),
        "run" | "exec" | "source" => cmd_run(args),
        "time" => cmd_time(args),
        "sort" => cmd_sort(args),
        "uniq" => cmd_uniq(args),
        "more" => cmd_more(args),
        "which" => cmd_which(args),
        "calc" => cmd_calc(args),
        // System internals
        "dmesg" => cmd_dmesg(args),
        "free" => cmd_free(),
        "top" => cmd_top(),
        "drivers" => cmd_drivers(),
        "neofetch" | "screenfetch" => cmd_neofetch(),
        "lsblk" => cmd_lsblk(),
        _ => {
            // Check aliases before reporting unknown command
            if let Some(alias_cmd) = alias_get(cmd) {
                let full_cmd = if args.is_empty() {
                    alias_cmd
                } else {
                    let mut c = alias_cmd;
                    c.push(' ');
                    c.push_str(args);
                    c
                };
                execute_single_command(&full_cmd);
            } else {
                let mut s = String::new();
                write!(s, "'{}' is not recognized as an internal or external command.", cmd).ok();
                console::set_color(0xFF, 0x55, 0x55);
                console::println(&s);
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }
    }
}

// ============================================================================
// Command Implementations
// ============================================================================

fn cmd_help() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("System Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  help             Show this help message");
    console::println("  ver              Display kernel version");
    console::println("  sysinfo          System information dashboard");
    console::println("  mem              Display memory statistics");
    console::println("  memmap           Show memory map regions");
    console::println("  cpu              Display CPU information");
    console::println("  date             Show current date and time (RTC)");
    console::println("  uptime           Show system uptime");
    console::println("  hostname [name]  Show/set hostname");
    console::println("  whoami           Show current user");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nProcess Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  tasks            List active tasks with priority/CPU");
    console::println("  spawn <task>     Spawn a task (counter/spinner/stress)");
    console::println("  kill <id>        Terminate a task by ID");
    console::println("  priority <id> <p> Set task priority (idle/low/normal/high/rt)");
    console::println("  sleep <ms>       Sleep current task for N milliseconds");
    console::println("  yield            Yield current time slice");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nHardware Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  pci / lspci      Enumerate PCI devices");
    console::println("  acpi             Show ACPI information");
    console::println("  interrupts       Show IRQ statistics");
    console::println("  bootinfo         Show boot information");
    console::println("  hexdump <a> [n]  Dump n bytes at hex address a");
    console::println("  beep [freq] [ms] Play a tone via PC speaker");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nFilesystem Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  dir [path]       List directory contents (alias: ls)");
    console::println("  type <file>      Display file contents (alias: cat)");
    console::println("  write <f> <text> Write text to a file");
    console::println("  append <f> <txt> Append text to a file");
    console::println("  edit <file>      Open file in text editor");
    console::println("  touch <file>     Create an empty file");
    console::println("  md <dir>         Create a directory (alias: mkdir)");
    console::println("  del <file|dir>   Delete a file or empty dir (alias: rm)");
    console::println("  copy <src> <dst> Copy a file (alias: cp)");
    console::println("  rename <s> <d>   Rename/move a file (alias: mv)");
    console::println("  cd <dir>         Change working directory");
    console::println("  cd               Print working directory");
    console::println("  disk             Show disk/filesystem info");
    console::println("  stat <file>      Show file information");
    console::println("  tree [path]      Show directory tree");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nText Processing:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  grep <pat> <f>   Search for pattern in file");
    console::println("  find <name>      Find files by name");
    console::println("  wc <file>        Count lines/words/chars");
    console::println("  head <file> [n]  Show first n lines (default 10)");
    console::println("  tail <file> [n]  Show last n lines (default 10)");
    console::println("  xxd <file>       Hex dump a file");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nShell & Environment:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  echo <msg>       Echo text to console");
    console::println("  history          Show command history");
    console::println("  env / set        Show/set environment variables");
    console::println("  unset <var>      Remove an environment variable");
    console::println("  cal              Show calendar");
    console::println("  fortune          Random wisdom");
    console::println("  banner <text>    ASCII art banner");
    console::println("  color <scheme>   Change color (green/amber/white/blue/default)");
    console::println("  clear            Clear the screen");
    console::println("  desktop          Launch the graphical desktop environment");
    console::println("  halt / poweroff  Shut down the system (ACPI S5)");
    console::println("  reboot           Reboot the system");
    console::println("  panic            Trigger a kernel panic (for testing BSOD)");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nNetwork Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  ping <ip>        Send ICMP echo requests");
    console::println("  ip / ipconfig    Show/set network configuration");
    console::println("  arp              Show ARP cache");
    console::println("  netstat          Show network statistics");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nSystem Internals:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  dmesg [n]        Show kernel log (last n entries)");
    console::println("  free             Detailed memory usage report");
    console::println("  top              Interactive task monitor (Q to quit)");
    console::println("  drivers          List loaded drivers and subsystems");
    console::println("  neofetch         System info with ASCII art");
    console::println("  lsblk            List block devices");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nShell Power-ups:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  alias [n=cmd]    Define/list command aliases");
    console::println("  unalias <name>   Remove a command alias");
    console::println("  run <script>     Execute a script file");
    console::println("  time <cmd>       Measure command execution time");
    console::println("  sort <file>      Sort file lines alphabetically");
    console::println("  uniq <file>      Remove consecutive duplicate lines");
    console::println("  more <file>      View file with paged scrolling");
    console::println("  which <cmd>      Check if a command exists");
    console::println("  calc <expr>      Integer calculator (+,-,*,/,%)");
    console::println("  cmd > file       Redirect output to file");
    console::println("  cmd >> file      Append output to file");
    console::println("  cmd1 | cmd2      Pipe output between commands");
    console::println("  %VAR%            Expand environment variables");
    console::set_color(0xAA, 0xAA, 0xAA);
    console::println("\nTip: Up/Down = history | Tab = completion | ; = chain commands");
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_version() {
    let mut s = String::new();
    console::println("");
    write!(s, "CantayaOS [Version {}]", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("Architecture: x86_64 (AMD64)");
    console::println("Kernel: Hybrid (Rust, no_std bare-metal)");
    console::println("Filesystem: FAT32 with Windows-like hierarchy");
    console::println("Boot: UEFI");
}

fn cmd_memory() {
    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Memory Statistics:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();
    write!(s, "  Physical frames:  {} total, {} used, {} free",
        total_frames, used_frames, free_frames).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Physical memory:  {} MiB total, {} MiB used, {} MiB free",
        total_frames * 4 / 1024, used_frames * 4 / 1024, free_frames * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Page size:        4 KiB").ok();
    console::println(&s);

    s.clear();
    write!(s, "  Kernel heap:      {} KiB", crate::memory::heap::HEAP_SIZE / 1024).ok();
    console::println(&s);

    // Usage bar
    let bar_width = 40usize;
    let used_bar = if total_frames > 0 { (used_frames * bar_width) / total_frames } else { 0 };
    let free_bar = bar_width - used_bar;

    s.clear();
    write!(s, "  [").ok();
    for _ in 0..used_bar { write!(s, "#").ok(); }
    for _ in 0..free_bar { write!(s, ".").ok(); }
    write!(s, "] {}%", if total_frames > 0 { used_frames * 100 / total_frames } else { 0 }).ok();
    console::println(&s);
}

fn cmd_cpu() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("CPU Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    // CPUID vendor string
    let (vendor_ebx, vendor_ecx, vendor_edx) = unsafe {
        let ebx: u32; let ecx: u32; let edx: u32;
        core::arch::asm!(
            "push rbx", "cpuid", "mov {ebx:e}, ebx", "pop rbx",
            in("eax") 0u32,
            ebx = out(reg) ebx, out("ecx") ecx, out("edx") edx,
        );
        (ebx, ecx, edx)
    };

    let mut vendor = [0u8; 12];
    vendor[0..4].copy_from_slice(&vendor_ebx.to_le_bytes());
    vendor[4..8].copy_from_slice(&vendor_edx.to_le_bytes());
    vendor[8..12].copy_from_slice(&vendor_ecx.to_le_bytes());
    let vendor_str = core::str::from_utf8(&vendor).unwrap_or("Unknown");

    let mut s = String::new();
    write!(s, "  Vendor:    {}", vendor_str).ok();
    console::println(&s);

    // Brand string (CPUID 0x80000002..04)
    let max_ext: u32 = unsafe {
        let eax: u32;
        core::arch::asm!(
            "push rbx", "cpuid", "pop rbx",
            in("eax") 0x80000000u32,
            lateout("eax") eax, out("ecx") _, out("edx") _,
        );
        eax
    };

    if max_ext >= 0x80000004 {
        let mut brand = [0u8; 48];
        for i in 0..3u32 {
            let leaf = 0x80000002 + i;
            let (a, b, c, d) = unsafe {
                let a: u32; let b: u32; let c: u32; let d: u32;
                core::arch::asm!(
                    "push rbx", "cpuid", "mov {b:e}, ebx", "pop rbx",
                    in("eax") leaf,
                    lateout("eax") a, b = out(reg) b, out("ecx") c, out("edx") d,
                );
                (a, b, c, d)
            };
            let off = (i as usize) * 16;
            brand[off..off+4].copy_from_slice(&a.to_le_bytes());
            brand[off+4..off+8].copy_from_slice(&b.to_le_bytes());
            brand[off+8..off+12].copy_from_slice(&c.to_le_bytes());
            brand[off+12..off+16].copy_from_slice(&d.to_le_bytes());
        }
        let brand_str = core::str::from_utf8(&brand).unwrap_or("").trim_end_matches('\0').trim();
        if !brand_str.is_empty() {
            s.clear();
            write!(s, "  Model:     {}", brand_str).ok();
            console::println(&s);
        }
    }

    // CPUID leaf 1 — family/model/features
    let (family_model, features_edx, features_ecx) = unsafe {
        let eax: u32; let ecx: u32; let edx: u32;
        core::arch::asm!(
            "push rbx", "cpuid", "pop rbx",
            in("eax") 1u32,
            out("ecx") ecx, out("edx") edx, lateout("eax") eax,
        );
        (eax, edx, ecx)
    };

    let stepping = family_model & 0xF;
    let model = ((family_model >> 4) & 0xF) | (((family_model >> 16) & 0xF) << 4);
    let family = ((family_model >> 8) & 0xF) + ((family_model >> 20) & 0xFF);

    s.clear();
    write!(s, "  Family:    {} Model: {} Stepping: {}", family, model, stepping).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Mode:      64-bit Long Mode (x86_64)").ok();
    console::println(&s);

    // Feature flags
    s.clear();
    write!(s, "  Features: ").ok();
    if features_edx & (1 << 0) != 0 { write!(s, " FPU").ok(); }
    if features_edx & (1 << 23) != 0 { write!(s, " MMX").ok(); }
    if features_edx & (1 << 25) != 0 { write!(s, " SSE").ok(); }
    if features_edx & (1 << 26) != 0 { write!(s, " SSE2").ok(); }
    if features_ecx & (1 << 0) != 0 { write!(s, " SSE3").ok(); }
    if features_ecx & (1 << 9) != 0 { write!(s, " SSSE3").ok(); }
    if features_ecx & (1 << 19) != 0 { write!(s, " SSE4.1").ok(); }
    if features_ecx & (1 << 20) != 0 { write!(s, " SSE4.2").ok(); }
    if features_ecx & (1 << 28) != 0 { write!(s, " AVX").ok(); }
    if features_ecx & (1 << 25) != 0 { write!(s, " AES-NI").ok(); }
    console::println(&s);

    let cr3 = crate::hal::cpu::read_cr3();
    s.clear();
    write!(s, "  CR3:       {:#X} (PML4)", cr3).ok();
    console::println(&s);
}

fn cmd_uptime() {
    let tick_count = ticks();
    let ms = crate::hal::pit::ticks_to_ms(tick_count);
    let seconds = ms / 1000;
    let minutes = seconds / 60;
    let hours = minutes / 60;

    let mut s = String::new();
    if hours > 0 {
        write!(s, "Uptime: {}h {}m {}s ({} ticks, {} ms)",
            hours, minutes % 60, seconds % 60, tick_count, ms).ok();
    } else if minutes > 0 {
        write!(s, "Uptime: {}m {}.{}s ({} ticks)",
            minutes, seconds % 60, (ms % 1000) / 100, tick_count).ok();
    } else {
        write!(s, "Uptime: {}.{}s ({} ticks)",
            seconds, (ms % 1000) / 100, tick_count).ok();
    }
    console::println(&s);
}

fn cmd_tick() {
    let mut s = String::new();
    let rate = crate::hal::pit::tick_rate_hz();
    write!(s, "Timer ticks: {} ({} Hz, {} ms/tick)", ticks(), rate,
        if rate > 0 { 1000 / rate } else { 0 }).ok();
    console::println(&s);
}

fn cmd_clear() {
    console::clear();
}

fn cmd_echo(args: &str) {
    if args.is_empty() {
        console::println("");
    } else {
        console::println(args);
    }
}

fn cmd_tasks() {
    use crate::core_kernel::scheduler;

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Active Tasks:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  ID  State       Priority  Switches   CPU Ticks  Name");
    console::println("  --  -----       --------  --------   ---------  ----");

    let tasks = scheduler::task_list();
    let mut s = String::new();
    for (id, state, name, switches, priority, cpu_ticks) in &tasks {
        let state_str = match state {
            scheduler::TaskState::Running => "Running ",
            scheduler::TaskState::Ready   => "Ready   ",
            scheduler::TaskState::Blocked => "Blocked ",
            _ => "Unknown ",
        };
        s.clear();
        write!(s, "  {:3} {}  {:8}  {:>8}   {:>9}  {}", id, state_str, priority.name(), switches, cpu_ticks, name).ok();
        console::println(&s);
    }

    s.clear();
    write!(s, "\n  {} task(s), {} context switches",
        scheduler::active_task_count(),
        scheduler::context_switch_count()).ok();
    console::println(&s);
}

fn cmd_spawn(args: &str) {
    use crate::core_kernel::scheduler;

    let task_name = args.trim();
    if task_name.is_empty() {
        console::println("Usage: spawn <task>");
        console::println("  Tasks: counter, spinner, stress");
        return;
    }

    let (entry, name): (fn() -> !, &str) = match task_name {
        "counter" => (demo_task_counter, "counter"),
        "spinner" => (demo_task_spinner, "spinner"),
        "stress"  => (demo_task_stress, "stress"),
        _ => {
            let mut s = String::new();
            write!(s, "Unknown task '{}'. Available: counter, spinner, stress", task_name).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    match scheduler::spawn(name, entry) {
        Some(id) => {
            let mut s = String::new();
            console::set_color(0x55, 0xFF, 0x55);
            write!(s, "Spawned task '{}' with ID {}", name, id).ok();
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
        None => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Failed to spawn task (no slots or out of memory)");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_kill(args: &str) {
    use crate::core_kernel::scheduler;

    let id_str = args.trim();
    if id_str.is_empty() {
        console::println("Usage: kill <task_id>");
        return;
    }

    match id_str.parse::<u32>() {
        Ok(0) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Cannot kill the kernel task (ID 0)");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
        Ok(id) => {
            if scheduler::kill(id) {
                let mut s = String::new();
                console::set_color(0x55, 0xFF, 0x55);
                write!(s, "Task {} terminated", id).ok();
                console::println(&s);
                console::set_color(0xFF, 0xFF, 0xFF);
            } else {
                let mut s = String::new();
                console::set_color(0xFF, 0x55, 0x55);
                write!(s, "Task {} not found", id).ok();
                console::println(&s);
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }
        Err(_) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Invalid task ID (must be a number)");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_yield() {
    crate::core_kernel::scheduler::yield_now();
    console::println("Yielded time slice");
}

// ============================================================================
// Demo background tasks
// ============================================================================

/// Counter task — increments a global counter visible via `tasks` output.
/// Periodically writes to the serial log to prove it's running.
fn demo_task_counter() -> ! {
    let mut count: u64 = 0;
    loop {
        count = count.wrapping_add(1);
        if count % 1_000_000 == 0 {
            log::info!("[counter] count = {}", count);
        }
        // Busy-wait a bit to not dominate everything
        for _ in 0..10_000 {
            core::hint::spin_loop();
        }
    }
}

/// Spinner task — cycles through animation frames (purely CPU-bound demo).
fn demo_task_spinner() -> ! {
    let mut i: u64 = 0;
    loop {
        i = i.wrapping_add(1);
        if i % 5_000_000 == 0 {
            let phase = (i / 5_000_000) % 4;
            let ch = match phase {
                0 => '|',
                1 => '/',
                2 => '-',
                _ => '\\',
            };
            log::info!("[spinner] {}", ch);
        }
        core::hint::spin_loop();
    }
}

/// Stress task — exercises the heap allocator from a background task.
fn demo_task_stress() -> ! {
    let mut iteration: u64 = 0;
    loop {
        iteration = iteration.wrapping_add(1);

        // Allocate and deallocate some heap memory
        {
            let mut v = alloc::vec::Vec::new();
            for j in 0..64u64 {
                v.push(iteration.wrapping_mul(j));
            }
            // Vec is dropped here, freeing memory
        }

        if iteration % 100_000 == 0 {
            log::info!("[stress] iteration {}", iteration);
        }

        for _ in 0..5_000 {
            core::hint::spin_loop();
        }
    }
}

fn cmd_interrupts() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("IRQ Statistics:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let timer = IRQ_TIMER_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    let kbd = IRQ_KEYBOARD_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    let rate = crate::hal::pit::tick_rate_hz();

    let mut s = String::new();
    write!(s, "  IRQ 0  (PIT Timer)   : {} ({} Hz)", timer, rate).ok();
    console::println(&s);

    s.clear();
    write!(s, "  IRQ 1  (Keyboard)    : {}", kbd).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Total                : {}", timer + kbd).ok();
    console::println(&s);
}

fn cmd_hexdump(args: &str) {
    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        console::println("Usage: hexdump <address> [count]");
        console::println("  address: hex (e.g., 0x1000 or 1000)");
        console::println("  count:   bytes to dump (default 128, max 512)");
        return;
    }

    let addr_str = parts[0].trim_start_matches("0x").trim_start_matches("0X");
    let addr = match u64::from_str_radix(addr_str, 16) {
        Ok(a) => a,
        Err(_) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Invalid hex address");
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    let count = if parts.len() > 1 {
        parts[1].parse::<usize>().unwrap_or(128).min(512)
    } else {
        128
    };

    console::set_color(0xFF, 0xFF, 0x55);
    let mut s = String::new();
    write!(s, "Memory at {:#010X}, {} bytes:", addr, count).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);

    for row in (0..count).step_by(16) {
        s.clear();
        write!(s, "  {:08X}: ", addr as usize + row).ok();

        for col in 0..16 {
            if row + col < count {
                let byte = unsafe { *((addr as usize + row + col) as *const u8) };
                write!(s, "{:02X} ", byte).ok();
            } else {
                write!(s, "   ").ok();
            }
            if col == 7 { write!(s, " ").ok(); }
        }

        write!(s, " |").ok();
        for col in 0..16 {
            if row + col < count {
                let byte = unsafe { *((addr as usize + row + col) as *const u8) };
                let ch = if (0x20..=0x7E).contains(&byte) { byte as char } else { '.' };
                write!(s, "{}", ch).ok();
            }
        }
        write!(s, "|").ok();
        console::println(&s);
    }
}

/// Enumerate PCI devices using the HAL PCI subsystem
fn cmd_pci() {
    use crate::hal::pci;

    // Ensure PCI is enumerated
    if !pci::is_enumerated() {
        pci::enumerate();
    }

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("PCI Device Enumeration:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  Bus  Dev  Fn  Vendor  Device  Class");
    console::println("  ---  ---  --  ------  ------  -----");

    let devices = pci::device_list();
    let mut s = String::new();
    for dev in &devices {
        s.clear();
        write!(s, "  {:3}  {:3}  {:2}  {:04X}    {:04X}    {}",
            dev.bus, dev.device, dev.function,
            dev.vendor_id, dev.device_id,
            dev.class_name()).ok();
        console::println(&s);
    }

    s.clear();
    write!(s, "\n  {} device(s) found", devices.len()).ok();
    console::println(&s);
}

fn cmd_color(args: &str) {
    let scheme = args.trim();
    if apply_color_scheme(scheme) {
        save_color_scheme(scheme);
    }
}

/// Apply a color scheme by name. Returns true if recognized.
fn apply_color_scheme(name: &str) -> bool {
    match name {
        "green" | "matrix" => {
            console::set_color(0x00, 0xFF, 0x00);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: Matrix green");
            true
        }
        "amber" => {
            console::set_color(0xFF, 0xB0, 0x00);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: Amber terminal");
            true
        }
        "white" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: White on black");
            true
        }
        "blue" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x00, 0xAA);
            console::clear();
            console::println("Color scheme: BSOD classic");
            true
        }
        "default" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x80, 0x80);
            console::clear();
            console::println("Color scheme: Default (teal)");
            true
        }
        _ => {
            console::println("Usage: color <scheme>");
            console::println("  Schemes: green, amber, white, blue, default");
            false
        }
    }
}

/// Save the current color scheme to /system/theme.cfg on the FAT32 disk.
fn save_color_scheme(name: &str) {
    use crate::storage::vfs;
    use crate::hal::virtio_blk;

    if !virtio_blk::is_available() {
        return;
    }

    // Ensure /system directory exists
    if !vfs::exists("/system") {
        vfs::mkdir("/system");
    }

    vfs::write_file("/system/theme.cfg", name.as_bytes());
}

/// Load and apply the saved color scheme from /system/theme.cfg.
/// Called at shell startup. Applies silently (no "Color scheme:" message).
fn load_saved_theme() {
    use crate::storage::vfs;
    use crate::hal::virtio_blk;

    if !virtio_blk::is_available() {
        return;
    }

    if let Some(data) = vfs::read_file("/system/theme.cfg") {
        if let Ok(name) = core::str::from_utf8(&data) {
            let name = name.trim();
            // Apply silently — just set colors, no clear/print
            match name {
                "green" | "matrix" => {
                    console::set_color(0x00, 0xFF, 0x00);
                    console::set_bg_color(0x00, 0x00, 0x00);
                }
                "amber" => {
                    console::set_color(0xFF, 0xB0, 0x00);
                    console::set_bg_color(0x00, 0x00, 0x00);
                }
                "white" => {
                    console::set_color(0xFF, 0xFF, 0xFF);
                    console::set_bg_color(0x00, 0x00, 0x00);
                }
                "blue" => {
                    console::set_color(0xFF, 0xFF, 0xFF);
                    console::set_bg_color(0x00, 0x00, 0xAA);
                }
                "default" => {
                    console::set_color(0xFF, 0xFF, 0xFF);
                    console::set_bg_color(0x00, 0x80, 0x80);
                }
                _ => {}
            }
        }
    }
}

fn cmd_sysinfo() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("System Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();
    write!(s, "  OS:          CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("  Arch:        x86_64 (AMD64)");
    console::println("  Kernel:      Hybrid (NT-inspired)");

    let free = crate::memory::frame_allocator::free_frame_count();
    let total = crate::memory::frame_allocator::total_frame_count();
    s.clear();
    write!(s, "  RAM:         {} MiB total, {} MiB free", total * 4 / 1024, free * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Heap:        {} KiB", crate::memory::heap::HEAP_SIZE / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Timer:       PIT at {} Hz", crate::hal::pit::tick_rate_hz()).ok();
    console::println(&s);

    let ms = crate::hal::pit::ticks_to_ms(ticks());
    s.clear();
    write!(s, "  Uptime:      {}.{}s", ms / 1000, (ms % 1000) / 100).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Tasks:       {} active, {} ctx switches",
        crate::core_kernel::scheduler::active_task_count(),
        crate::core_kernel::scheduler::context_switch_count()).ok();
    console::println(&s);

    let (cols, rows) = console::dimensions();
    s.clear();
    write!(s, "  Console:     {}x{} characters", cols, rows).ok();
    console::println(&s);

    console::println("  Video:       1920x1080 BGR framebuffer (double-buffered)");
    console::println("  Interrupts:  8259 PIC (IRQ0 timer, IRQ1 keyboard)");
    console::println("  Scheduler:   Priority-based preemptive (5 priority levels)");
    console::println("  FPU/SSE:     Context saved per-task (FXSAVE/FXRSTOR)");
}

fn cmd_halt() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Shutting down CantayaOS...");
    console::set_color(0xFF, 0xFF, 0xFF);
    log::info!("System halt requested by user");
    crate::hal::speaker::beep(440, 100);
    crate::hal::acpi::acpi_shutdown();
}

fn cmd_reboot() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Rebooting...");
    console::set_color(0xFF, 0xFF, 0xFF);
    log::info!("System reboot requested by user");

    unsafe {
        let mut status = crate::hal::port::inb(0x64);
        while status & 0x02 != 0 {
            status = crate::hal::port::inb(0x64);
        }
        crate::hal::port::outb(0x64, 0xFE);
        loop { core::arch::asm!("cli; hlt"); }
    }
}

// ============================================================================
// Desktop Command
// ============================================================================

fn cmd_desktop() {
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("Launching CantayaOS Desktop...");
    console::set_color(0xFF, 0xFF, 0xFF);

    // Enter the desktop environment (blocks until user exits)
    crate::desktop::run();

    // Returned from desktop — restore console
    console::clear();
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("Returned to CantayaOS Shell.\n");
}

// ============================================================================
// New Commands (Phase 14)
// ============================================================================

fn cmd_date() {
    use crate::hal::rtc;

    let dt = rtc::read_datetime();
    let mut s = String::new();
    let mut buf = [0u8; 20];
    let formatted = dt.format(&mut buf);
    write!(s, "Date/Time: {}", formatted).ok();
    console::println(&s);
}

fn cmd_acpi() {
    use crate::hal::acpi;

    let info = acpi::ACPI_INFO.lock();
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("ACPI Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    if !info.valid {
        console::println("  ACPI not initialized or RSDP not found.");
        return;
    }

    let mut s = String::new();
    write!(s, "  Revision:      {}", info.revision).ok();
    console::println(&s);

    let oem = core::str::from_utf8(&info.oem_id).unwrap_or("?").trim_end_matches('\0');
    s.clear();
    write!(s, "  OEM ID:        {}", oem).ok();
    console::println(&s);

    s.clear();
    write!(s, "  CPU count:     {} (via MADT Local APIC entries)", info.cpu_count).ok();
    console::println(&s);

    if info.lapic_address != 0 {
        s.clear();
        write!(s, "  LAPIC addr:    {:#X}", info.lapic_address).ok();
        console::println(&s);
    }

    if info.ioapic_address != 0 {
        s.clear();
        write!(s, "  I/O APIC addr: {:#X}", info.ioapic_address).ok();
        console::println(&s);
    }

    if info.hpet_address != 0 {
        s.clear();
        write!(s, "  HPET addr:     {:#X}", info.hpet_address).ok();
        console::println(&s);
    }

    if info.mcfg_address != 0 {
        s.clear();
        write!(s, "  MCFG addr:     {:#X} (PCIe ECAM)", info.mcfg_address).ok();
        console::println(&s);
    }

    // Table signatures
    s.clear();
    write!(s, "  Tables found:  {}", info.table_count).ok();
    console::println(&s);

    if info.table_count > 0 {
        s.clear();
        write!(s, "  Signatures:   ").ok();
        for i in 0..info.table_count.min(32) {
            let sig = &info.table_signatures[i];
            if let Ok(sig_str) = core::str::from_utf8(sig) {
                write!(s, " {}", sig_str.trim_end_matches('\0')).ok();
            }
        }
        console::println(&s);
    }
}

fn cmd_sleep(args: &str) {
    let ms_str = args.trim();
    if ms_str.is_empty() {
        console::println("Usage: sleep <milliseconds>");
        return;
    }

    match ms_str.parse::<u64>() {
        Ok(ms) if ms > 0 && ms <= 60000 => {
            let mut s = String::new();
            write!(s, "Sleeping for {} ms...", ms).ok();
            console::println(&s);
            crate::core_kernel::scheduler::sleep_ms(ms);
            console::println("Awake!");
        }
        Ok(_) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Sleep duration must be 1-60000 ms");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
        Err(_) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Invalid number");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_priority(args: &str) {
    use crate::core_kernel::scheduler::{self, TaskPriority};

    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.len() != 2 {
        console::println("Usage: priority <task_id> <level>");
        console::println("  Levels: idle, low, normal, high, realtime (or rt)");
        return;
    }

    let task_id = match parts[0].parse::<u32>() {
        Ok(id) => id,
        Err(_) => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Invalid task ID");
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    let priority = match TaskPriority::from_name(parts[1]) {
        Some(p) => p,
        None => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Unknown priority. Use: idle, low, normal, high, realtime");
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    if scheduler::set_priority(task_id, priority) {
        let mut s = String::new();
        console::set_color(0x55, 0xFF, 0x55);
        write!(s, "Task {} priority set to {}", task_id, priority.name()).ok();
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        console::set_color(0xFF, 0x55, 0x55);
        write!(s, "Task {} not found", task_id).ok();
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_bootinfo() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Boot Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();
    write!(s, "  Kernel:     CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("  Boot mode:  UEFI x86_64");
    console::println("  Kernel at:  0xFFFFFFFF80000000 (higher half)");

    let cr3 = crate::hal::cpu::read_cr3();
    s.clear();
    write!(s, "  PML4:       {:#X}", cr3).ok();
    console::println(&s);

    let free = crate::memory::frame_allocator::free_frame_count();
    let total = crate::memory::frame_allocator::total_frame_count();
    s.clear();
    write!(s, "  RAM:        {} MiB total, {} MiB free", total * 4 / 1024, free * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Stack:      64 KiB bootstrap (in .bss)").ok();
    console::println(&s);

    s.clear();
    write!(s, "  Heap:       {} KiB (growable)", crate::memory::heap::HEAP_SIZE / 1024).ok();
    console::println(&s);

    let tasks = crate::core_kernel::scheduler::active_task_count();
    let ctx = crate::core_kernel::scheduler::context_switch_count();
    s.clear();
    write!(s, "  Scheduler:  {} tasks, {} ctx switches, priority-based preemptive", tasks, ctx).ok();
    console::println(&s);

    console::println("  Timer:      PIT 1000 Hz (1ms resolution)");
    console::println("  Keyboard:   PS/2 scan code set 1");
    console::println("  Display:    UEFI GOP framebuffer, double-buffered");
}

fn cmd_memmap() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Memory Map (frame allocator regions):");
    console::set_color(0xFF, 0xFF, 0xFF);

    let free = crate::memory::frame_allocator::free_frame_count();
    let total = crate::memory::frame_allocator::total_frame_count();
    let used = total - free;

    let mut s = String::new();
    write!(s, "  Total frames:  {} ({} MiB)", total, total * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Used frames:   {} ({} MiB)", used, used * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Free frames:   {} ({} MiB)", free, free * 4 / 1024).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Page size:     4 KiB").ok();
    console::println(&s);

    s.clear();
    write!(s, "  Kernel heap:   {} KiB (growable)", crate::memory::heap::HEAP_SIZE / 1024).ok();
    console::println(&s);

    // Memory layout overview
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nKernel Virtual Memory Layout:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  0x0000000000000000  User space (future)");
    console::println("  0xFFFF800000000000  Direct physical mapping");
    console::println("  0xFFFFFFFF80000000  Kernel code + data (.text/.rodata/.bss)");
    console::println("  0xFFFFFFFFC0000000  Kernel heap (virtual, identity-mapped)");
    console::println("  Stack pages:        Allocated per-task from frame allocator");
}

fn cmd_panic() {
    console::set_color(0xFF, 0xFF, 0x00);
    console::println("Triggering kernel panic for BSOD test...");
    console::set_color(0xFF, 0xFF, 0xFF);
    panic!("User-triggered panic via 'panic' command");
}

// ============================================================================
// Filesystem Commands
// ============================================================================

fn cmd_ls(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let path = if args.is_empty() { "." } else { args };
    // Convert backslashes if user typed Windows-style path
    let path_unix = win_to_unix_path(path);
    let entries = match vfs::list_dir(&path_unix) {
        Some(e) => e,
        None => {
            let mut s = String::new();
            write!(s, "File Not Found: {}", path).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    // Windows dir header: show the volume and directory path
    let display_path = if path_unix == "." {
        unix_to_win_path(&vfs::cwd())
    } else if path_unix.starts_with('/') {
        unix_to_win_path(&path_unix)
    } else {
        // Relative path — combine with CWD for display
        let cwd = vfs::cwd();
        let full = if cwd == "/" {
            let mut p = String::from("/");
            p.push_str(&path_unix);
            p
        } else {
            let mut p = cwd;
            p.push('/');
            p.push_str(&path_unix);
            p
        };
        unix_to_win_path(&full)
    };
    console::println("");
    console::println(" Volume in drive C is CANTAYAOS");
    console::println("");
    let mut s = String::new();
    write!(s, " Directory of {}", display_path).ok();
    console::println(&s);
    console::println("");

    if entries.is_empty() {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("File Not Found");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let mut file_count = 0u32;
    let mut dir_count = 0u32;
    let mut total_size: u64 = 0;

    for entry in &entries {
        let mut s = String::new();
        if entry.is_dir {
            dir_count += 1;
            console::set_color(0x55, 0xBB, 0xFF);
            write!(s, "    <DIR>          {}", entry.name).ok();
        } else {
            file_count += 1;
            total_size += entry.size as u64;
            console::set_color(0xFF, 0xFF, 0xFF);
            write!(s, "    {:>14}  {}", format_size_commas(entry.size), entry.name).ok();
        }
        console::println(&s);
    }
    console::set_color(0xFF, 0xFF, 0xFF);

    // Windows dir footer
    s.clear();
    write!(s, "    {:>8} File(s)  {:>14} bytes", file_count, format_size_commas(total_size as u32)).ok();
    console::println(&s);
    s.clear();
    write!(s, "    {:>8} Dir(s)", dir_count).ok();
    console::println(&s);
}

/// Format a file size with comma separators (Windows dir style).
fn format_size_commas(size: u32) -> String {
    let num = alloc::format!("{}", size);
    let bytes = num.as_bytes();
    let len = bytes.len();
    if len <= 3 {
        return num;
    }
    let mut result = String::with_capacity(len + len / 3);
    let first_group = len % 3;
    if first_group > 0 {
        for &b in &bytes[..first_group] {
            result.push(b as char);
        }
        if first_group < len {
            result.push(',');
        }
    }
    let remaining = &bytes[first_group..];
    for (i, &b) in remaining.iter().enumerate() {
        result.push(b as char);
        if (i + 1) % 3 == 0 && i + 1 < remaining.len() {
            result.push(',');
        }
    }
    result
}

fn cmd_cat(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: cat <filename>");
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            if data.is_empty() {
                console::set_color(0xAA, 0xAA, 0xAA);
                console::println("(empty file)");
                console::set_color(0xFF, 0xFF, 0xFF);
                return;
            }

            // Display as text (with fallback for non-UTF8)
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    console::println(text);
                }
                Err(_) => {
                    // Show as hex dump for binary files
                    console::set_color(0xAA, 0xAA, 0xAA);
                    console::println("(binary file, showing hex dump)");
                    console::set_color(0xFF, 0xFF, 0xFF);
                    let limit = data.len().min(256);
                    for (i, chunk) in data[..limit].chunks(16).enumerate() {
                        let mut s = String::new();
                        write!(s, "  {:04X}: ", i * 16).ok();
                        for b in chunk {
                            write!(s, "{:02X} ", b).ok();
                        }
                        console::println(&s);
                    }
                    if data.len() > 256 {
                        let mut s = String::new();
                        write!(s, "  ... ({} more bytes)", data.len() - 256).ok();
                        console::println(&s);
                    }
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "cat: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_write(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    // Parse: write <filename> <content>
    let (filename, content) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: write <filename> <text>");
            return;
        }
    };

    if filename.is_empty() || content.is_empty() {
        console::println("Usage: write <filename> <text>");
        return;
    }

    if vfs::write_file(filename, content.as_bytes()) {
        let mut s = String::new();
        write!(s, "Wrote {} bytes to '{}'", content.len(), filename).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "write: failed to write to '{}'", filename).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_mkdir(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: mkdir <dirname>");
        return;
    }

    if vfs::mkdir(args) {
        let mut s = String::new();
        write!(s, "Created directory '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "mkdir: failed to create '{}'", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_rm(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        console::println("Usage: rm <filename|dirname>");
        return;
    }

    if vfs::delete(args) {
        let mut s = String::new();
        write!(s, "Deleted '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "rm: failed to delete '{}' (does it exist? is the directory empty?)", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_cp(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (src, dst) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: cp <source> <destination>");
            return;
        }
    };

    if src.is_empty() || dst.is_empty() {
        console::println("Usage: cp <source> <destination>");
        return;
    }

    if vfs::copy_file(src, dst) {
        let mut s = String::new();
        write!(s, "Copied '{}' -> '{}'", src, dst).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        let mut s = String::new();
        write!(s, "cp: failed to copy '{}' to '{}'", src, dst).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_cd(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("The system cannot find the drive specified.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        // Like Windows cmd: cd with no args prints current directory
        cmd_pwd();
        return;
    }

    // Convert Windows-style backslashes to forward slashes for VFS
    let path = win_to_unix_path(args);

    if !vfs::cd(&path) {
        let mut s = String::new();
        write!(s, "The system cannot find the path specified: {}", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

/// Convert a Windows-style path input to Unix-style for VFS.
///
/// Handles:
///   - Backslash to forward slash: `Users\Root` → `Users/Root`
///   - Strip drive letter: `C:\Users\Root` → `/Users/Root`
///   - Bare `C:` or `C:\` → `/`
fn win_to_unix_path(path: &str) -> String {
    let mut p = path;
    // Strip optional drive letter prefix (C: or C:\)
    if p.len() >= 2 && p.as_bytes()[1] == b':' {
        p = &p[2..];
        if p.starts_with('\\') || p.starts_with('/') {
            p = &p[1..];
        }
        if p.is_empty() {
            return String::from("/");
        }
    }
    // Convert backslashes to forward slashes
    let mut result = String::with_capacity(p.len() + 1);
    // If the remaining path doesn't start with / or \, keep as relative
    for ch in p.chars() {
        if ch == '\\' {
            result.push('/');
        } else {
            result.push(ch);
        }
    }
    result
}

fn cmd_pwd() {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    // Show Windows-style path
    let path = unix_to_win_path(&vfs::cwd());
    console::println(&path);
}

fn cmd_vol() {
    console::println("");
    console::println(" Volume in drive C is CANTAYAOS");
    console::println(" Volume Serial Number is CA17-AY05");
    console::println("");
}

fn cmd_disk() {
    use crate::storage::{vfs, fat32};
    use crate::hal::virtio_blk;

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Disk Information:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();

    if !virtio_blk::is_available() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("  No block device available.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let sectors = virtio_blk::capacity_sectors();
    write!(s, "  Block device:   virtio-blk").ok();
    console::println(&s);

    s.clear();
    write!(s, "  Capacity:       {} sectors ({} MiB)", sectors, sectors * 512 / (1024 * 1024)).ok();
    console::println(&s);

    s.clear();
    write!(s, "  Sector size:    512 bytes").ok();
    console::println(&s);

    if fat32::is_mounted() {
        let label = fat32::volume_label();
        s.clear();
        write!(s, "  Filesystem:     FAT32").ok();
        console::println(&s);

        s.clear();
        write!(s, "  Volume label:   {}", label).ok();
        console::println(&s);

        let (total, free, cluster_size) = fat32::stats();
        s.clear();
        write!(s, "  Cluster size:   {} bytes", cluster_size).ok();
        console::println(&s);

        let used = total - free;
        s.clear();
        write!(s, "  Clusters:       {} total, {} used, {} free", total, used, free).ok();
        console::println(&s);

        let free_bytes = free as u64 * cluster_size as u64;
        let used_bytes = used as u64 * cluster_size as u64;
        s.clear();
        write!(s, "  Space:          {} KiB used, {} KiB free",
            used_bytes / 1024, free_bytes / 1024).ok();
        console::println(&s);

        if vfs::is_ready() {
            s.clear();
            write!(s, "  Mount point:    /").ok();
            console::println(&s);
            s.clear();
            write!(s, "  Current dir:    {}", vfs::cwd()).ok();
            console::println(&s);
        }
    } else {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("  Filesystem:     (not mounted)");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

// ============================================================================
// New Commands — Enhanced Shell v2.0
// ============================================================================

fn cmd_beep(args: &str) {
    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    let freq = parts.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(440);
    let dur = parts.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(200);
    let dur = dur.min(5000);
    let mut s = String::new();
    write!(s, "Beep: {} Hz for {} ms", freq, dur).ok();
    console::println(&s);
    crate::hal::speaker::beep(freq, dur);
}

fn cmd_hostname(args: &str) {
    let name = args.trim();
    if name.is_empty() {
        console::println(&get_hostname());
    } else {
        set_hostname_value(name);
        // Persist hostname
        use crate::storage::vfs;
        if crate::hal::virtio_blk::is_available() {
            if !vfs::exists("/system") { vfs::mkdir("/system"); }
            vfs::write_file("/system/hostname.cfg", name.as_bytes());
        }
        let mut s = String::new();
        write!(s, "Hostname set to '{}'", name).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_whoami() {
    let user = env_get("USER").unwrap_or_else(|| "root".into());
    console::println(&user);
}

fn cmd_history() {
    let hist = CMD_HISTORY.lock();
    if hist.count == 0 {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("(no command history)");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Command History:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let start = if hist.count < HISTORY_SIZE {
        0
    } else {
        hist.write_idx
    };

    for i in 0..hist.count {
        let idx = (start + i) % HISTORY_SIZE;
        let len = hist.lengths[idx];
        if len > 0 {
            if let Ok(cmd) = core::str::from_utf8(&hist.entries[idx][..len]) {
                let mut s = String::new();
                write!(s, "  {:>3}  {}", i + 1, cmd).ok();
                console::println(&s);
            }
        }
    }
}

fn cmd_grep(args: &str) {
    use crate::storage::vfs;

    let (pattern, filename) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: grep <pattern> <file>");
            return;
        }
    };

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(filename) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let pattern_lower = pattern.to_lowercase();
                    let mut count = 0;
                    for (i, line) in text.lines().enumerate() {
                        if line.to_lowercase().contains(&pattern_lower) {
                            let mut s = String::new();
                            console::set_color(0x55, 0xFF, 0x55);
                            write!(s, "{:>4}: ", i + 1).ok();
                            console::print(&s);
                            console::set_color(0xFF, 0xFF, 0xFF);
                            console::println(line);
                            count += 1;
                        }
                    }
                    if count == 0 {
                        console::set_color(0xAA, 0xAA, 0xAA);
                        console::println("(no matches)");
                        console::set_color(0xFF, 0xFF, 0xFF);
                    } else {
                        let mut s = String::new();
                        console::set_color(0xAA, 0xAA, 0xAA);
                        write!(s, "\n{} matching line(s)", count).ok();
                        console::println(&s);
                        console::set_color(0xFF, 0xFF, 0xFF);
                    }
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("grep: binary file, cannot search");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "grep: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_find(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let pattern = args.trim();
    if pattern.is_empty() {
        console::println("Usage: find <name-pattern>");
        return;
    }

    let pattern_lower = pattern.to_lowercase();
    let mut count = 0;

    fn search_recursive(path: &str, pattern: &str, count: &mut usize) {
        use crate::storage::vfs;
        if let Some(entries) = vfs::list_dir(path) {
            for entry in &entries {
                let full_path = if path == "/" {
                    alloc::format!("/{}", entry.name)
                } else {
                    alloc::format!("{}/{}", path, entry.name)
                };
                if entry.name.to_lowercase().contains(pattern) {
                    if entry.is_dir {
                        console::set_color(0x55, 0xBB, 0xFF);
                    } else {
                        console::set_color(0xFF, 0xFF, 0xFF);
                    }
                    console::println(&full_path);
                    *count += 1;
                }
                if entry.is_dir {
                    search_recursive(&full_path, pattern, count);
                }
            }
        }
    }

    search_recursive("/", &pattern_lower, &mut count);

    if count == 0 {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("(no files found)");
    } else {
        let mut s = String::new();
        console::set_color(0xAA, 0xAA, 0xAA);
        write!(s, "\n{} file(s) found", count).ok();
        console::println(&s);
    }
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_wc(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: wc <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("");
            let lines = text.lines().count();
            let words = text.split_whitespace().count();
            let chars = text.len();
            let mut s = String::new();
            write!(s, "  {:>6} lines  {:>6} words  {:>6} bytes  {}", lines, words, chars, args).ok();
            console::println(&s);
        }
        None => {
            let mut s = String::new();
            write!(s, "wc: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_head(args: &str) {
    use crate::storage::vfs;

    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        console::println("Usage: head <file> [n]");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = parts[0];
    let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);

    match vfs::read_file(filename) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("(binary)");
            for (i, line) in text.lines().enumerate() {
                if i >= count { break; }
                console::println(line);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "head: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_tail(args: &str) {
    use crate::storage::vfs;

    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        console::println("Usage: tail <file> [n]");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = parts[0];
    let count = parts.get(1).and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);

    match vfs::read_file(filename) {
        Some(data) => {
            let text = core::str::from_utf8(&data).unwrap_or("(binary)");
            let lines: alloc::vec::Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(count);
            for line in &lines[start..] {
                console::println(line);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "tail: '{}': No such file", filename).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_touch(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: touch <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if vfs::exists(args) {
        console::println("File already exists.");
        return;
    }

    if vfs::write_file(args, &[]) {
        let mut s = String::new();
        write!(s, "Created '{}'", args).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("touch: failed to create file");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_stat(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: stat <file|dir>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if !vfs::exists(args) {
        let mut s = String::new();
        write!(s, "stat: '{}': No such file or directory", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let mut s = String::new();
    console::set_color(0xFF, 0xFF, 0x55);
    write!(s, "  File: {}", args).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);

    if vfs::is_dir(args) {
        console::println("  Type: directory");
        if let Some(entries) = vfs::list_dir(args) {
            s.clear();
            write!(s, "  Contents: {} entries", entries.len()).ok();
            console::println(&s);
        }
    } else {
        console::println("  Type: regular file");
        if let Some(data) = vfs::read_file(args) {
            s.clear();
            write!(s, "  Size: {} bytes", data.len()).ok();
            console::println(&s);
            // Check if text or binary
            let is_text = data.iter().all(|&b| b == b'\n' || b == b'\r' || b == b'\t' || (b >= 0x20 && b < 0x7F));
            console::println(if is_text { "  Kind: text" } else { "  Kind: binary" });
        }
    }
}

fn cmd_tree(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let path = if args.is_empty() { "." } else { args };
    console::set_color(0x55, 0xBB, 0xFF);
    console::println(path);
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut file_count = 0usize;
    let mut dir_count = 0usize;

    fn print_tree(path: &str, prefix: &str, fc: &mut usize, dc: &mut usize) {
        use crate::storage::vfs;
        if let Some(entries) = vfs::list_dir(path) {
            let len = entries.len();
            for (i, entry) in entries.iter().enumerate() {
                let is_last = i == len - 1;
                let connector = if is_last { "└── " } else { "├── " };
                let mut line = String::new();
                write!(line, "{}{}", prefix, connector).ok();

                if entry.is_dir {
                    *dc += 1;
                    console::set_color(0x55, 0xBB, 0xFF);
                    write!(line, "{}/", entry.name).ok();
                    console::println(&line);
                    console::set_color(0xFF, 0xFF, 0xFF);

                    let new_prefix = alloc::format!("{}{}", prefix, if is_last { "    " } else { "│   " });
                    let child_path = if path == "/" || path == "." {
                        alloc::format!("/{}", entry.name)
                    } else {
                        alloc::format!("{}/{}", path, entry.name)
                    };
                    print_tree(&child_path, &new_prefix, fc, dc);
                } else {
                    *fc += 1;
                    write!(line, "{}", entry.name).ok();
                    console::println(&line);
                }
            }
        }
    }

    print_tree(path, "", &mut file_count, &mut dir_count);

    let mut s = String::new();
    console::set_color(0xAA, 0xAA, 0xAA);
    write!(s, "\n{} directories, {} files", dir_count, file_count).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_cal() {
    use crate::hal::rtc;

    let dt = rtc::read_datetime();
    let year = dt.year as i32;
    let month = dt.month as u32;
    let day = dt.day as u32;

    // Month names
    let month_names = [
        "", "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ];

    let month_name = if (month as usize) < month_names.len() {
        month_names[month as usize]
    } else {
        "?"
    };

    let mut header = String::new();
    write!(header, "    {} {}", month_name, year).ok();
    console::set_color(0xFF, 0xFF, 0x55);
    console::println(&header);
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println(" Su Mo Tu We Th Fr Sa");

    // Days in month
    let days_in = match month {
        1 => 31, 2 => if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 29 } else { 28 },
        3 => 31, 4 => 30, 5 => 31, 6 => 30,
        7 => 31, 8 => 31, 9 => 30, 10 => 31, 11 => 30, 12 => 31,
        _ => 30,
    };

    // Zeller's congruence for first day of month
    let (y, m) = if month <= 2 { (year - 1, month + 12) } else { (year, month) };
    let q = 1;
    let k = y % 100;
    let j = y / 100;
    let h = (q + (13 * (m as i32 + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    let first_dow = ((h + 6) % 7) as u32; // 0=Sunday

    let mut line = String::new();
    // Pad first week
    for _ in 0..first_dow {
        write!(line, "   ").ok();
    }

    let mut dow = first_dow;
    for d in 1..=days_in {
        if d == day {
            console::print(&line);
            line.clear();
            console::set_color(0x00, 0xFF, 0x00);
            write!(line, "{:>3}", d).ok();
            console::print(&line);
            console::set_color(0xFF, 0xFF, 0xFF);
            line.clear();
        } else {
            write!(line, "{:>3}", d).ok();
        }
        dow += 1;
        if dow >= 7 {
            console::println(&line);
            line.clear();
            dow = 0;
        }
    }
    if !line.is_empty() {
        console::println(&line);
    }
}

fn cmd_fortune() {
    static FORTUNES: &[&str] = &[
        "The best OS is the one you write yourself.",
        "In kernel space, no one can hear you segfault.",
        "Keep calm and write more drivers.",
        "A wise kernel never panics... unless asked politely.",
        "Interrupts are just the universe's way of saying 'hello'.",
        "Every lock has its deadlock, and every deadlock its lesson.",
        "To understand recursion, first understand recursion.",
        "The stack grows down, but ambition grows up.",
        "There are only two hard things: cache invalidation and naming things.",
        "Memory is the mind of the machine; don't waste it.",
        "An OS without a shell is like a ship without a wheel.",
        "The PIT ticks 1000 times per second. What do you do with yours?",
        "If it compiles, ship it. (But maybe test first.)",
        "Kernel development: where 'hello world' takes 10,000 lines.",
        "A page fault a day keeps the users away.",
        "Perfection is achieved not when there is nothing more to add, but when there is nothing left to panic about.",
    ];

    let tick = ticks() as usize;
    let idx = tick % FORTUNES.len();
    console::set_color(0xFF, 0xFF, 0x55);
    console::print("  ");
    console::println(FORTUNES[idx]);
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_banner(args: &str) {
    if args.is_empty() {
        console::println("Usage: banner <text>");
        return;
    }

    // Simple block-letter ASCII art for uppercase letters
    let text = args.to_uppercase();
    // Each character is 5 lines tall, 5 chars wide
    let patterns: &[(char, [&str; 5])] = &[
        ('A', [" ### ", "#   #", "#####", "#   #", "#   #"]),
        ('B', ["#### ", "#   #", "#### ", "#   #", "#### "]),
        ('C', [" ### ", "#    ", "#    ", "#    ", " ### "]),
        ('D', ["#### ", "#   #", "#   #", "#   #", "#### "]),
        ('E', ["#####", "#    ", "###  ", "#    ", "#####"]),
        ('F', ["#####", "#    ", "###  ", "#    ", "#    "]),
        ('G', [" ### ", "#    ", "# ## ", "#   #", " ### "]),
        ('H', ["#   #", "#   #", "#####", "#   #", "#   #"]),
        ('I', ["#####", "  #  ", "  #  ", "  #  ", "#####"]),
        ('J', ["#####", "   # ", "   # ", "#  # ", " ## "]),
        ('K', ["#   #", "#  # ", "###  ", "#  # ", "#   #"]),
        ('L', ["#    ", "#    ", "#    ", "#    ", "#####"]),
        ('M', ["#   #", "## ##", "# # #", "#   #", "#   #"]),
        ('N', ["#   #", "##  #", "# # #", "#  ##", "#   #"]),
        ('O', [" ### ", "#   #", "#   #", "#   #", " ### "]),
        ('P', ["#### ", "#   #", "#### ", "#    ", "#    "]),
        ('Q', [" ### ", "#   #", "# # #", "#  # ", " ## #"]),
        ('R', ["#### ", "#   #", "#### ", "#  # ", "#   #"]),
        ('S', [" ### ", "#    ", " ### ", "    #", " ### "]),
        ('T', ["#####", "  #  ", "  #  ", "  #  ", "  #  "]),
        ('U', ["#   #", "#   #", "#   #", "#   #", " ### "]),
        ('V', ["#   #", "#   #", "#   #", " # # ", "  #  "]),
        ('W', ["#   #", "#   #", "# # #", "## ##", "#   #"]),
        ('X', ["#   #", " # # ", "  #  ", " # # ", "#   #"]),
        ('Y', ["#   #", " # # ", "  #  ", "  #  ", "  #  "]),
        ('Z', ["#####", "   # ", "  #  ", " #   ", "#####"]),
        ('0', [" ### ", "#   #", "#   #", "#   #", " ### "]),
        ('1', ["  #  ", " ##  ", "  #  ", "  #  ", " ### "]),
        ('!', ["  #  ", "  #  ", "  #  ", "     ", "  #  "]),
        (' ', ["     ", "     ", "     ", "     ", "     "]),
    ];

    console::set_color(0x55, 0xFF, 0xFF);
    for row in 0..5 {
        let mut line = String::new();
        for ch in text.chars() {
            let pattern = patterns.iter().find(|(c, _)| *c == ch);
            if let Some((_, rows)) = pattern {
                write!(line, "{}  ", rows[row]).ok();
            } else {
                write!(line, "     ").ok();
            }
        }
        console::println(&line);
    }
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_env(args: &str) {
    env_init();
    let args = args.trim();
    if args.is_empty() {
        // Show all env vars
        let env = ENV_VARS.lock();
        if let Some(ref map) = *env {
            console::set_color(0xFF, 0xFF, 0x55);
            console::println("Environment Variables:");
            console::set_color(0xFF, 0xFF, 0xFF);
            for (k, v) in map.iter() {
                let mut s = String::new();
                write!(s, "  {}={}", k, v).ok();
                console::println(&s);
            }
        }
    } else {
        // set KEY=VALUE
        if let Some(eq_pos) = args.find('=') {
            let key = args[..eq_pos].trim();
            let value = args[eq_pos + 1..].trim();
            env_set(key, value);
            let mut s = String::new();
            write!(s, "{}={}", key, value).ok();
            console::println(&s);
        } else {
            // Show single var
            match env_get(args) {
                Some(val) => {
                    let mut s = String::new();
                    write!(s, "{}={}", args, val).ok();
                    console::println(&s);
                }
                None => {
                    let mut s = String::new();
                    write!(s, "'{}' not set", args).ok();
                    console::println(&s);
                }
            }
        }
    }
}

fn cmd_unset(args: &str) {
    if args.is_empty() {
        console::println("Usage: unset <variable>");
        return;
    }
    env_remove(args.trim());
    console::set_color(0x55, 0xFF, 0x55);
    let mut s = String::new();
    write!(s, "Unset '{}'", args.trim()).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_append(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (filename, content) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: append <filename> <text>");
            return;
        }
    };

    // Read existing content, append, write back
    let mut existing = vfs::read_file(filename).unwrap_or_default();
    existing.extend_from_slice(content.as_bytes());
    existing.push(b'\n');

    if vfs::write_file(filename, &existing) {
        let mut s = String::new();
        write!(s, "Appended {} bytes to '{}'", content.len() + 1, filename).ok();
        console::set_color(0x55, 0xFF, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("append: failed");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_rename(args: &str) {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let (src, dst) = match args.find(' ') {
        Some(pos) => (&args[..pos], args[pos + 1..].trim()),
        None => {
            console::println("Usage: rename <source> <destination>");
            return;
        }
    };

    // Rename = copy + delete
    if vfs::copy_file(src, dst) {
        if vfs::delete(src) {
            let mut s = String::new();
            write!(s, "Renamed '{}' -> '{}'", src, dst).ok();
            console::set_color(0x55, 0xFF, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        } else {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("rename: copied but failed to delete source");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    } else {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("rename: failed to copy");
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_xxd(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: xxd <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    match vfs::read_file(args) {
        Some(data) => {
            let limit = data.len().min(512);
            for (i, chunk) in data[..limit].chunks(16).enumerate() {
                let mut s = String::new();
                console::set_color(0xAA, 0xAA, 0xAA);
                write!(s, "{:08X}: ", i * 16).ok();
                console::print(&s);
                s.clear();

                console::set_color(0xFF, 0xFF, 0xFF);
                for (j, b) in chunk.iter().enumerate() {
                    write!(s, "{:02X}", b).ok();
                    if j % 2 == 1 { write!(s, " ").ok(); }
                }
                // Pad if short
                for _ in chunk.len()..16 {
                    write!(s, "  ").ok();
                }
                write!(s, " ").ok();

                // ASCII representation
                console::set_color(0x55, 0xFF, 0x55);
                for b in chunk {
                    if *b >= 0x20 && *b < 0x7F {
                        write!(s, "{}", *b as char).ok();
                    } else {
                        write!(s, ".").ok();
                    }
                }
                console::println(&s);
            }
            console::set_color(0xFF, 0xFF, 0xFF);
            if data.len() > limit {
                let mut s = String::new();
                write!(s, "  ... ({} more bytes)", data.len() - limit).ok();
                console::set_color(0xAA, 0xAA, 0xAA);
                console::println(&s);
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "xxd: '{}': No such file", args).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

// ============================================================================
// In-Shell Text Editor
// ============================================================================

fn cmd_edit(args: &str) {
    use crate::storage::vfs;
    use alloc::vec::Vec;

    if args.is_empty() {
        console::println("Usage: edit <file>");
        return;
    }

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    let filename = args.trim();

    // Load existing file content or start empty
    let initial = vfs::read_file(filename).unwrap_or_default();
    let text = core::str::from_utf8(&initial).unwrap_or("");
    let mut lines: Vec<String> = if text.is_empty() {
        alloc::vec![String::new()]
    } else {
        text.lines().map(|l| l.into()).collect()
    };
    if lines.is_empty() { lines.push(String::new()); }

    let mut cursor_row: usize = 0;
    let mut cursor_col: usize = 0;
    let mut scroll_offset: usize = 0;
    let mut modified = false;
    let mut running = true;
    let mut message = String::from("Ctrl+S=Save  Ctrl+Q=Quit  Ctrl+G=Goto");

    let (screen_cols, screen_rows) = console::dimensions();
    let edit_rows = screen_rows.saturating_sub(2); // Reserve for header + footer

    while running {
        // Draw editor
        console::clear();

        // Header bar
        console::set_color(0x00, 0x00, 0x00);
        console::set_bg_color(0xAA, 0xAA, 0xAA);
        let mut header = String::new();
        write!(header, " EDIT: {} {} [{}/{}]",
            filename,
            if modified { "(modified)" } else { "" },
            cursor_row + 1,
            lines.len()
        ).ok();
        while header.len() < screen_cols { header.push(' '); }
        console::print(&header);
        console::print("\n");

        // Reset colors for content
        console::set_color(0xFF, 0xFF, 0xFF);
        console::set_bg_color(0x00, 0x00, 0x00);

        // Draw visible lines
        for i in 0..edit_rows {
            let line_idx = scroll_offset + i;
            if line_idx < lines.len() {
                let line = &lines[line_idx];
                let display = if line.len() > screen_cols - 5 {
                    &line[..screen_cols - 5]
                } else {
                    line.as_str()
                };
                // Line number
                let mut num = String::new();
                console::set_color(0x55, 0x55, 0x55);
                write!(num, "{:>3} ", line_idx + 1).ok();
                console::print(&num);
                console::set_color(0xFF, 0xFF, 0xFF);
                console::println(display);
            } else {
                console::set_color(0x55, 0x55, 0x55);
                console::println("  ~ ");
                console::set_color(0xFF, 0xFF, 0xFF);
            }
        }

        // Footer / status bar
        console::set_color(0x00, 0x00, 0x00);
        console::set_bg_color(0xAA, 0xAA, 0xAA);
        let mut footer = String::new();
        write!(footer, " {}", message).ok();
        while footer.len() < screen_cols { footer.push(' '); }
        console::print(&footer);

        // Reset colors
        console::set_color(0xFF, 0xFF, 0xFF);
        console::set_bg_color(0x00, 0x00, 0x00);

        // Wait for input
        let event = loop {
            if let Some(e) = keyboard::try_read_char() {
                break e;
            }
            unsafe { core::arch::asm!("hlt"); }
        };

        message.clear();
        write!(message, "Ctrl+S=Save  Ctrl+Q=Quit  Ctrl+G=Goto").ok();

        match event.ascii {
            // Ctrl+S — save
            0x13 => {
                let mut content = String::new();
                for (i, line) in lines.iter().enumerate() {
                    content.push_str(line);
                    if i < lines.len() - 1 {
                        content.push('\n');
                    }
                }
                if vfs::write_file(filename, content.as_bytes()) {
                    modified = false;
                    message.clear();
                    write!(message, "Saved {} bytes to '{}'", content.len(), filename).ok();
                    crate::hal::speaker::beep(880, 50);
                } else {
                    message.clear();
                    write!(message, "ERROR: Failed to save!").ok();
                    crate::hal::speaker::error_beep();
                }
            }

            // Ctrl+Q — quit
            0x11 => {
                if modified {
                    message.clear();
                    write!(message, "Unsaved changes! Press Ctrl+Q again to confirm quit.").ok();
                    // Redraw with message, wait for next key
                    console::clear();
                    console::set_color(0xFF, 0xFF, 0x55);
                    console::println(&message);
                    console::set_color(0xFF, 0xFF, 0xFF);

                    let confirm = loop {
                        if let Some(e) = keyboard::try_read_char() { break e; }
                        unsafe { core::arch::asm!("hlt"); }
                    };
                    if confirm.ascii == 0x11 {
                        running = false;
                    }
                } else {
                    running = false;
                }
            }

            // Ctrl+G — goto line
            0x07 => {
                message.clear();
                write!(message, "Goto line (type number then Enter):").ok();
                // Simple line number input — for now just jump to top/bottom
                cursor_row = 0;
                cursor_col = 0;
                scroll_offset = 0;
            }

            // Enter — insert new line
            b'\n' => {
                let current = &lines[cursor_row];
                let rest = current[cursor_col..].to_string();
                lines[cursor_row] = current[..cursor_col].to_string();
                lines.insert(cursor_row + 1, rest);
                cursor_row += 1;
                cursor_col = 0;
                modified = true;
            }

            // Backspace
            0x08 => {
                if cursor_col > 0 {
                    lines[cursor_row].remove(cursor_col - 1);
                    cursor_col -= 1;
                    modified = true;
                } else if cursor_row > 0 {
                    let current_line = lines.remove(cursor_row);
                    cursor_row -= 1;
                    cursor_col = lines[cursor_row].len();
                    lines[cursor_row].push_str(&current_line);
                    modified = true;
                }
            }

            // Tab
            b'\t' => {
                lines[cursor_row].insert_str(cursor_col, "    ");
                cursor_col += 4;
                modified = true;
            }

            // Printable character
            c @ 0x20..=0x7E => {
                lines[cursor_row].insert(cursor_col, c as char);
                cursor_col += 1;
                modified = true;
            }

            _ => {
                // Arrow keys
                match event.key {
                    KeyCode::Up => {
                        if cursor_row > 0 {
                            cursor_row -= 1;
                            cursor_col = cursor_col.min(lines[cursor_row].len());
                        }
                    }
                    KeyCode::Down => {
                        if cursor_row < lines.len() - 1 {
                            cursor_row += 1;
                            cursor_col = cursor_col.min(lines[cursor_row].len());
                        }
                    }
                    KeyCode::Left => {
                        if cursor_col > 0 {
                            cursor_col -= 1;
                        }
                    }
                    KeyCode::Right => {
                        if cursor_col < lines[cursor_row].len() {
                            cursor_col += 1;
                        }
                    }
                    _ => {}
                }
            }
        }

        // Adjust scroll
        if cursor_row < scroll_offset {
            scroll_offset = cursor_row;
        }
        if cursor_row >= scroll_offset + edit_rows {
            scroll_offset = cursor_row - edit_rows + 1;
        }
    }

    // Restore console
    console::clear();
    console::set_color(0xFF, 0xFF, 0xFF);
}

// ============================================================================
// Startup Script Support
// ============================================================================

fn run_autoexec() {
    use crate::storage::vfs;
    use crate::hal::virtio_blk;

    if !virtio_blk::is_available() {
        return;
    }

    // Load hostname — try Windows path first, fall back to legacy
    let hostname_paths = ["/Windows/System32/Config/hostname.cfg", "/system/hostname.cfg"];
    for path in &hostname_paths {
        if let Some(data) = vfs::read_file(path) {
            if let Ok(name) = core::str::from_utf8(&data) {
                let name = name.trim();
                if !name.is_empty() {
                    set_hostname_value(name);
                    break;
                }
            }
        }
    }

    // Run autoexec script — try Windows path first, fall back to legacy
    let autoexec_paths = ["/Windows/System32/Config/autoexec.cfg", "/system/autoexec.cfg"];
    for path in &autoexec_paths {
        if let Some(data) = vfs::read_file(path) {
            if let Ok(script) = core::str::from_utf8(&data) {
                log::info!("Running {}", path);
                for line in script.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    execute_command(line);
                }
                break;
            }
        }
    }

    // Navigate to the user's home directory
    if vfs::is_ready() {
        vfs::cd("/Users/Root");
    }
}

// ============================================================================
// Networking Commands
// ============================================================================

fn cmd_ping(args: &str) {
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
            let start = ticks();
            loop {
                crate::net::poll();
                let elapsed = ticks().wrapping_sub(start);
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

fn cmd_ip(args: &str) {
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

fn cmd_arp() {
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

fn cmd_netstat() {
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

// ============================================================================
// Shell Power-up Commands
// ============================================================================

/// Resolve a file path (for redirection).
fn resolve_path(path: &str) -> String {
    String::from(path.trim())
}

fn cmd_alias(args: &str) {
    aliases_init();
    if args.is_empty() {
        // List all aliases
        let a = ALIASES.lock();
        if let Some(ref map) = *a {
            if map.is_empty() {
                console::println("No aliases defined.");
            } else {
                console::set_color(0xFF, 0xFF, 0x55);
                console::println("Command Aliases:");
                console::set_color(0xFF, 0xFF, 0xFF);
                for (name, value) in map {
                    let mut s = String::new();
                    write!(s, "  {} = {}", name, value).ok();
                    console::println(&s);
                }
            }
        }
        return;
    }

    // Parse alias definition: alias name=command
    if let Some(eq) = args.find('=') {
        let name = args[..eq].trim();
        let value = args[eq+1..].trim();
        if !name.is_empty() && !value.is_empty() {
            alias_set(name, value);
            let mut s = String::new();
            write!(s, "Alias '{}' set to '{}'", name, value).ok();
            console::println(&s);
        } else {
            console::println("Usage: alias name=command");
        }
    } else {
        // Show single alias
        if let Some(value) = alias_get(args.trim()) {
            let mut s = String::new();
            write!(s, "  {} = {}", args.trim(), value).ok();
            console::println(&s);
        } else {
            let mut s = String::new();
            write!(s, "Alias '{}' not found.", args.trim()).ok();
            console::println(&s);
        }
    }
}

fn cmd_unalias(args: &str) {
    if args.is_empty() {
        console::println("Usage: unalias <name>");
        return;
    }
    alias_remove(args.trim());
    let mut s = String::new();
    write!(s, "Alias '{}' removed.", args.trim()).ok();
    console::println(&s);
}

fn cmd_run(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: run <script_file>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(script) => {
                    let mut s = String::new();
                    write!(s, "Running {}...", args.trim()).ok();
                    console::set_color(0xAA, 0xAA, 0xAA);
                    console::println(&s);
                    console::set_color(0xFF, 0xFF, 0xFF);

                    for line in script.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') || line.starts_with("REM") {
                            continue;
                        }
                        execute_command(line);
                    }
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: Script file is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "Error: Script '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_time(args: &str) {
    if args.is_empty() {
        console::println("Usage: time <command>");
        return;
    }

    let start = ticks();
    execute_command(args);
    let elapsed = ticks().wrapping_sub(start);
    let ms = crate::hal::pit::ticks_to_ms(elapsed);

    console::set_color(0xAA, 0xAA, 0xAA);
    let mut s = String::new();
    if ms >= 1000 {
        write!(s, "\nExecution time: {}.{:03}s", ms / 1000, ms % 1000).ok();
    } else {
        write!(s, "\nExecution time: {}ms", ms).ok();
    }
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_sort(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: sort <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let sorted = pipe_sort(text);
                    console::print(&sorted);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_uniq(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: uniq <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    let result = pipe_uniq(text);
                    console::print(&result);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_more(args: &str) {
    use crate::storage::vfs;

    if args.is_empty() {
        console::println("Usage: more <filename>");
        return;
    }

    match vfs::read_file(args.trim()) {
        Some(data) => {
            match core::str::from_utf8(&data) {
                Ok(text) => {
                    pipe_more(text);
                }
                Err(_) => {
                    console::set_color(0xFF, 0x55, 0x55);
                    console::println("Error: File is not valid UTF-8.");
                    console::set_color(0xFF, 0xFF, 0xFF);
                }
            }
        }
        None => {
            let mut s = String::new();
            write!(s, "File '{}' not found.", args.trim()).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

fn cmd_which(args: &str) {
    if args.is_empty() {
        console::println("Usage: which <command>");
        return;
    }

    const ALL_COMMANDS: &[&str] = &[
        "acpi", "alias", "append", "arp", "banner", "beep", "bootinfo", "cal",
        "calc", "cat", "cd", "chdir", "clear", "cls", "color", "copy", "cp",
        "cpu", "date", "del", "desktop", "dir", "disk", "dmesg", "drivers",
        "echo", "edit", "env", "exec", "find", "fortune", "free", "grep",
        "halt", "head", "help", "hexdump", "history", "hostname", "interrupts",
        "ip", "ipconfig", "irq", "kill", "ls", "lsblk", "lspci", "md", "mem",
        "memory", "memmap", "mkdir", "more", "mv", "neofetch", "netstat",
        "panic", "pci", "ping", "poweroff", "priority", "ps", "pwd", "reboot",
        "rename", "rm", "run", "screenfetch", "set", "shutdown", "sleep",
        "sort", "source", "spawn", "stat", "sysinfo", "tail", "tasks", "tick",
        "time", "top", "touch", "tree", "type", "unalias", "uniq", "unset",
        "uptime", "ver", "version", "vol", "wc", "which", "whoami", "write",
        "xxd", "yield",
    ];

    let cmd_name = args.trim();
    if ALL_COMMANDS.contains(&cmd_name) {
        let mut s = String::new();
        write!(s, "{}: built-in shell command", cmd_name).ok();
        console::println(&s);
    } else if alias_get(cmd_name).is_some() {
        let alias_val = alias_get(cmd_name).unwrap();
        let mut s = String::new();
        write!(s, "{}: aliased to '{}'", cmd_name, alias_val).ok();
        console::println(&s);
    } else {
        let mut s = String::new();
        write!(s, "{}: not found", cmd_name).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_calc(args: &str) {
    if args.is_empty() {
        console::println("Usage: calc <expression>");
        console::println("  Supports: +, -, *, /, %, ()");
        console::println("  Example:  calc 2 + 3 * 4");
        console::println("  Example:  calc (10 + 5) * 2");
        return;
    }

    match eval_expr(args) {
        Some(result) => {
            let mut s = String::new();
            write!(s, "= {}", result).ok();
            console::set_color(0x55, 0xFF, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
        None => {
            console::set_color(0xFF, 0x55, 0x55);
            console::println("Error: Invalid expression.");
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

/// Simple recursive descent expression evaluator for integer arithmetic.
fn eval_expr(input: &str) -> Option<i64> {
    let tokens = tokenize_expr(input)?;
    let mut pos = 0;
    let result = parse_add_sub(&tokens, &mut pos)?;
    if pos == tokens.len() {
        Some(result)
    } else {
        None // leftover tokens
    }
}

#[derive(Debug, Clone)]
enum Token {
    Num(i64),
    Op(u8), // b'+', b'-', b'*', b'/', b'%'
    LParen,
    RParen,
}

fn tokenize_expr(input: &str) -> Option<alloc::vec::Vec<Token>> {
    let mut tokens = alloc::vec::Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' => { i += 1; }
            b'+' | b'-' | b'*' | b'/' | b'%' => {
                // Handle negative numbers: if '-' is at start or after operator/lparen
                if bytes[i] == b'-' {
                    let is_unary = tokens.is_empty() ||
                        matches!(tokens.last(), Some(Token::Op(_)) | Some(Token::LParen));
                    if is_unary {
                        // Parse as negative number
                        i += 1;
                        let start = i;
                        while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                        if i == start { return None; }
                        let num_str = core::str::from_utf8(&bytes[start..i]).ok()?;
                        let n: i64 = num_str.parse().ok()?;
                        tokens.push(Token::Num(-n));
                        continue;
                    }
                }
                tokens.push(Token::Op(bytes[i]));
                i += 1;
            }
            b'(' => { tokens.push(Token::LParen); i += 1; }
            b')' => { tokens.push(Token::RParen); i += 1; }
            b'0'..=b'9' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                let num_str = core::str::from_utf8(&bytes[start..i]).ok()?;
                tokens.push(Token::Num(num_str.parse().ok()?));
            }
            _ => return None,
        }
    }
    Some(tokens)
}

fn parse_add_sub(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    let mut left = parse_mul_div(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Op(b'+') => { *pos += 1; left += parse_mul_div(tokens, pos)?; }
            Token::Op(b'-') => { *pos += 1; left -= parse_mul_div(tokens, pos)?; }
            _ => break,
        }
    }
    Some(left)
}

fn parse_mul_div(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    let mut left = parse_primary(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            Token::Op(b'*') => { *pos += 1; left *= parse_primary(tokens, pos)?; }
            Token::Op(b'/') => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0 { return None; } // division by zero
                left /= right;
            }
            Token::Op(b'%') => {
                *pos += 1;
                let right = parse_primary(tokens, pos)?;
                if right == 0 { return None; }
                left %= right;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Option<i64> {
    if *pos >= tokens.len() { return None; }
    match &tokens[*pos] {
        Token::Num(n) => { let v = *n; *pos += 1; Some(v) }
        Token::LParen => {
            *pos += 1;
            let result = parse_add_sub(tokens, pos)?;
            if *pos < tokens.len() && matches!(tokens[*pos], Token::RParen) {
                *pos += 1;
                Some(result)
            } else {
                None // missing closing paren
            }
        }
        _ => None,
    }
}
// ============================================================================
// Phase 5: System Internals Commands
// ============================================================================

fn cmd_dmesg(args: &str) {
    let entries = crate::logging::get_log_entries();
    if entries.is_empty() {
        console::println("(no kernel log entries)");
        return;
    }

    // Parse optional count: dmesg 20 -> last 20 entries
    let count = if args.is_empty() {
        entries.len()
    } else {
        args.trim().parse::<usize>().unwrap_or(entries.len())
    };

    let start = if count >= entries.len() { 0 } else { entries.len() - count };

    console::set_color(0xFF, 0xFF, 0x55);
    let mut s = String::new();
    write!(s, "Kernel Log ({} of {} entries):", entries.len() - start, entries.len()).ok();
    console::println(&s);
    console::set_color(0xCC, 0xCC, 0xCC);

    for entry in &entries[start..] {
        console::println(entry);
    }

    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_free() {
    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;
    let heap = crate::memory::heap::heap_stats();

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Memory Usage Report:");
    console::set_color(0xFF, 0xFF, 0xFF);

    // Header
    console::println("              total       used       free    usage");
    console::println("  ---------  ---------  ---------  ---------  -----");

    let mut s = String::new();

    // Physical memory (in KiB)
    let phys_total = total_frames * 4;
    let phys_used = used_frames * 4;
    let phys_free = free_frames * 4;
    let phys_pct = if phys_total > 0 { phys_used * 100 / phys_total } else { 0 };
    write!(s, "  Physical   {:>6} KiB {:>6} KiB {:>6} KiB   {:>3}%",
        phys_total, phys_used, phys_free, phys_pct).ok();
    console::println(&s);

    // Heap
    s.clear();
    let heap_pct = if heap.total_size > 0 { heap.allocated_bytes * 100 / heap.total_size } else { 0 };
    write!(s, "  Heap       {:>6} KiB {:>6} KiB {:>6} KiB   {:>3}%",
        heap.total_size / 1024, heap.allocated_bytes / 1024,
        heap.free_bytes / 1024, heap_pct).ok();
    console::println(&s);

    // Heap details
    console::set_color(0xAA, 0xAA, 0xAA);
    s.clear();
    write!(s, "  Heap detail: {} free blocks, largest {} KiB",
        heap.free_blocks, heap.largest_free_block / 1024).ok();
    console::println(&s);

    // Usage bars
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("");

    // Physical memory bar
    let bar_w = 40usize;
    let used_bar = if total_frames > 0 { (used_frames * bar_w) / total_frames } else { 0 };
    s.clear();
    write!(s, "  Phys  [").ok();
    for i in 0..bar_w {
        if i < used_bar { write!(s, "#").ok(); } else { write!(s, ".").ok(); }
    }
    write!(s, "] {} MiB / {} MiB", phys_used / 1024, phys_total / 1024).ok();
    console::println(&s);

    // Heap bar
    let heap_used_bar = if heap.total_size > 0 { (heap.allocated_bytes * bar_w) / heap.total_size } else { 0 };
    s.clear();
    write!(s, "  Heap  [").ok();
    for i in 0..bar_w {
        if i < heap_used_bar { write!(s, "#").ok(); } else { write!(s, ".").ok(); }
    }
    write!(s, "] {} KiB / {} KiB", heap.allocated_bytes / 1024, heap.total_size / 1024).ok();
    console::println(&s);
}

fn cmd_top() {
    use crate::core_kernel::scheduler;

    // Interactive task monitor — refreshes until 'q' is pressed
    loop {
        console::clear();
        console::set_color(0xFF, 0xFF, 0x55);

        // Header with uptime and general stats
        let tick_count = ticks();
        let ms = crate::hal::pit::ticks_to_ms(tick_count);
        let seconds = ms / 1000;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let secs = seconds % 60;

        let mut s = String::new();
        write!(s, "CantayaOS Task Monitor  -  Uptime: {:02}:{:02}:{:02}  -  Press Q to quit",
            hours, minutes, secs).ok();
        console::println(&s);

        // Memory summary line
        let free_frames = crate::memory::frame_allocator::free_frame_count();
        let total_frames = crate::memory::frame_allocator::total_frame_count();
        let used_frames = total_frames - free_frames;
        let heap = crate::memory::heap::heap_stats();

        console::set_color(0x55, 0xFF, 0x55);
        s.clear();
        write!(s, "Mem: {} MiB / {} MiB  |  Heap: {} KiB / {} KiB  |  Ctx switches: {}",
            used_frames * 4 / 1024, total_frames * 4 / 1024,
            heap.allocated_bytes / 1024, heap.total_size / 1024,
            scheduler::context_switch_count()).ok();
        console::println(&s);

        console::set_color(0xFF, 0xFF, 0xFF);
        console::println("");

        // Task table header
        console::set_color(0xFF, 0xFF, 0x55);
        console::println("  PID  State       Priority    Switches    CPU Ticks  CPU%   Name");
        console::set_color(0xFF, 0xFF, 0xFF);
        console::println("  ---  --------    --------    --------    ---------  ----   ----");

        // Get tasks
        let tasks = scheduler::task_list();
        let total_cpu: u64 = tasks.iter().map(|(_, _, _, _, _, ticks)| ticks).sum();

        for (id, state, name, switches, priority, cpu_ticks) in &tasks {
            let state_str = match state {
                scheduler::TaskState::Running => "Running ",
                scheduler::TaskState::Ready   => "Ready   ",
                scheduler::TaskState::Blocked => "Blocked ",
                _ => "Unknown ",
            };

            let pct = if total_cpu > 0 {
                (*cpu_ticks * 1000 / total_cpu) as u32
            } else {
                0
            };
            let pct_whole = pct / 10;
            let pct_frac = pct % 10;

            // Color code by state
            match state {
                scheduler::TaskState::Running => console::set_color(0x55, 0xFF, 0x55),
                scheduler::TaskState::Blocked => console::set_color(0xFF, 0xAA, 0x55),
                _ => console::set_color(0xFF, 0xFF, 0xFF),
            }

            s.clear();
            write!(s, "  {:3}  {}  {:8}  {:>10}  {:>11}  {:>2}.{}%   {}",
                id, state_str, priority.name(), switches, cpu_ticks,
                pct_whole, pct_frac, name).ok();
            console::println(&s);
        }

        console::set_color(0xAA, 0xAA, 0xAA);
        s.clear();
        write!(s, "\n  {} task(s), total CPU ticks: {}", tasks.len(), total_cpu).ok();
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);

        // Wait ~1 second for keypress, refresh if no 'q'
        let end_tick = ticks() + 1000;
        loop {
            if ticks() >= end_tick {
                break;
            }
            if let Some(key) = crate::hal::keyboard::try_read_char() {
                if key.pressed && (key.ascii == b'q' || key.ascii == b'Q') {
                    console::clear();
                    return;
                }
            }
            unsafe { core::arch::asm!("hlt"); }
        }
    }
}

fn cmd_drivers() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Loaded Drivers & Subsystems:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let mut s = String::new();

    // CPU / Core
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Core]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    GDT/TSS          loaded     Global Descriptor Table with TSS");
    console::println("    IDT              loaded     Interrupt Descriptor Table (256 vectors)");
    console::println("    8259 PIC         active     Programmable Interrupt Controller");

    s.clear();
    write!(s, "    PIT Timer        active     {} Hz system timer", crate::hal::pit::tick_rate_hz()).ok();
    console::println(&s);

    console::println("    SSE/FPU          enabled    FXSAVE/FXRSTOR per-task context");

    // Input
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Input]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    PS/2 Keyboard    active     IRQ1, scan code set 1");
    console::println("    PS/2 Mouse       active     IRQ12, IntelliMouse (scroll wheel)");

    // Storage
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Storage]");
    console::set_color(0xFF, 0xFF, 0xFF);

    let blk_avail = crate::hal::virtio_blk::is_available();
    if blk_avail {
        let sectors = crate::hal::virtio_blk::capacity_sectors();
        s.clear();
        write!(s, "    virtio-blk       active     {} MiB ({} sectors)", sectors / 2048, sectors).ok();
        console::println(&s);
        console::println("    FAT32            mounted    Volume: CANTAYAOS");
        console::println("    VFS              active     Virtual filesystem layer");
    } else {
        console::println("    virtio-blk       absent");
    }

    // Network
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Network]");
    console::set_color(0xFF, 0xFF, 0xFF);

    if crate::hal::virtio_net::is_available() {
        let mac = crate::hal::virtio_net::mac_address();
        s.clear();
        write!(s, "    virtio-net       active     MAC {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]).ok();
        console::println(&s);
        console::println("    Ethernet         active     Frame TX/RX");
        console::println("    ARP              active     Address Resolution Protocol");
        console::println("    IPv4             active     Internet Protocol v4");
        console::println("    ICMP             active     Echo request/reply (ping)");
        console::println("    UDP              active     User Datagram Protocol");
    } else {
        console::println("    virtio-net       absent");
    }

    // Graphics
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Graphics]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    Framebuffer      active     1920x1080 BGR, double-buffered");
    let (cols, rows) = console::dimensions();
    s.clear();
    write!(s, "    Console          active     {}x{} characters (8x16 font)", cols, rows).ok();
    console::println(&s);
    console::println("    Desktop WM       available  Window manager with taskbar");

    // Audio
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Audio]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    PC Speaker       active     Square wave via PIT channel 2");

    // Power
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Power]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    ACPI             active     S5 shutdown, RSDP v2.0");
    console::println("    RTC              active     CMOS real-time clock");

    // Scheduler
    console::set_color(0x55, 0xFF, 0xFF);
    console::println("\n  [Scheduler]");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("    Scheduler        active     Priority preemptive, 5 levels");
    s.clear();
    write!(s, "    Tasks            {:>3} active  Max 64 tasks, 20 KiB stacks",
        crate::core_kernel::scheduler::active_task_count()).ok();
    console::println(&s);
}

fn cmd_neofetch() {
    let mut s = String::new();

    // ASCII art logo (C letter for CantayaOS)
    let logo = [
        "       ██████████       ",
        "     ██          ██     ",
        "   ██              ██   ",
        "  ██                    ",
        "  ██                    ",
        "  ██                    ",
        "  ██                    ",
        "  ██                    ",
        "   ██              ██   ",
        "     ██          ██     ",
        "       ██████████       ",
    ];

    // System info lines
    let tick_count = ticks();
    let ms = crate::hal::pit::ticks_to_ms(tick_count);
    let uptime_s = ms / 1000;
    let hours = uptime_s / 3600;
    let minutes = (uptime_s % 3600) / 60;
    let secs = uptime_s % 60;

    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;

    let hostname = {
        let h = HOSTNAME.lock();
        let len = HOSTNAME_LEN.load(core::sync::atomic::Ordering::Relaxed);
        let mut hn = String::new();
        for &b in &h[..len] { hn.push(b as char); }
        hn
    };

    let task_count = crate::core_kernel::scheduler::active_task_count();
    let ctx_sw = crate::core_kernel::scheduler::context_switch_count();
    let (cols, rows) = console::dimensions();

    // Build info lines
    let mut info: [String; 11] = core::array::from_fn(|_| String::new());

    write!(info[0], "root@{}", hostname).ok();
    write!(info[1], "OS:      CantayaOS v{}", env!("CARGO_PKG_VERSION")).ok();
    write!(info[2], "Kernel:  Rust bare-metal hybrid (NT-inspired)").ok();
    write!(info[3], "Arch:    x86_64 (AMD64)").ok();
    write!(info[4], "Uptime:  {:02}:{:02}:{:02}", hours, minutes, secs).ok();
    write!(info[5], "Memory:  {} MiB / {} MiB", used_frames * 4 / 1024, total_frames * 4 / 1024).ok();
    write!(info[6], "Shell:   CantayaOS Shell (80+ commands)").ok();
    write!(info[7], "Display: 1920x1080 @ 60Hz (UEFI GOP)").ok();
    write!(info[8], "Console: {}x{} (8x16 font)", cols, rows).ok();
    write!(info[9], "Tasks:   {} active, {} ctx switches", task_count, ctx_sw).ok();
    write!(info[10], "Boot:    UEFI, FAT32 ESP").ok();

    console::println("");

    for (i, line) in logo.iter().enumerate() {
        console::set_color(0x55, 0xAA, 0xFF);
        console::print(line);
        console::print("  ");

        if i < info.len() {
            if i == 0 {
                console::set_color(0x55, 0xFF, 0x55);
            } else {
                console::set_color(0xFF, 0xFF, 0x55);
                // Print label in yellow, value in white
                if let Some(colon_pos) = info[i].find(':') {
                    console::print(&info[i][..colon_pos + 1]);
                    console::set_color(0xFF, 0xFF, 0xFF);
                    console::print(&info[i][colon_pos + 1..]);
                    console::println("");
                    continue;
                }
            }
            console::println(&info[i]);
        } else {
            console::println("");
        }
    }

    // Color palette bar
    console::print("                           ");
    let palette = [
        (0x00, 0x00, 0x00), (0xAA, 0x00, 0x00), (0x00, 0xAA, 0x00), (0xAA, 0xAA, 0x00),
        (0x00, 0x00, 0xAA), (0xAA, 0x00, 0xAA), (0x00, 0xAA, 0xAA), (0xAA, 0xAA, 0xAA),
    ];
    for (r, g, b) in &palette {
        console::set_color(*r, *g, *b);
        console::print("███");
    }
    console::println("");
    console::print("                           ");
    let bright_palette = [
        (0x55, 0x55, 0x55), (0xFF, 0x55, 0x55), (0x55, 0xFF, 0x55), (0xFF, 0xFF, 0x55),
        (0x55, 0x55, 0xFF), (0xFF, 0x55, 0xFF), (0x55, 0xFF, 0xFF), (0xFF, 0xFF, 0xFF),
    ];
    for (r, g, b) in &bright_palette {
        console::set_color(*r, *g, *b);
        console::print("███");
    }
    console::println("");
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_lsblk() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Block Devices:");
    console::set_color(0xFF, 0xFF, 0xFF);

    console::println("  NAME       TYPE    SIZE       MOUNT   FSTYPE");
    console::println("  ----       ----    ----       -----   ------");

    let mut s = String::new();

    if crate::hal::virtio_blk::is_available() {
        let sectors = crate::hal::virtio_blk::capacity_sectors();
        let size_mib = sectors / 2048;

        write!(s, "  vda        disk    {} MiB      /       FAT32", size_mib).ok();
        console::println(&s);

        s.clear();
        write!(s, "  └─vda1     part    {} MiB      /       FAT32 (CANTAYAOS)", size_mib).ok();
        console::println(&s);
    } else {
        console::println("  (no block devices found)");
    }
}