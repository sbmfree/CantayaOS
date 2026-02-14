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

mod commands;
mod editor;
pub(crate) mod calc;

use alloc::string::String;
use crate::hal::keyboard::{self, KeyCode};
use crate::graphics::console;
use core::fmt::Write;

/// Maximum command line length
const MAX_CMD_LEN: usize = 256;

/// Command history depth
pub(crate) const HISTORY_SIZE: usize = 16;

/// System tick counter (incremented by the timer IRQ handler)
static TICK_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// IRQ counters for `interrupts` command
pub(crate) static IRQ_TIMER_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub(crate) static IRQ_KEYBOARD_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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

pub(crate) fn get_hostname() -> String {
    let buf = HOSTNAME.lock();
    let len = HOSTNAME_LEN.load(core::sync::atomic::Ordering::Relaxed);
    if len == 0 {
        return String::from("cantaya");
    }
    core::str::from_utf8(&buf[..len]).unwrap_or("cantaya").into()
}

pub(crate) fn set_hostname_value(name: &str) {
    let mut buf = HOSTNAME.lock();
    let len = name.len().min(63);
    buf[..len].copy_from_slice(&name.as_bytes()[..len]);
    HOSTNAME_LEN.store(len, core::sync::atomic::Ordering::Relaxed);
}

/// Environment variables
use alloc::collections::BTreeMap;
pub(crate) static ENV_VARS: spin::Mutex<Option<BTreeMap<String, String>>> = spin::Mutex::new(None);

pub(crate) fn env_init() {
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

pub(crate) fn env_get(key: &str) -> Option<String> {
    env_init();
    let env = ENV_VARS.lock();
    env.as_ref().and_then(|m| m.get(key).cloned())
}

pub(crate) fn env_set(key: &str, value: &str) {
    env_init();
    let mut env = ENV_VARS.lock();
    if let Some(ref mut m) = *env {
        m.insert(key.into(), value.into());
    }
}

pub(crate) fn env_remove(key: &str) {
    env_init();
    let mut env = ENV_VARS.lock();
    if let Some(ref mut m) = *env {
        m.remove(key);
    }
}

/// Command aliases — user-defined shortcut names for commands
pub(crate) static ALIASES: spin::Mutex<Option<BTreeMap<String, String>>> = spin::Mutex::new(None);

pub(crate) fn aliases_init() {
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

pub(crate) fn alias_get(name: &str) -> Option<String> {
    aliases_init();
    let a = ALIASES.lock();
    a.as_ref().and_then(|m| m.get(name).cloned())
}

pub(crate) fn alias_set(name: &str, value: &str) {
    aliases_init();
    let mut a = ALIASES.lock();
    if let Some(ref mut m) = *a {
        m.insert(name.into(), value.into());
    }
}

pub(crate) fn alias_remove(name: &str) {
    aliases_init();
    let mut a = ALIASES.lock();
    if let Some(ref mut m) = *a {
        m.remove(name);
    }
}

/// Static command history accessible from the `history` command
pub(crate) static CMD_HISTORY: spin::Mutex<History> = spin::Mutex::new(History::new());

/// Command history ring buffer
pub(crate) struct History {
    pub entries: [[u8; MAX_CMD_LEN]; HISTORY_SIZE],
    pub lengths: [usize; HISTORY_SIZE],
    pub count: usize,
    pub write_idx: usize,
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
pub(crate) fn unix_to_win_path(path: &str) -> String {
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

/// Convert a Windows-style path to a Unix-style VFS path.
pub(crate) fn win_to_unix_path(path: &str) -> String {
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
pub(crate) fn execute_command(input: &str) {
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
pub(crate) fn pipe_sort(input: &str) -> String {
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
pub(crate) fn pipe_uniq(input: &str) -> String {
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
pub(crate) fn pipe_more(input: &str) {
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
        // System commands
        "help" | "?" => commands::system::cmd_help(),
        "ver" | "version" => commands::system::cmd_version(),
        "mem" | "memory" => commands::system::cmd_memory(),
        "cpu" => commands::system::cmd_cpu(),
        "uptime" => commands::system::cmd_uptime(),
        "clear" | "cls" => commands::system::cmd_clear(),
        "halt" | "shutdown" | "poweroff" => commands::system::cmd_halt(),
        "reboot" => commands::system::cmd_reboot(),
        "tasks" | "ps" => commands::system::cmd_tasks(),
        "spawn" => commands::system::cmd_spawn(args),
        "kill" => commands::system::cmd_kill(args),
        "yield" => commands::system::cmd_yield(),
        "tick" => commands::system::cmd_tick(),
        "interrupts" | "irq" => commands::system::cmd_interrupts(),
        "hexdump" => commands::system::cmd_hexdump(args),
        "pci" | "lspci" => commands::system::cmd_pci(),
        "sysinfo" => commands::system::cmd_sysinfo(),
        "date" => commands::system::cmd_date(),
        "desktop" => commands::system::cmd_desktop(),
        "acpi" => commands::system::cmd_acpi(),
        "sleep" => commands::system::cmd_sleep(args),
        "priority" => commands::system::cmd_priority(args),
        "bootinfo" => commands::system::cmd_bootinfo(),
        "memmap" => commands::system::cmd_memmap(),
        "panic" => commands::system::cmd_panic(),
        "dmesg" => commands::system::cmd_dmesg(args),
        "free" => commands::system::cmd_free(),
        "top" => commands::system::cmd_top(),
        "vmm" => commands::system::cmd_vmm(),
        "vtop" => commands::system::cmd_vtop(args),
        "drivers" => commands::system::cmd_drivers(),
        "neofetch" | "screenfetch" => commands::system::cmd_neofetch(),
        "lsblk" => commands::system::cmd_lsblk(),

        // Filesystem commands
        "ls" | "dir" => commands::fs::cmd_ls(args),
        "cat" | "type" => commands::fs::cmd_cat(args),
        "write" => commands::fs::cmd_write(args),
        "mkdir" | "md" => commands::fs::cmd_mkdir(args),
        "rm" | "del" => commands::fs::cmd_rm(args),
        "cp" | "copy" => commands::fs::cmd_cp(args),
        "disk" => commands::fs::cmd_disk(),
        "cd" | "chdir" => commands::fs::cmd_cd(args),
        "pwd" => commands::fs::cmd_pwd(),
        "vol" => commands::fs::cmd_vol(),
        "grep" => commands::fs::cmd_grep(args),
        "find" => commands::fs::cmd_find(args),
        "wc" => commands::fs::cmd_wc(args),
        "head" => commands::fs::cmd_head(args),
        "tail" => commands::fs::cmd_tail(args),
        "touch" => commands::fs::cmd_touch(args),
        "stat" => commands::fs::cmd_stat(args),
        "tree" => commands::fs::cmd_tree(args),
        "append" => commands::fs::cmd_append(args),
        "rename" | "mv" => commands::fs::cmd_rename(args),
        "xxd" => commands::fs::cmd_xxd(args),
        "sort" => commands::fs::cmd_sort(args),
        "uniq" => commands::fs::cmd_uniq(args),
        "more" => commands::fs::cmd_more(args),

        // Network commands
        "ping" => commands::net::cmd_ping(args),
        "ip" | "ipconfig" => commands::net::cmd_ip(args),
        "arp" => commands::net::cmd_arp(),
        "netstat" => commands::net::cmd_netstat(),

        // Miscellaneous commands
        "echo" => commands::misc::cmd_echo(args),
        "beep" => commands::misc::cmd_beep(args),
        "hostname" => commands::misc::cmd_hostname(args),
        "whoami" => commands::misc::cmd_whoami(),
        "history" => commands::misc::cmd_history(),
        "cal" => commands::misc::cmd_cal(),
        "fortune" => commands::misc::cmd_fortune(),
        "banner" => commands::misc::cmd_banner(args),
        "env" | "set" => commands::misc::cmd_env(args),
        "unset" => commands::misc::cmd_unset(args),
        "color" => commands::misc::cmd_color(args),
        "alias" => commands::misc::cmd_alias(args),
        "unalias" => commands::misc::cmd_unalias(args),
        "run" | "exec" | "source" => commands::misc::cmd_run(args),
        "time" => commands::misc::cmd_time(args),
        "which" => commands::misc::cmd_which(args),
        "calc" => commands::misc::cmd_calc(args),

        // Editor
        "edit" => editor::cmd_edit(args),

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
// Color Scheme Helpers
// ============================================================================

/// Apply a color scheme by name. Returns true if recognized.
pub(crate) fn apply_color_scheme(name: &str) -> bool {
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
pub(crate) fn save_color_scheme(name: &str) {
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
// Utility Helpers
// ============================================================================

/// Format a file size with comma separators (Windows dir style).
pub(crate) fn format_size_commas(size: u32) -> String {
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

/// Resolve a file path (currently pass-through, but could resolve relative paths).
pub(crate) fn resolve_path(path: &str) -> String {
    String::from(path.trim())
}
