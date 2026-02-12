//! CantayaOS Interactive Shell
//!
//! NT-like command interpreter with:
//!   - Pipe operator (cmd1 | cmd2 | cmd3)
//!   - I/O redirection (>, >>)
//!   - Shell aliases (alias/unalias)
//!   - Shell scripting (source/sh)
//!   - Environment variable expansion ($VAR)
//!   - Tab completion (commands + paths)
//!   - History navigation (up/down arrows)
//!   - 50+ built-in commands

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use alloc::format;

use crate::{kprint, kprintln};
use crate::fs;
use crate::hal;
use crate::process;
use crate::mm;

extern crate alloc;

/// All command names for tab completion
const COMMANDS: &[&str] = &[
    "alias", "arp", "awk", "basename", "cal", "cat", "cd", "chmod", "chown",
    "clear", "cls", "cp", "cut", "date", "df", "dig", "dirname", "dmesg",
    "du", "echo", "edit", "env", "export", "false", "fdisk", "find", "for",
    "free", "grep", "groups", "halt", "head", "help", "hexdump", "history",
    "hostname", "id", "if", "ifconfig", "info", "kill", "ln", "logger",
    "login", "ls", "lsblk", "lspci", "man", "mem", "mkdir", "mount", "mv",
    "neofetch", "netstat", "nslookup", "panic", "passwd", "ping", "pkg",
    "printenv", "ps", "pwd", "readlink", "reboot", "rev", "rm", "rmdir",
    "route", "seq", "service", "set", "sh", "shutdown", "sleep", "sort",
    "source", "stat", "su", "sysinfo", "syslog", "tail", "tee", "test",
    "top", "touch", "tr", "tree", "true", "type", "uname", "unalias",
    "unset", "uptime", "ver", "version", "wc", "while", "which", "whoami",
    "write", "xargs", "xxd", "yes",
];

/// Read a file into a String, reading in chunks until EOF.
/// Caps at 64KB to prevent OOM. Returns None on failure.
fn read_file_string(path: &str) -> Option<String> {
    let mut handle = fs::open(path, fs::AccessMode::Read).ok()?;
    let mut content = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match fs::read(&mut handle, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                content.extend_from_slice(&buf[..n]);
                if content.len() >= 65536 { break; }
            }
            Err(_) => break,
        }
    }
    fs::close(handle);
    core::str::from_utf8(&content).ok().map(|s| String::from(s))
}

/// Read a file into raw bytes, reading in chunks until EOF.
/// Caps at 64KB. Returns None on failure.
fn read_file_bytes(path: &str) -> Option<Vec<u8>> {
    let mut handle = fs::open(path, fs::AccessMode::Read).ok()?;
    let mut content = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match fs::read(&mut handle, &mut buf) {
            Ok(0) => break,
            Ok(n) => {
                content.extend_from_slice(&buf[..n]);
                if content.len() >= 65536 { break; }
            }
            Err(_) => break,
        }
    }
    fs::close(handle);
    Some(content)
}

/// ANSI color constants
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const CYAN: &str = "\x1b[36m";
const WHITE: &str = "\x1b[97m";

/// Shell state
struct Shell {
    cwd: String,
    env: BTreeMap<String, String>,
    aliases: BTreeMap<String, String>,
    history: Vec<String>,
    last_exit_code: i32,
    running: bool,
}

impl Shell {
    fn new() -> Self {
        let mut env = BTreeMap::new();
        env.insert(String::from("HOME"), String::from("/home"));
        env.insert(String::from("PATH"), String::from("/bin:/sbin"));
        env.insert(String::from("SHELL"), String::from("/bin/csh"));
        env.insert(String::from("USER"), String::from("root"));
        env.insert(String::from("HOSTNAME"), String::from("cantaya"));
        env.insert(String::from("TERM"), String::from("vt100"));
        env.insert(String::from("LANG"), String::from("en_US.UTF-8"));
        env.insert(String::from("OSTYPE"), String::from("CantayaOS"));
        env.insert(String::from("PS1"), String::from("cantaya"));

        let mut aliases = BTreeMap::new();
        aliases.insert(String::from("ll"), String::from("ls -l"));
        aliases.insert(String::from("la"), String::from("ls -a"));
        aliases.insert(String::from(".."), String::from("cd .."));
        aliases.insert(String::from("cls"), String::from("clear"));
        aliases.insert(String::from("dir"), String::from("ls"));
        aliases.insert(String::from("del"), String::from("rm"));
        aliases.insert(String::from("md"), String::from("mkdir"));

        Shell {
            cwd: String::from("/"),
            env,
            aliases,
            history: Vec::new(),
            last_exit_code: 0,
            running: true,
        }
    }
}

/// Entry point — called from kernel_main
pub fn run() -> ! {
    kprintln!("");
    kprintln!("  {}CantayaOS Shell v{}{}", BOLD, crate::KERNEL_VERSION, RESET);
    kprintln!("  Type '{}help{}' for available commands. Tab for completion.", BOLD, RESET);
    kprintln!("");

    let mut shell = Shell::new();

    // Source /etc/profile if it exists
    source_file(&mut shell, "/etc/profile");

    loop {
        if !shell.running {
            kprintln!("");
            kprintln!("  {}Halting system...{}", DIM, RESET);
            kprintln!("  {}It is now safe to close QEMU.{}", DIM, RESET);
            loop { crate::arch::aarch64::cpu::halt(); }
        }
        print_prompt(&shell);
        let line = read_line_advanced(&mut shell);
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // History expansion: !!, !N, !prefix
        let line = match expand_history(&shell, line) {
            Some(expanded) => {
                if expanded != line {
                    kprintln!("{}", expanded);
                }
                shell.history.push(expanded.clone());
                expanded
            }
            None => {
                // Invalid history ref
                continue;
            }
        };

        execute_line(&mut shell, &line);
    }
}

/// Source a script file (execute each line)
fn source_file(shell: &mut Shell, path: &str) {
    let full = resolve_path(&shell.cwd, path);
    if let Some(content) = read_file_string(&full) {
        for script_line in content.lines() {
            let trimmed = script_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            execute_line(shell, trimmed);
        }
    }
}

/// Print the shell prompt
fn print_prompt(shell: &Shell) {
    kprint!("{}{}root{}{}: {}{}{}${} ", BOLD, CYAN, RESET, DIM, BLUE, shell.cwd, RESET, " ");
}

/// Advanced line reader with history, tab completion, and shortcuts
fn read_line_advanced(shell: &mut Shell) -> String {
    let mut buf = String::new();
    let mut cursor = 0usize;
    let mut hist_idx: Option<usize> = None;
    let mut saved_line = String::new();

    loop {
        let ch = hal::console::read_char_blocking();
        match ch {
            // Enter
            b'\r' | b'\n' => {
                kprint!("\r\n");
                return buf;
            }
            // Backspace
            0x08 | 0x7F => {
                if cursor > 0 {
                    cursor -= 1;
                    buf.remove(cursor);
                    // Redraw from cursor
                    kprint!("\x08");
                    let tail: String = buf[cursor..].chars().collect();
                    kprint!("{} ", tail);
                    for _ in 0..tail.len() + 1 {
                        kprint!("\x08");
                    }
                }
            }
            // Tab — completion
            b'\t' => {
                if let Some(completed) = tab_complete(&buf, &shell.cwd) {
                    // Clear current line display
                    for _ in 0..cursor {
                        kprint!("\x08");
                    }
                    for _ in 0..buf.len() {
                        kprint!(" ");
                    }
                    for _ in 0..buf.len() {
                        kprint!("\x08");
                    }
                    buf = completed;
                    cursor = buf.len();
                    kprint!("{}", buf);
                }
            }
            // Ctrl-C
            0x03 => {
                kprint!("^C\r\n");
                return String::new();
            }
            // Ctrl-U — clear line
            0x15 => {
                for _ in 0..cursor {
                    kprint!("\x08 \x08");
                }
                let rest = String::from(&buf[cursor..]);
                for _ in 0..rest.len() {
                    kprint!(" ");
                }
                for _ in 0..rest.len() {
                    kprint!("\x08");
                }
                buf = rest;
                cursor = 0;
                kprint!("{}", buf);
                for _ in 0..buf.len() {
                    kprint!("\x08");
                }
            }
            // Ctrl-L — clear screen
            0x0C => {
                kprint!("\x1b[2J\x1b[H");
                print_prompt(shell);
                kprint!("{}", buf);
                // Move cursor back if needed
                for _ in cursor..buf.len() {
                    kprint!("\x08");
                }
            }
            // Ctrl-W — delete word backwards
            0x17 => {
                let mut deleted = 0;
                // Skip trailing spaces
                while cursor > 0 && buf.as_bytes()[cursor - 1] == b' ' {
                    cursor -= 1;
                    buf.remove(cursor);
                    deleted += 1;
                }
                // Delete word
                while cursor > 0 && buf.as_bytes()[cursor - 1] != b' ' {
                    cursor -= 1;
                    buf.remove(cursor);
                    deleted += 1;
                }
                // Redraw
                for _ in 0..deleted {
                    kprint!("\x08");
                }
                let tail: String = buf[cursor..].chars().collect();
                kprint!("{}", tail);
                for _ in 0..deleted {
                    kprint!(" ");
                }
                for _ in 0..tail.len() + deleted {
                    kprint!("\x08");
                }
            }
            // Ctrl-A — move to beginning
            0x01 => {
                while cursor > 0 {
                    kprint!("\x08");
                    cursor -= 1;
                }
            }
            // Ctrl-E — move to end
            0x05 => {
                let tail = &buf[cursor..];
                kprint!("{}", tail);
                cursor = buf.len();
            }
            // Ctrl-K — delete to end of line
            0x0B => {
                let right = buf.len() - cursor;
                for _ in 0..right {
                    kprint!(" ");
                }
                for _ in 0..right {
                    kprint!("\x08");
                }
                buf.truncate(cursor);
            }
            // Escape sequences (arrow keys)
            0x1B => {
                let b1 = hal::console::read_char_blocking();
                if b1 == b'[' {
                    let b2 = hal::console::read_char_blocking();
                    match b2 {
                        // Up arrow — previous history
                        b'A' => {
                            if shell.history.is_empty() {
                                continue;
                            }
                            if hist_idx.is_none() {
                                saved_line = buf.clone();
                                hist_idx = Some(shell.history.len().saturating_sub(1));
                            } else if let Some(idx) = hist_idx {
                                if idx > 0 {
                                    hist_idx = Some(idx - 1);
                                }
                            }
                            if let Some(idx) = hist_idx {
                                // Clear display
                                for _ in 0..cursor { kprint!("\x08"); }
                                for _ in 0..buf.len() { kprint!(" "); }
                                for _ in 0..buf.len() { kprint!("\x08"); }
                                buf = shell.history[idx].clone();
                                cursor = buf.len();
                                kprint!("{}", buf);
                            }
                        }
                        // Down arrow — next history
                        b'B' => {
                            if let Some(idx) = hist_idx {
                                // Clear display
                                for _ in 0..cursor { kprint!("\x08"); }
                                for _ in 0..buf.len() { kprint!(" "); }
                                for _ in 0..buf.len() { kprint!("\x08"); }
                                if idx + 1 < shell.history.len() {
                                    hist_idx = Some(idx + 1);
                                    buf = shell.history[idx + 1].clone();
                                } else {
                                    hist_idx = None;
                                    buf = saved_line.clone();
                                }
                                cursor = buf.len();
                                kprint!("{}", buf);
                            }
                        }
                        // Right arrow
                        b'C' => {
                            if cursor < buf.len() {
                                kprint!("\x1b[C");
                                cursor += 1;
                            }
                        }
                        // Left arrow
                        b'D' => {
                            if cursor > 0 {
                                kprint!("\x08");
                                cursor -= 1;
                            }
                        }
                        // Home
                        b'H' => {
                            while cursor > 0 {
                                kprint!("\x08");
                                cursor -= 1;
                            }
                        }
                        // End
                        b'F' => {
                            let tail = &buf[cursor..];
                            kprint!("{}", tail);
                            cursor = buf.len();
                        }
                        // Delete key: ESC [ 3 ~
                        b'3' => {
                            let _tilde = hal::console::read_char_blocking();
                            if cursor < buf.len() {
                                buf.remove(cursor);
                                let tail: String = buf[cursor..].chars().collect();
                                kprint!("{} ", tail);
                                for _ in 0..tail.len() + 1 {
                                    kprint!("\x08");
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            // Printable
            ch if ch >= 0x20 && ch < 0x7F => {
                buf.insert(cursor, ch as char);
                let tail: String = buf[cursor..].chars().collect();
                kprint!("{}", tail);
                cursor += 1;
                for _ in cursor..buf.len() {
                    kprint!("\x08");
                }
            }
            _ => {}
        }
    }
}

/// Tab completion for commands and file paths
fn tab_complete(partial: &str, cwd: &str) -> Option<String> {
    let parts: Vec<&str> = partial.splitn(2, ' ').collect();

    if parts.len() == 1 && !partial.ends_with(' ') {
        // Command completion
        let prefix = parts[0];
        let matches: Vec<&&str> = COMMANDS.iter().filter(|c| c.starts_with(prefix)).collect();
        if matches.len() == 1 {
            return Some(format!("{} ", matches[0]));
        } else if matches.len() > 1 {
            kprint!("\r\n");
            for m in &matches {
                kprint!("  {}{}{}", CYAN, m, RESET);
            }
            kprint!("\r\n");
            return Some(String::from(partial));
        }
        return None;
    }

    // Path completion
    let path_part = if parts.len() > 1 { parts[1] } else { "" };
    let (dir, file_prefix): (&str, &str) = if let Some(slash_pos) = path_part.rfind('/') {
        (&path_part[..=slash_pos], &path_part[slash_pos + 1..])
    } else {
        ("", path_part)
    };

    let search_dir = if dir.is_empty() {
        String::from(cwd)
    } else {
        resolve_path(cwd, dir)
    };

    if let Ok(entries) = fs::readdir(&search_dir) {
        let matches: Vec<&fs::DirEntry> = entries
            .iter()
            .filter(|e| e.name.starts_with(file_prefix))
            .collect();

        if matches.len() == 1 {
            let entry = &matches[0];
            let suffix = if entry.file_type == fs::FileType::Directory { "/" } else { " " };
            return Some(format!("{} {}{}{}", parts[0], dir, entry.name, suffix));
        } else if matches.len() > 1 {
            kprint!("\r\n");
            for m in &matches {
                let color = if m.file_type == fs::FileType::Directory { BLUE } else { WHITE };
                kprint!("  {}{}{}", color, m.name, RESET);
            }
            kprint!("\r\n");
            // Find common prefix
            let first = &matches[0].name;
            let mut common_len = first.len();
            for m in &matches[1..] {
                let cl = first
                    .chars()
                    .zip(m.name.chars())
                    .take_while(|(a, b)| a == b)
                    .count();
                if cl < common_len {
                    common_len = cl;
                }
            }
            if common_len > file_prefix.len() {
                let common: String = first.chars().take(common_len).collect();
                return Some(format!("{} {}{}", parts[0], dir, common));
            }
            return Some(String::from(partial));
        }
    }
    None
}

// ─── Pipeline & Redirection ──────────────────────────────────────────────

/// Execute a full command line (handles pipes, redirection, aliases, variables)
fn execute_line(shell: &mut Shell, raw_line: &str) {
    let trimmed = raw_line.trim();

    // Check for control flow FIRST (before variable expansion & semicolon splitting)
    // These contain variables that must be expanded per-iteration
    if trimmed.starts_with("if ") || trimmed == "if" {
        execute_if(shell, trimmed);
        return;
    }
    if trimmed.starts_with("for ") || trimmed == "for" {
        execute_for(shell, trimmed);
        return;
    }
    if trimmed.starts_with("while ") || trimmed == "while" {
        execute_while(shell, trimmed);
        return;
    }

    let line = expand_vars(shell, raw_line);
    let line = expand_alias(shell, &line);

    // Handle semicolons: cmd1; cmd2; cmd3
    if line.contains(';') {
        for segment in line.split(';') {
            let seg = segment.trim();
            if !seg.is_empty() {
                execute_pipeline(shell, seg);
            }
        }
        return;
    }

    execute_pipeline(shell, &line);
}

/// Execute a single pipeline (cmd1 | cmd2 | cmd3) with optional redirection
fn execute_pipeline(shell: &mut Shell, line: &str) {
    // Parse output redirection on the LAST command
    // e.g., "ls | grep foo > output.txt"
    // We only support redirect on the entire pipeline output

    let (pipe_line, redirect) = parse_redirect(line);

    let segments: Vec<&str> = pipe_line.split('|').collect();

    let mut stdin_data: Option<String> = None;

    for (i, segment) in segments.iter().enumerate() {
        let seg = segment.trim();
        if seg.is_empty() {
            continue;
        }

        let is_last = i == segments.len() - 1;

        // Parse command and args
        let tokens: Vec<&str> = seg.split_whitespace().collect();
        if tokens.is_empty() {
            continue;
        }

        let cmd = tokens[0];
        let args = &tokens[1..];

        if is_last && redirect.is_none() {
            // Last command, no redirect — output to console
            execute_command(shell, cmd, args, stdin_data.as_deref(), None);
            stdin_data = None;
        } else {
            // Capture output into buffer
            let mut output = String::new();
            execute_command(shell, cmd, args, stdin_data.as_deref(), Some(&mut output));
            stdin_data = Some(output);
        }
    }

    // Handle redirect if present
    if let Some((redir_type, redir_path)) = redirect {
        if let Some(data) = stdin_data {
            let full_path = resolve_path(&shell.cwd, &redir_path);
            match redir_type {
                RedirectType::Overwrite => {
                    // Create or truncate
                    let _ = fs::vfs::unlink(&full_path);
                    if let Ok(mut handle) = fs::create(&full_path) {
                        let _ = fs::write(&mut handle, data.as_bytes());
                        fs::close(handle);
                    }
                }
                RedirectType::Append => {
                    // Read existing + append
                    let existing = read_file_bytes(&full_path).unwrap_or_default();
                    let _ = fs::vfs::unlink(&full_path);
                    if let Ok(mut handle) = fs::create(&full_path) {
                        if !existing.is_empty() {
                            let _ = fs::write(&mut handle, &existing);
                        }
                        let _ = fs::write(&mut handle, data.as_bytes());
                        fs::close(handle);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
enum RedirectType {
    Overwrite, // >
    Append,    // >>
}

/// Parse redirection from end of line. Returns (command_part, optional redirect)
fn parse_redirect(line: &str) -> (&str, Option<(RedirectType, String)>) {
    // Check for >> first, then >
    if let Some(pos) = line.rfind(">>") {
        let cmd = line[..pos].trim();
        let file = line[pos + 2..].trim();
        if !file.is_empty() {
            return (cmd, Some((RedirectType::Append, String::from(file))));
        }
    }
    if let Some(pos) = line.rfind('>') {
        // Make sure it's not >>
        if pos == 0 || line.as_bytes()[pos - 1] != b'>' {
            let cmd = line[..pos].trim();
            let file = line[pos + 1..].trim();
            if !file.is_empty() {
                return (cmd, Some((RedirectType::Overwrite, String::from(file))));
            }
        }
    }
    (line, None)
}

/// Expand environment variables ($VAR and $?)
fn expand_vars(shell: &Shell, line: &str) -> String {
    let mut result = String::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            i += 1;
            if i < bytes.len() && bytes[i] == b'?' {
                result.push_str(&format!("{}", shell.last_exit_code));
                i += 1;
            } else {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                if i > start {
                    let var_name = &line[start..i];
                    if let Some(val) = shell.env.get(var_name) {
                        result.push_str(val);
                    }
                } else {
                    result.push('$');
                }
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    result
}

/// Expand aliases (only first word)
fn expand_alias(shell: &Shell, line: &str) -> String {
    let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
    if let Some(alias_val) = shell.aliases.get(parts[0]) {
        if parts.len() > 1 {
            format!("{} {}", alias_val, parts[1])
        } else {
            alias_val.clone()
        }
    } else {
        String::from(line)
    }
}

/// Resolve a path relative to cwd
fn resolve_path(cwd: &str, path: &str) -> String {
    if path.starts_with('/') {
        normalize_path(path)
    } else {
        let combined = if cwd.ends_with('/') {
            format!("{}{}", cwd, path)
        } else {
            format!("{}/{}", cwd, path)
        };
        normalize_path(&combined)
    }
}

/// Normalize a path (resolve . and ..)
fn normalize_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        String::from("/")
    } else {
        let mut result = String::new();
        for part in &parts {
            result.push('/');
            result.push_str(part);
        }
        result
    }
}

// ─── Command Dispatch ────────────────────────────────────────────────────

/// Write to either console or buffer
macro_rules! out {
    ($output:expr, $($arg:tt)*) => {
        match $output {
            Some(ref mut buf) => {
                use core::fmt::Write;
                let _ = core::write!(buf, $($arg)*);
            }
            None => {
                $crate::kprint!($($arg)*);
            }
        }
    };
}

macro_rules! outln {
    ($output:expr) => {{
        out!($output, "\n")
    }};
    ($output:expr, $($arg:tt)*) => {{
        out!($output, $($arg)*);
        out!($output, "\n")
    }};
}

/// Execute a single command. If output is Some, capture output there; else print to console.
fn execute_command(
    shell: &mut Shell,
    cmd: &str,
    args: &[&str],
    stdin: Option<&str>,
    mut output: Option<&mut String>,
) {
    shell.last_exit_code = 0;

    match cmd {
        "help" => cmd_help(&mut output),
        "clear" | "cls" => cmd_clear(&mut output),
        "uname" => cmd_uname(args, &mut output),
        "ps" => cmd_ps(&mut output),
        "mem" | "free" => cmd_mem(&mut output),
        "uptime" => cmd_uptime(&mut output),
        "ls" | "dir" => cmd_ls(shell, args, &mut output),
        "cat" | "type" => cmd_cat(shell, args, stdin, &mut output),
        "mkdir" | "md" => cmd_mkdir(shell, args, &mut output),
        "touch" => cmd_touch(shell, args, &mut output),
        "write" => cmd_write(shell, args, &mut output),
        "rm" | "del" => cmd_rm(shell, args, &mut output),
        "rmdir" => cmd_rmdir(shell, args, &mut output),
        "echo" => cmd_echo(args, &mut output),
        "info" | "sysinfo" => cmd_sysinfo(&mut output),
        "lspci" => cmd_lspci(&mut output),
        "ver" | "version" => cmd_version(&mut output),
        "date" => cmd_date(&mut output),
        "halt" | "shutdown" => { shell.running = false; }
        "reboot" => {
            outln!(output, "\n  {}Rebooting...{}", YELLOW, RESET);
            shell.running = false;
        }
        "panic" => panic!("User-triggered kernel panic via shell"),
        "history" => cmd_history(shell, &mut output),
        "cd" => cmd_cd(shell, args, &mut output),
        "pwd" => cmd_pwd(shell, &mut output),
        "tree" => cmd_tree(shell, args, &mut output),
        "stat" => cmd_stat(shell, args, &mut output),
        "hexdump" | "xxd" => cmd_hexdump(shell, args, stdin, &mut output),
        "hostname" => cmd_hostname(args, &mut output),
        "whoami" => outln!(output, "root"),
        "cp" => cmd_cp(shell, args, &mut output),
        "mv" => cmd_mv(shell, args, &mut output),
        "dmesg" => cmd_dmesg(args, &mut output),
        "env" | "printenv" => cmd_env(shell, &mut output),
        "set" | "export" => cmd_set(shell, args, &mut output),
        "unset" => cmd_unset(shell, args, &mut output),
        "neofetch" => cmd_neofetch(&mut output),
        "kill" => cmd_kill(args, &mut output),
        "wc" => cmd_wc(shell, args, stdin, &mut output),
        "head" => cmd_head(shell, args, stdin, &mut output),
        "tail" => cmd_tail(shell, args, stdin, &mut output),
        "grep" => cmd_grep(shell, args, stdin, &mut output),
        "find" => cmd_find(shell, args, &mut output),
        "sort" => cmd_sort(shell, args, stdin, &mut output),
        "df" => cmd_df(&mut output),
        "cal" => cmd_cal(&mut output),
        "seq" => cmd_seq(args, &mut output),
        "sleep" => cmd_sleep(args, &mut output),
        "tee" => cmd_tee(shell, args, stdin, &mut output),
        "alias" => cmd_alias(shell, args, &mut output),
        "unalias" => cmd_unalias(shell, args, &mut output),
        "source" | "sh" => cmd_source(shell, args),
        "which" => cmd_which(args, &mut output),
        "ln" => cmd_ln(shell, args, &mut output),
        "true" => { shell.last_exit_code = 0; }
        "false" => { shell.last_exit_code = 1; }
        "test" => cmd_test(shell, args, &mut output),
        "ping" => cmd_ping(args, &mut output),
        "ifconfig" => cmd_ifconfig(&mut output),
        "netstat" => cmd_netstat(args, &mut output),
        "arp" => cmd_arp(&mut output),
        "route" => cmd_route(&mut output),
        "id" => cmd_id(&mut output),
        "groups" => cmd_groups(&mut output),
        "logger" => cmd_logger(args, &mut output),
        "edit" => cmd_edit(shell, args),
        "man" => cmd_man(args, &mut output),
        "mount" => cmd_mount(&mut output),
        "syslog" => cmd_syslog(args, &mut output),
        "service" => cmd_service(args, &mut output),
        "du" => cmd_du(shell, args, &mut output),
        "fdisk" => cmd_fdisk(args, &mut output),
        "lsblk" => cmd_lsblk(&mut output),
        "top" => cmd_top(&mut output),
        "nslookup" | "dig" => cmd_nslookup(args, &mut output),
        "pkg" => cmd_pkg(shell, args, &mut output),
        "passwd" => cmd_passwd(&mut output),
        "su" => cmd_su(args, &mut output),
        "login" => cmd_login(&mut output),
        "awk" => cmd_awk(args, stdin, &mut output),
        "readlink" => cmd_readlink(shell, args, &mut output),
        "chmod" => cmd_chmod(args, &mut output),
        "chown" => cmd_chown(args, &mut output),
        "xargs" => cmd_xargs(shell, args, stdin, &mut output),
        "basename" => cmd_basename(args, &mut output),
        "dirname" => cmd_dirname(args, &mut output),
        "cut" => cmd_cut(args, stdin, &mut output),
        "tr" => cmd_tr(args, stdin, &mut output),
        "rev" => cmd_rev(stdin, &mut output),
        "yes" => cmd_yes(args, &mut output),
        "run" | "exec" => cmd_run(shell, args, &mut output),
        "readelf" => cmd_readelf(shell, args, &mut output),
        _ => {
            // Check if it's a script file
            let full_path = resolve_path(&shell.cwd, cmd);
            if let Ok(mut handle) = fs::open(&full_path, fs::AccessMode::Read) {
                let mut buf = [0u8; 8];
                if let Ok(n) = fs::read(&mut handle, &mut buf) {
                    fs::close(handle);
                    if n > 1 && buf[0] == b'#' && buf[1] == b'!' {
                        source_file(shell, &full_path);
                        return;
                    }
                } else {
                    fs::close(handle);
                }
            }
            outln!(output, "{}: command not found", cmd);
            shell.last_exit_code = 127;
        }
    }
}

// ─── Individual Commands ─────────────────────────────────────────────────

fn cmd_help(output: &mut Option<&mut String>) {
    outln!(output, "");
    outln!(output, "  {}{}CantayaOS Shell Commands{}", BOLD, WHITE, RESET);
    outln!(output, "  {}{}{}", DIM, "═".repeat(50), RESET);
    outln!(output, "");
    outln!(output, "  {}Navigation:{}", YELLOW, RESET);
    outln!(output, "    cd <dir>          Change directory");
    outln!(output, "    pwd               Print working directory");
    outln!(output, "    ls [-l] [path]    List directory contents");
    outln!(output, "    tree [path]       Directory tree view");
    outln!(output, "    find <path> <pat> Find files by name");
    outln!(output, "");
    outln!(output, "  {}File Operations:{}", YELLOW, RESET);
    outln!(output, "    cat <file>        Display file contents");
    outln!(output, "    head [-n N] file  First N lines (default 10)");
    outln!(output, "    tail [-n N] file  Last N lines (default 10)");
    outln!(output, "    hexdump <file>    Hex + ASCII dump");
    outln!(output, "    wc <file>         Line/word/byte count");
    outln!(output, "    grep <pat> [file] Search for pattern");
    outln!(output, "    sort [file]       Sort lines alphabetically");
    outln!(output, "    tee <file>        Read stdin, write to stdout+file");
    outln!(output, "    touch <file>      Create empty file");
    outln!(output, "    write <f> <text>  Write text to file");
    outln!(output, "    cp <src> <dst>    Copy file");
    outln!(output, "    mv <src> <dst>    Move/rename file");
    outln!(output, "    rm <file>         Remove file");
    outln!(output, "    rmdir <dir>       Remove empty directory");
    outln!(output, "    mkdir <dir>       Create directory");
    outln!(output, "    ln <src> <dst>    Link (copy) file");
    outln!(output, "    stat <path>       File information");
    outln!(output, "");
    outln!(output, "  {}Text Processing:{}", YELLOW, RESET);
    outln!(output, "    echo <text>       Print text");
    outln!(output, "    seq <start> [end] Number sequence");
    outln!(output, "    cal               Show calendar");
    outln!(output, "");
    outln!(output, "  {}System:{}", YELLOW, RESET);
    outln!(output, "    ps                Process list");
    outln!(output, "    kill <pid>        Terminate process");
    outln!(output, "    mem / free        Memory information");
    outln!(output, "    df                Filesystem disk usage");
    outln!(output, "    uptime            System uptime");
    outln!(output, "    uname [-a]        System information");
    outln!(output, "    hostname [name]   Get/set hostname");
    outln!(output, "    whoami            Current user");
    outln!(output, "    date              Current date and time");
    outln!(output, "    dmesg [-n N]      Kernel log messages");
    outln!(output, "    lspci             PCI devices");
    outln!(output, "    neofetch          System info display");
    outln!(output, "");
    outln!(output, "  {}Shell:{}", YELLOW, RESET);
    outln!(output, "    alias [n=v]       Show/set aliases");
    outln!(output, "    unalias <name>    Remove alias");
    outln!(output, "    env               Environment variables");
    outln!(output, "    set K=V           Set variable");
    outln!(output, "    unset <var>       Unset variable");
    outln!(output, "    source <file>     Run script file");
    outln!(output, "    which <cmd>       Check if command exists");
    outln!(output, "    test <expr>       Evaluate condition");
    outln!(output, "    history           Command history");
    outln!(output, "    sleep <seconds>   Wait N seconds");
    outln!(output, "");
    outln!(output, "  {}Piping & Redirection:{}", YELLOW, RESET);
    outln!(output, "    cmd1 | cmd2       Pipe output");
    outln!(output, "    cmd > file        Redirect (overwrite)");
    outln!(output, "    cmd >> file       Redirect (append)");
    outln!(output, "    cmd1 ; cmd2       Sequential execution");
    outln!(output, "");
    outln!(output, "  {}Control:{}", YELLOW, RESET);
    outln!(output, "    clear             Clear screen");
    outln!(output, "    halt / shutdown   Halt system");
    outln!(output, "    reboot            Reboot system");
    outln!(output, "");
    outln!(output, "  {}Network:{}", YELLOW, RESET);
    outln!(output, "    ping <host>       Ping a host (simulated)");
    outln!(output, "    ifconfig          Network interface config");
    outln!(output, "    netstat [-a]      Network connections");
    outln!(output, "    arp               ARP table");
    outln!(output, "    route             Routing table");
    outln!(output, "    nslookup <host>   DNS lookup");
    outln!(output, "");
    outln!(output, "  {}Services:{}", YELLOW, RESET);
    outln!(output, "    service <cmd>     Service management");
    outln!(output, "                      list|start|stop|restart|status|enable|disable");
    outln!(output, "    top               Process/service monitor");
    outln!(output, "");
    outln!(output, "  {}Storage:{}", YELLOW, RESET);
    outln!(output, "    du [path]         Disk usage");
    outln!(output, "    fdisk -l          Partition table");
    outln!(output, "    lsblk             Block device listing");
    outln!(output, "    mount             Mounted filesystems");
    outln!(output, "");
    outln!(output, "  {}Package Manager:{}", YELLOW, RESET);
    outln!(output, "    pkg install <p>   Install package");
    outln!(output, "    pkg remove <p>    Remove package");
    outln!(output, "    pkg list          List installed packages");
    outln!(output, "    pkg search <q>    Search available packages");
    outln!(output, "    pkg info <p>      Package information");
    outln!(output, "");
    outln!(output, "  {}Users & Logging:{}", YELLOW, RESET);
    outln!(output, "    id                User/group IDs");
    outln!(output, "    groups            Group memberships");
    outln!(output, "    passwd            Change password");
    outln!(output, "    su [user]         Switch user");
    outln!(output, "    login             Login prompt");
    outln!(output, "    chmod <mode> <f>  Change permissions");
    outln!(output, "    chown <o> <file>  Change ownership");
    outln!(output, "    logger <msg>      Log to syslog");
    outln!(output, "    syslog [-n N]     View system log");
    outln!(output, "    man <cmd>         Manual page");
    outln!(output, "    edit <file>       Line editor");
    outln!(output, "");
    outln!(output, "  {}Text Processing:{}", YELLOW, RESET);
    outln!(output, "    echo <text>       Print text");
    outln!(output, "    seq <start> [end] Number sequence");
    outln!(output, "    awk <prog> [file] Field processing");
    outln!(output, "    cut -f N [-d C]   Extract fields");
    outln!(output, "    tr <from> <to>    Translate characters");
    outln!(output, "    rev               Reverse lines");
    outln!(output, "    basename <path>   Strip directory");
    outln!(output, "    dirname <path>    Strip filename");
    outln!(output, "    xargs <cmd>       Build args from stdin");
    outln!(output, "    yes [string]      Repeat string");
    outln!(output, "    cal               Show calendar");
    outln!(output, "");
    outln!(output, "  {}Control Flow:{}", YELLOW, RESET);
    outln!(output, "    if <cmd>; then ...; [elif ...; then ...;] [else ...;] fi");
    outln!(output, "    for <var> in <list>; do ...; done");
    outln!(output, "    while <cmd>; do ...; done");
    outln!(output, "");
    outln!(output, "  {}History:{}", YELLOW, RESET);
    outln!(output, "    !!                Repeat last command");
    outln!(output, "    !N                Repeat command N from history");
    outln!(output, "    !prefix           Repeat last command starting with prefix");
    outln!(output, "");
    outln!(output, "  {}Shortcuts:{} Ctrl-C cancel, Ctrl-L clear, Ctrl-W del word", YELLOW, RESET);
    outln!(output, "             Ctrl-A home, Ctrl-E end, Ctrl-K del to end");
    outln!(output, "             Tab completion, Up/Down history");
    outln!(output, "");
}

fn cmd_clear(output: &mut Option<&mut String>) {
    if output.is_none() {
        kprint!("\x1b[2J\x1b[H");
    }
}

fn cmd_uname(args: &[&str], output: &mut Option<&mut String>) {
    if args.contains(&"-a") {
        outln!(output, "{} cantaya {} {} ARM Cortex-A72 {}",
            crate::KERNEL_NAME, crate::KERNEL_VERSION, crate::KERNEL_ARCH,
            crate::KERNEL_NAME);
    } else if args.contains(&"-r") {
        outln!(output, "{}", crate::KERNEL_VERSION);
    } else if args.contains(&"-m") {
        outln!(output, "{}", crate::KERNEL_ARCH);
    } else {
        outln!(output, "{}", crate::KERNEL_NAME);
    }
}

fn cmd_ps(output: &mut Option<&mut String>) {
    outln!(output, "");
    outln!(output, "  {}PID    STATE      PRI  THR  NAME{}", BOLD, RESET);
    outln!(output, "  {}{}{}", DIM, "─".repeat(44), RESET);
    let procs = process::list_processes();
    for p in &procs {
        let state_color = match p.state {
            process::ProcessState::Running => GREEN,
            process::ProcessState::Ready => YELLOW,
            process::ProcessState::Waiting => BLUE,
            process::ProcessState::Terminated => RED,
            _ => WHITE,
        };
        outln!(output, "  {:<6} {}{:<10}{} {:<4} {:<4} {}",
            p.pid, state_color, p.state.as_str(), RESET,
            p.priority, p.threads, p.name);
    }
    outln!(output, "");
    outln!(output, "  {} processes, {} threads",
        process::process_count(), process::thread_count());
    outln!(output, "");
}

fn cmd_mem(output: &mut Option<&mut String>) {
    let free = mm::physical::free_memory() as u64;
    let total = mm::physical::total_memory() as u64;
    let used = if total > free { total - free } else { 0 };
    let heap_size = 4u64 * 1024 * 1024;
    outln!(output, "");
    outln!(output, "  {}{}Memory Information{}", BOLD, WHITE, RESET);
    outln!(output, "  {}{}{}", DIM, "─".repeat(40), RESET);
    outln!(output, "  Total:     {:>8} KB  ({} MB)", total / 1024, total / 1024 / 1024);
    outln!(output, "  Used:      {:>8} KB  ({} MB)", used / 1024, used / 1024 / 1024);
    outln!(output, "  Free:      {:>8} KB  ({} MB)", free / 1024, free / 1024 / 1024);
    outln!(output, "  Heap:      {:>8} KB  ({} MB)", heap_size / 1024, heap_size / 1024 / 1024);
    outln!(output, "  Page Size: {:>8} KB", 4);
    outln!(output, "");
}

fn cmd_uptime(output: &mut Option<&mut String>) {
    let ms = hal::timer::uptime_ms();
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    outln!(output, " up {}:{:02}:{:02}, 1 user, load average: 0.00, 0.00, 0.00",
        hours, mins, secs);
}

fn cmd_ls(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    let mut long = false;
    let mut path_arg: Option<&str> = None;
    for &a in args {
        if a == "-l" || a == "-la" || a == "-al" {
            long = true;
        } else if !a.starts_with('-') {
            path_arg = Some(a);
        }
    }
    let dir = match path_arg {
        Some(p) => resolve_path(&shell.cwd, p),
        None => shell.cwd.clone(),
    };
    match fs::readdir(&dir) {
        Ok(entries) => {
            outln!(output, "");
            outln!(output, "  {}Directory: {}{}", DIM, dir, RESET);
            outln!(output, "  {}{}{}", DIM, "─".repeat(38), RESET);
            for e in &entries {
                let (icon, color) = match e.file_type {
                    fs::FileType::Directory => ("📁", BLUE),
                    fs::FileType::Device => ("⚙\u{fe0f}", YELLOW),
                    fs::FileType::Symlink => ("🔗", MAGENTA),
                    _ => ("  ", WHITE),
                };
                if long {
                    let type_char = match e.file_type {
                        fs::FileType::Directory => "d",
                        fs::FileType::Device => "c",
                        fs::FileType::Symlink => "l",
                        fs::FileType::Pipe => "p",
                        _ => "-",
                    };
                    let perms = match e.file_type {
                        fs::FileType::Directory => "rwxr-xr-x",
                        fs::FileType::Device => "rw-rw-rw-",
                        _ => "rw-r--r--",
                    };
                    outln!(output, "  {}{}{} root root {:>8}  {}{}{}{}",
                        type_char, perms, " 1",
                        e.size, icon, color, e.name, RESET);
                } else {
                    outln!(output, "  {} {}{:<24}{} {:>8} B",
                        icon, color, e.name, RESET, e.size);
                }
            }
            outln!(output, "    {} entries", entries.len());
            outln!(output, "");
        }
        Err(_) => {
            outln!(output, "ls: cannot access '{}': No such directory", dir);
            // shell.last_exit_code = 1; // can't mutate through args
        }
    }
}

fn cmd_cat(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    if args.is_empty() {
        // Cat from stdin (pipe mode)
        if let Some(data) = stdin {
            out!(output, "{}", data);
        } else {
            outln!(output, "cat: missing file operand");
        }
        return;
    }
    for &filename in args {
        let full = resolve_path(&shell.cwd, filename);
        match read_file_string(&full) {
            Some(text) => {
                out!(output, "{}", text);
                // Ensure trailing newline
                if !text.is_empty() && !text.ends_with('\n') {
                    outln!(output);
                }
            }
            None => outln!(output, "cat: {}: No such file", filename),
        }
    }
}

fn cmd_mkdir(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "mkdir: missing operand");
        return;
    }
    for &dir in args {
        let full = resolve_path(&shell.cwd, dir);
        match fs::mkdir(&full) {
            Ok(()) => {}
            Err(_) => outln!(output, "mkdir: cannot create '{}': Already exists or invalid", dir),
        }
    }
}

fn cmd_touch(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "touch: missing operand");
        return;
    }
    for &filename in args {
        let full = resolve_path(&shell.cwd, filename);
        if fs::open(&full, fs::AccessMode::Read).is_err() {
            match fs::create(&full) {
                Ok(handle) => fs::close(handle),
                Err(_) => outln!(output, "touch: cannot create '{}'", filename),
            }
        }
    }
}

fn cmd_write(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "write: usage: write <file> <text...>");
        return;
    }
    let full = resolve_path(&shell.cwd, args[0]);
    let text: String = args[1..].join(" ");
    let data = format!("{}\n", text);
    // Remove if exists
    let _ = fs::vfs::unlink(&full);
    match fs::create(&full) {
        Ok(mut handle) => {
            let _ = fs::write(&mut handle, data.as_bytes());
            fs::close(handle);
            outln!(output, "Wrote {} bytes to {}", data.len(), args[0]);
        }
        Err(_) => outln!(output, "write: cannot create '{}'", args[0]),
    }
}

fn cmd_rm(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "rm: missing operand");
        return;
    }
    for &filename in args {
        if filename.starts_with('-') { continue; }
        let full = resolve_path(&shell.cwd, filename);
        match fs::vfs::unlink(&full) {
            Ok(()) => {}
            Err(_) => outln!(output, "rm: cannot remove '{}': No such file", filename),
        }
    }
}

fn cmd_rmdir(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "rmdir: missing operand");
        return;
    }
    for &dir in args {
        let full = resolve_path(&shell.cwd, dir);
        // Check if directory is empty
        match fs::readdir(&full) {
            Ok(entries) => {
                if entries.is_empty() {
                    match fs::vfs::unlink(&full) {
                        Ok(()) => {}
                        Err(_) => outln!(output, "rmdir: failed to remove '{}'", dir),
                    }
                } else {
                    outln!(output, "rmdir: '{}': Directory not empty", dir);
                }
            }
            Err(_) => outln!(output, "rmdir: '{}': No such directory", dir),
        }
    }
}

fn cmd_echo(args: &[&str], output: &mut Option<&mut String>) {
    let text = args.join(" ");
    outln!(output, "{}", text);
}

fn cmd_sysinfo(output: &mut Option<&mut String>) {
    let free = mm::physical::free_memory() as u64;
    let total = mm::physical::total_memory() as u64;
    let used = if total > free { total - free } else { 0 };
    outln!(output, "");
    outln!(output, "  {}{}CantayaOS System Information{}", BOLD, WHITE, RESET);
    outln!(output, "  {}{}{}", DIM, "─".repeat(40), RESET);
    outln!(output, "  OS:          {} v{}", crate::KERNEL_NAME, crate::KERNEL_VERSION);
    outln!(output, "  Arch:        {} (ARMv8-A)", crate::KERNEL_ARCH);
    outln!(output, "  CPU:         ARM Cortex-A72");
    outln!(output, "  Total RAM:   {} MB", total / 1024 / 1024);
    outln!(output, "  Used RAM:    {} MB", used / 1024 / 1024);
    outln!(output, "  Free RAM:    {} MB", free / 1024 / 1024);
    outln!(output, "  Processes:   {}", process::process_count());
    outln!(output, "  Threads:     {}", process::thread_count());
    outln!(output, "  Uptime:      {} ms", hal::timer::uptime_ms());
    outln!(output, "  Scheduler:   32-level preemptive");
    outln!(output, "");
}

fn cmd_lspci(output: &mut Option<&mut String>) {
    outln!(output, "");
    outln!(output, "  {}{}PCI Devices{}", BOLD, WHITE, RESET);
    outln!(output, "  {}{}{}", DIM, "─".repeat(40), RESET);
    outln!(output, "  00:00.0 Host bridge: CantayaOS Virtual PCI");
    outln!(output, "  00:01.0 VGA:         CantayaOS Virtual GPU");
    outln!(output, "  00:02.0 Network:     CantayaOS Virtual NIC");
    outln!(output, "  00:03.0 Storage:     CantayaOS Virtual SCSI");
    outln!(output, "");
}

fn cmd_version(output: &mut Option<&mut String>) {
    outln!(output, "{} v{} ({}) — Hybrid Kernel for ARM64",
        crate::KERNEL_NAME, crate::KERNEL_VERSION, crate::KERNEL_ARCH);
}

fn cmd_date(output: &mut Option<&mut String>) {
    let dt = hal::rtc::now();
    outln!(output, "{} {} {:>2} {:02}:{:02}:{:02} UTC {}",
        dt.weekday_str(), dt.month_str(), dt.day,
        dt.hour, dt.minute, dt.second, dt.year);
}

fn cmd_history(shell: &Shell, output: &mut Option<&mut String>) {
    outln!(output, "");
    for (i, entry) in shell.history.iter().enumerate() {
        outln!(output, "  {:>4}  {}", i + 1, entry);
    }
    outln!(output, "");
}

fn cmd_cd(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    let target = if args.is_empty() {
        shell.env.get("HOME").cloned().unwrap_or_else(|| String::from("/"))
    } else {
        String::from(args[0])
    };
    let new_path = resolve_path(&shell.cwd, &target);
    // Verify it's a directory
    match fs::readdir(&new_path) {
        Ok(_) => {
            shell.cwd = new_path.clone();
            shell.env.insert(String::from("PWD"), new_path);
        }
        Err(_) => {
            outln!(output, "cd: {}: No such directory", target);
            shell.last_exit_code = 1;
        }
    }
}

fn cmd_pwd(shell: &Shell, output: &mut Option<&mut String>) {
    outln!(output, "{}", shell.cwd);
}

fn cmd_tree(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    let path = if args.is_empty() {
        shell.cwd.clone()
    } else {
        resolve_path(&shell.cwd, args[0])
    };
    outln!(output, "{}", path);
    let mut dirs = 0u32;
    let mut files = 0u32;
    tree_recursive(&path, "", true, &mut dirs, &mut files, output, 0);
    outln!(output, "");
    outln!(output, "  {} directories, {} files", dirs, files);
    outln!(output, "");
}

fn tree_recursive(
    path: &str,
    prefix: &str,
    _is_root: bool,
    dirs: &mut u32,
    files: &mut u32,
    output: &mut Option<&mut String>,
    depth: u32,
) {
    if depth > 8 { return; }
    if let Ok(entries) = fs::readdir(path) {
        let len = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            let is_last = i == len - 1;
            let connector = if is_last { "└── " } else { "├── " };
            let child_prefix = if is_last {
                format!("{}    ", prefix)
            } else {
                format!("{}│   ", prefix)
            };
            if entry.file_type == fs::FileType::Directory {
                outln!(output, "{}{}{}{}{}", prefix, connector, BLUE, entry.name, RESET);
                *dirs += 1;
                let child_path = if path == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", path, entry.name)
                };
                tree_recursive(&child_path, &child_prefix, false, dirs, files, output, depth + 1);
            } else {
                outln!(output, "{}{}{}", prefix, connector, entry.name);
                *files += 1;
            }
        }
    }
}

fn cmd_stat(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "stat: missing operand");
        return;
    }
    let full = resolve_path(&shell.cwd, args[0]);
    match fs::vfs::stat(&full) {
        Ok(info) => {
            outln!(output, "");
            outln!(output, "  File: {}", args[0]);
            let ftype = match info.file_type {
                fs::FileType::Regular => "regular file",
                fs::FileType::Directory => "directory",
                fs::FileType::Device => "character device",
                fs::FileType::Pipe => "named pipe",
                fs::FileType::Symlink => "symbolic link",
            };
            outln!(output, "  Type: {}", ftype);
            outln!(output, "  Size: {} bytes", info.size);
            outln!(output, "  Created:  {} ms", info.created);
            outln!(output, "  Modified: {} ms", info.modified);
            outln!(output, "");
        }
        Err(_) => outln!(output, "stat: '{}': No such file or directory", args[0]),
    }
}

fn cmd_hexdump(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    let data: Vec<u8> = if !args.is_empty() {
        let full = resolve_path(&shell.cwd, args[0]);
        match read_file_bytes(&full) {
            Some(bytes) => bytes,
            None => {
                outln!(output, "hexdump: '{}': No such file", args[0]);
                return;
            }
        }
    } else if let Some(input) = stdin {
        input.as_bytes().to_vec()
    } else {
        outln!(output, "hexdump: missing file operand");
        return;
    };

    outln!(output, "");
    for (i, chunk) in data.chunks(16).enumerate() {
        let offset = i * 16;
        out!(output, "  {:08x}  ", offset);
        for (j, byte) in chunk.iter().enumerate() {
            out!(output, "{:02x} ", byte);
            if j == 7 { out!(output, " "); }
        }
        // Pad if less than 16 bytes
        for j in chunk.len()..16 {
            out!(output, "   ");
            if j == 7 { out!(output, " "); }
        }
        out!(output, " |");
        for byte in chunk {
            let ch = if *byte >= 0x20 && *byte < 0x7F { *byte as char } else { '.' };
            out!(output, "{}", ch);
        }
        outln!(output, "|");
    }
    outln!(output, "  {:08x}", data.len());
    outln!(output, "");
}

fn cmd_hostname(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        // Read from /etc/hostname
        if let Ok(mut handle) = fs::open("/etc/hostname", fs::AccessMode::Read) {
            let mut buf = [0u8; 256];
            if let Ok(n) = fs::read(&mut handle, &mut buf) {
                fs::close(handle);
                if let Ok(name) = core::str::from_utf8(&buf[..n]) {
                    outln!(output, "{}", name.trim());
                    return;
                }
            } else {
                fs::close(handle);
            }
        }
        outln!(output, "cantaya");
    } else {
        // Set hostname
        let _ = fs::vfs::unlink("/etc/hostname");
        if let Ok(mut handle) = fs::create("/etc/hostname") {
            let data = format!("{}\n", args[0]);
            let _ = fs::write(&mut handle, data.as_bytes());
            fs::close(handle);
        }
    }
}

fn cmd_cp(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "cp: usage: cp <source> <dest>");
        return;
    }
    let src = resolve_path(&shell.cwd, args[0]);
    let dst = resolve_path(&shell.cwd, args[1]);
    match read_file_bytes(&src) {
        Some(data) => {
            let n = data.len();
            let _ = fs::vfs::unlink(&dst);
            match fs::create(&dst) {
                Ok(mut dh) => {
                    let _ = fs::write(&mut dh, &data);
                    fs::close(dh);
                    outln!(output, "Copied {} bytes: {} -> {}", n, args[0], args[1]);
                }
                Err(_) => outln!(output, "cp: cannot create '{}'", args[1]),
            }
        }
        None => outln!(output, "cp: '{}': No such file", args[0]),
    }
}

fn cmd_mv(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "mv: usage: mv <source> <dest>");
        return;
    }
    let src = resolve_path(&shell.cwd, args[0]);
    let dst = resolve_path(&shell.cwd, args[1]);
    match read_file_bytes(&src) {
        Some(data) => {
            let _ = fs::vfs::unlink(&dst);
            match fs::create(&dst) {
                Ok(mut dh) => {
                    let _ = fs::write(&mut dh, &data);
                    fs::close(dh);
                    let _ = fs::vfs::unlink(&src);
                    outln!(output, "Moved: {} -> {}", args[0], args[1]);
                }
                Err(_) => outln!(output, "mv: cannot create '{}'", args[1]),
            }
        }
        None => outln!(output, "mv: '{}': No such file", args[0]),
    }
}

fn cmd_dmesg(args: &[&str], output: &mut Option<&mut String>) {
    if output.is_some() {
        // Capture dmesg into output buffer — read from /proc/version as fallback
        outln!(output, "[dmesg: {} bytes in kernel log]", hal::klog::log_size());
        return;
    }
    let mut n_lines: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n_lines = args[i + 1].parse().ok();
            i += 2;
        } else {
            i += 1;
        }
    }
    match n_lines {
        Some(n) => hal::klog::dump_tail(n),
        None => hal::klog::dump_all(),
    }
}

fn cmd_env(shell: &Shell, output: &mut Option<&mut String>) {
    outln!(output, "");
    for (k, v) in &shell.env {
        outln!(output, "  {}={}", k, v);
    }
    outln!(output, "");
}

fn cmd_set(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        cmd_env(shell, output);
        return;
    }
    let combined = args.join(" ");
    if let Some(eq_pos) = combined.find('=') {
        let key = String::from(combined[..eq_pos].trim());
        let val = String::from(combined[eq_pos + 1..].trim());
        outln!(output, "{}={}", key, val);
        shell.env.insert(key, val);
    } else {
        outln!(output, "set: usage: set NAME=VALUE");
    }
}

fn cmd_unset(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "unset: missing variable name");
        return;
    }
    for &name in args {
        shell.env.remove(name);
    }
}

fn cmd_neofetch(output: &mut Option<&mut String>) {
    let free = mm::physical::free_memory() as u64;
    let total = mm::physical::total_memory() as u64;
    let used_mb = if total > free { (total - free) / 1024 / 1024 } else { 0 };
    let total_mb = total / 1024 / 1024;
    let up = hal::timer::uptime_ms() / 1000;

    outln!(output, "");
    outln!(output, "      {}▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄{}    {}{}root@cantaya{}", CYAN, RESET, BOLD, WHITE, RESET);
    outln!(output, "     {}████████████████████{}  {}{}", CYAN, RESET, DIM, "─".repeat(17));
    outln!(output, "    {}███╔══════════╗████{}  {}OS:{}      CantayaOS v{}", CYAN, RESET, BOLD, RESET, crate::KERNEL_VERSION);
    outln!(output, "    {}███║ CANTAYA  ║████{}  {}Kernel:{}  AArch64 (hybrid)", CYAN, RESET, BOLD, RESET);
    outln!(output, "    {}███║    OS    ║████{}  {}CPU:{}     ARM Cortex-A72", CYAN, RESET, BOLD, RESET);
    outln!(output, "    {}███╚══════════╝████{}  {}Memory:{}  {} MB / {} MB", CYAN, RESET, BOLD, RESET, used_mb, total_mb);
    outln!(output, "     {}████████████████████{}  {}Uptime:{}  {}s", CYAN, RESET, BOLD, RESET, up);
    outln!(output, "      {}▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀{}    {}Shell:{}   cantaya shell", CYAN, RESET, BOLD, RESET);
    outln!(output, "                            {}Procs:{}   {}", BOLD, RESET, process::process_count());
    outln!(output, "                            {}Sched:{}   32-level preemptive", BOLD, RESET);
    outln!(output, "                            {}IPC:{}     mutex, pipe, event", BOLD, RESET);
    outln!(output, "                            {}Net:{}     eth0 (10.0.2.15/24)", BOLD, RESET);
    outln!(output, "                            {}FS:{}      ramfs, devfs, procfs", BOLD, RESET);
    outln!(output, "                            {}Svcs:{}    {} running", BOLD, RESET, hal::services::running_count());
    let installed = "0";
    outln!(output, "                            {}Pkgs:{}    {} (pkg)", BOLD, RESET, installed);
    outln!(output, "");
    // Color palette
    out!(output, "                            ");
    for color in &["\x1b[40m", "\x1b[41m", "\x1b[42m", "\x1b[43m",
                   "\x1b[44m", "\x1b[45m", "\x1b[46m", "\x1b[47m"] {
        out!(output, "{}   ", color);
    }
    outln!(output, "{}", RESET);
    out!(output, "                            ");
    for color in &["\x1b[100m", "\x1b[101m", "\x1b[102m", "\x1b[103m",
                   "\x1b[104m", "\x1b[105m", "\x1b[106m", "\x1b[107m"] {
        out!(output, "{}   ", color);
    }
    outln!(output, "{}", RESET);
    outln!(output, "");
}

fn cmd_kill(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "kill: usage: kill <pid>");
        return;
    }
    if let Ok(pid) = args[0].parse::<u32>() {
        if pid == 0 {
            outln!(output, "kill: cannot kill System process");
        } else {
            process::terminate_process(pid, -9);
            outln!(output, "Terminated process {}", pid);
        }
    } else {
        outln!(output, "kill: invalid PID");
    }
}

fn cmd_wc(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    let content: String = if !args.is_empty() {
        let full = resolve_path(&shell.cwd, args[0]);
        match read_file_string(&full) {
            Some(text) => text,
            None => {
                outln!(output, "wc: {}: No such file", args[0]);
                return;
            }
        }
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "wc: missing file operand");
        return;
    };

    let lines = content.lines().count();
    let words = content.split_whitespace().count();
    let bytes = content.len();
    let name = if !args.is_empty() { args[0] } else { "" };
    outln!(output, "{:>8}{:>8}{:>8} {}", lines, words, bytes, name);
}

fn cmd_head(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    let mut n_lines = 10usize;
    let mut file_arg: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n_lines = args[i + 1].parse().unwrap_or(10);
            i += 2;
        } else if !args[i].starts_with('-') {
            file_arg = Some(args[i]);
            i += 1;
        } else {
            // Try parsing as number after -
            if let Ok(n) = args[i][1..].parse::<usize>() {
                n_lines = n;
            }
            i += 1;
        }
    }

    let content: String = if let Some(filename) = file_arg {
        let full = resolve_path(&shell.cwd, filename);
        match read_file_string(&full) {
            Some(text) => text,
            None => {
                outln!(output, "head: {}: No such file", filename);
                return;
            }
        }
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "head: missing file operand");
        return;
    };

    for line in content.lines().take(n_lines) {
        outln!(output, "{}", line);
    }
}

fn cmd_tail(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    let mut n_lines = 10usize;
    let mut file_arg: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n_lines = args[i + 1].parse().unwrap_or(10);
            i += 2;
        } else if !args[i].starts_with('-') {
            file_arg = Some(args[i]);
            i += 1;
        } else {
            if args[i].len() > 1 {
                if let Ok(n) = args[i][1..].parse::<usize>() {
                    n_lines = n;
                }
            }
            i += 1;
        }
    }

    let content: String = if let Some(filename) = file_arg {
        let full = resolve_path(&shell.cwd, filename);
        match read_file_string(&full) {
            Some(text) => text,
            None => {
                outln!(output, "tail: {}: No such file", filename);
                return;
            }
        }
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "tail: missing file operand");
        return;
    };

    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > n_lines { lines.len() - n_lines } else { 0 };
    for line in &lines[start..] {
        outln!(output, "{}", line);
    }
}

fn cmd_grep(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "grep: usage: grep <pattern> [file...]");
        return;
    }
    let pattern = args[0];
    let mut case_insensitive = false;
    let mut show_line_numbers = false;
    let mut invert = false;
    let mut count_only = false;
    let mut file_args: Vec<&str> = Vec::new();

    for &arg in &args[1..] {
        if arg == "-i" {
            case_insensitive = true;
        } else if arg == "-n" {
            show_line_numbers = true;
        } else if arg == "-v" {
            invert = true;
        } else if arg == "-c" {
            count_only = true;
        } else if !arg.starts_with('-') {
            file_args.push(arg);
        }
    }

    let content: String = if !file_args.is_empty() {
        let mut all_content = String::new();
        for &filename in &file_args {
            let full = resolve_path(&shell.cwd, filename);
            match read_file_string(&full) {
                Some(text) => all_content.push_str(&text),
                None => {
                    outln!(output, "grep: {}: No such file", filename);
                }
            }
        }
        all_content
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "grep: no input");
        return;
    };

    let pat_lower = if case_insensitive {
        pattern.to_ascii_lowercase()
    } else {
        String::from(pattern)
    };

    let mut match_count = 0;
    for (line_num, line) in content.lines().enumerate() {
        let search_line = if case_insensitive {
            line.to_ascii_lowercase()
        } else {
            String::from(line)
        };
        let matches = search_line.contains(pat_lower.as_str());
        let show = if invert { !matches } else { matches };
        if show {
            match_count += 1;
            if !count_only {
                if show_line_numbers {
                    // Highlight matched text in red
                    outln!(output, "{}{}:{} {}", GREEN, line_num + 1, RESET, line);
                } else {
                    outln!(output, "{}", line);
                }
            }
        }
    }
    if count_only {
        outln!(output, "{}", match_count);
    }
}

fn cmd_find(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "find: usage: find <path> [-name <pattern>]");
        return;
    }
    let search_path = resolve_path(&shell.cwd, args[0]);
    let mut name_pattern: Option<&str> = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "-name" && i + 1 < args.len() {
            name_pattern = Some(args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }
    find_recursive(&search_path, name_pattern, output);
}

fn find_recursive(path: &str, pattern: Option<&str>, output: &mut Option<&mut String>) {
    if let Ok(entries) = fs::readdir(path) {
        for entry in &entries {
            let child_path = if path == "/" {
                format!("/{}", entry.name)
            } else {
                format!("{}/{}", path, entry.name)
            };
            let matches = match pattern {
                Some(pat) => {
                    // Simple glob matching: *pattern, pattern*, *pattern*
                    if pat.starts_with('*') && pat.ends_with('*') && pat.len() > 2 {
                        entry.name.contains(&pat[1..pat.len()-1])
                    } else if pat.starts_with('*') {
                        entry.name.ends_with(&pat[1..])
                    } else if pat.ends_with('*') {
                        entry.name.starts_with(&pat[..pat.len()-1])
                    } else {
                        entry.name == pat
                    }
                }
                None => true,
            };
            if matches {
                outln!(output, "{}", child_path);
            }
            if entry.file_type == fs::FileType::Directory {
                find_recursive(&child_path, pattern, output);
            }
        }
    }
}

fn cmd_sort(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    let content: String = if !args.is_empty() && !args[0].starts_with('-') {
        let full = resolve_path(&shell.cwd, args[0]);
        match read_file_string(&full) {
            Some(text) => text,
            None => {
                outln!(output, "sort: {}: No such file", args[0]);
                return;
            }
        }
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "sort: no input");
        return;
    };

    let reverse = args.contains(&"-r");
    let mut lines: Vec<&str> = content.lines().collect();
    // Simple insertion sort (works for small inputs)
    for i in 1..lines.len() {
        let mut j = i;
        while j > 0 {
            let cmp = lines[j - 1].cmp(lines[j]);
            let should_swap = if reverse {
                cmp == core::cmp::Ordering::Less
            } else {
                cmp == core::cmp::Ordering::Greater
            };
            if should_swap {
                lines.swap(j - 1, j);
                j -= 1;
            } else {
                break;
            }
        }
    }
    for line in &lines {
        outln!(output, "{}", line);
    }
}

fn cmd_df(output: &mut Option<&mut String>) {
    let free = mm::physical::free_memory() as u64;
    let total = mm::physical::total_memory() as u64;
    let used = if total > free { total - free } else { 0 };
    let pct = if total > 0 { used * 100 / total } else { 0 };

    outln!(output, "");
    outln!(output, "  {}Filesystem     Size      Used     Avail  Use%  Mount{}", BOLD, RESET);
    outln!(output, "  {}{}{}", DIM, "─".repeat(58), RESET);
    outln!(output, "  ramfs          {} MB    {} MB    {} MB  {:>3}%  /",
        total / 1024 / 1024, used / 1024 / 1024, free / 1024 / 1024, pct);
    outln!(output, "  devfs              0         0         0    0%  /dev");
    outln!(output, "  procfs             0         0         0    0%  /proc");
    outln!(output, "");
}

fn cmd_cal(output: &mut Option<&mut String>) {
    // February 2026 calendar
    outln!(output, "");
    outln!(output, "   {}{}February 2026{}", BOLD, WHITE, RESET);
    outln!(output, "  {}Su Mo Tu We Th Fr Sa{}", DIM, RESET);
    outln!(output, "   1  2  3  4  5  6  7");
    outln!(output, "   8  {}{}9{} 10 11 12 13 14", BOLD, CYAN, RESET);
    outln!(output, "  15 16 17 18 19 20 21");
    outln!(output, "  22 23 24 25 26 27 28");
    outln!(output, "");
}

fn cmd_seq(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "seq: usage: seq [start] <end> [step]");
        return;
    }
    let (start, end, step) = match args.len() {
        1 => (1i64, args[0].parse::<i64>().unwrap_or(1), 1i64),
        2 => (args[0].parse::<i64>().unwrap_or(1), args[1].parse::<i64>().unwrap_or(1), 1i64),
        _ => (
            args[0].parse::<i64>().unwrap_or(1),
            args[1].parse::<i64>().unwrap_or(1),
            args[2].parse::<i64>().unwrap_or(1),
        ),
    };
    if step == 0 {
        outln!(output, "seq: zero step");
        return;
    }
    let mut i = start;
    let mut count = 0;
    loop {
        if step > 0 && i > end { break; }
        if step < 0 && i < end { break; }
        outln!(output, "{}", i);
        i += step;
        count += 1;
        if count > 10000 { break; } // Safety limit
    }
}

fn cmd_sleep(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "sleep: usage: sleep <seconds>");
        return;
    }
    if let Ok(secs) = args[0].parse::<u64>() {
        let target = hal::timer::uptime_ms() + secs * 1000;
        while hal::timer::uptime_ms() < target {
            core::hint::spin_loop();
        }
    }
}

fn cmd_tee(shell: &Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "tee: usage: tee <file>");
        return;
    }
    let data = stdin.unwrap_or("");
    // Write to stdout (or output buffer)
    out!(output, "{}", data);

    // Also write to file
    let full = resolve_path(&shell.cwd, args[0]);
    let append = args.contains(&"-a");

    if append {
        // Read existing content first
        let existing = read_file_bytes(&full).unwrap_or_default();
        let _ = fs::vfs::unlink(&full);
        if let Ok(mut handle) = fs::create(&full) {
            if !existing.is_empty() {
                let _ = fs::write(&mut handle, &existing);
            }
            let _ = fs::write(&mut handle, data.as_bytes());
            fs::close(handle);
        }
    } else {
        let _ = fs::vfs::unlink(&full);
        if let Ok(mut handle) = fs::create(&full) {
            let _ = fs::write(&mut handle, data.as_bytes());
            fs::close(handle);
        }
    }
}

fn cmd_alias(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        // List all aliases
        outln!(output, "");
        for (name, val) in &shell.aliases {
            outln!(output, "  alias {}='{}'", name, val);
        }
        outln!(output, "");
        return;
    }
    let combined = args.join(" ");
    if let Some(eq_pos) = combined.find('=') {
        let name = String::from(combined[..eq_pos].trim());
        let mut val = String::from(combined[eq_pos + 1..].trim());
        // Strip surrounding quotes
        if (val.starts_with('\'') && val.ends_with('\''))
            || (val.starts_with('"') && val.ends_with('"'))
        {
            val = String::from(&val[1..val.len() - 1]);
        }
        shell.aliases.insert(name, val);
    } else {
        // Show specific alias
        if let Some(val) = shell.aliases.get(args[0]) {
            outln!(output, "alias {}='{}'", args[0], val);
        } else {
            outln!(output, "alias: '{}' not found", args[0]);
        }
    }
}

fn cmd_unalias(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "unalias: usage: unalias <name>");
        return;
    }
    if args[0] == "-a" {
        shell.aliases.clear();
    } else {
        for &name in args {
            shell.aliases.remove(name);
        }
    }
}

fn cmd_source(shell: &mut Shell, args: &[&str]) {
    if args.is_empty() {
        kprintln!("source: usage: source <file>");
        return;
    }
    source_file(shell, args[0]);
}

fn cmd_which(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "which: usage: which <command>");
        return;
    }
    for &cmd in args {
        if COMMANDS.contains(&cmd) {
            outln!(output, "{}: shell built-in", cmd);
        } else {
            outln!(output, "{}: not found", cmd);
        }
    }
}

fn cmd_ln(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    // In our ramfs, "ln" just copies the file (no real hard/symlinks)
    if args.len() < 2 {
        outln!(output, "ln: usage: ln [-s] <target> <link>");
        return;
    }
    let (src, dst) = if args[0] == "-s" {
        if args.len() < 3 {
            outln!(output, "ln: usage: ln -s <target> <link>");
            return;
        }
        (args[1], args[2])
    } else {
        (args[0], args[1])
    };
    let src_path = resolve_path(&shell.cwd, src);
    let dst_path = resolve_path(&shell.cwd, dst);
    match read_file_bytes(&src_path) {
        Some(data) => {
            match fs::create(&dst_path) {
                Ok(mut dh) => {
                    let _ = fs::write(&mut dh, &data);
                    fs::close(dh);
                    outln!(output, "Linked: {} -> {}", dst, src);
                }
                Err(_) => outln!(output, "ln: cannot create '{}'", dst),
            }
        }
        None => outln!(output, "ln: '{}': No such file", src),
    }
}

fn cmd_test(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        shell.last_exit_code = 1;
        return;
    }
    let result = match args.get(0).copied() {
        // -f file: true if file exists and is regular
        Some("-f") => {
            if let Some(&path_arg) = args.get(1) {
                let full = resolve_path(&shell.cwd, path_arg);
                matches!(fs::vfs::stat(&full), Ok(info) if info.file_type == fs::FileType::Regular)
            } else {
                false
            }
        }
        // -d dir: true if directory exists
        Some("-d") => {
            if let Some(&path_arg) = args.get(1) {
                let full = resolve_path(&shell.cwd, path_arg);
                matches!(fs::vfs::stat(&full), Ok(info) if info.file_type == fs::FileType::Directory)
            } else {
                false
            }
        }
        // -e path: true if exists
        Some("-e") => {
            if let Some(&path_arg) = args.get(1) {
                let full = resolve_path(&shell.cwd, path_arg);
                fs::vfs::stat(&full).is_ok()
            } else {
                false
            }
        }
        // -z string: true if string is empty
        Some("-z") => args.get(1).map_or(true, |s| s.is_empty()),
        // -n string: true if string is not empty
        Some("-n") => args.get(1).map_or(false, |s| !s.is_empty()),
        // string = string, numeric comparisons
        _ => {
            if args.len() >= 3 {
                let a_str = args[0];
                let op = args[1];
                let b_str = args[2];
                match op {
                    "=" => a_str == b_str,
                    "!=" => a_str != b_str,
                    "-eq" => a_str.parse::<i64>().unwrap_or(0) == b_str.parse::<i64>().unwrap_or(0),
                    "-ne" => a_str.parse::<i64>().unwrap_or(0) != b_str.parse::<i64>().unwrap_or(0),
                    "-gt" => a_str.parse::<i64>().unwrap_or(0) > b_str.parse::<i64>().unwrap_or(0),
                    "-ge" => a_str.parse::<i64>().unwrap_or(0) >= b_str.parse::<i64>().unwrap_or(0),
                    "-lt" => (a_str.parse::<i64>().unwrap_or(0)) < b_str.parse::<i64>().unwrap_or(0),
                    "-le" => a_str.parse::<i64>().unwrap_or(0) <= b_str.parse::<i64>().unwrap_or(0),
                    _ => !args[0].is_empty(),
                }
            } else {
                // Non-empty string is true
                !args[0].is_empty()
            }
        }
    };
    shell.last_exit_code = if result { 0 } else { 1 };
    let _ = output; // suppress unused
}

// ─── Network Commands ────────────────────────────────────────────────────

fn cmd_ping(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "ping: usage: ping [-c count] <host>");
        return;
    }
    let mut count = 4usize;
    let mut host: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-c" && i + 1 < args.len() {
            count = args[i + 1].parse().unwrap_or(4);
            i += 2;
        } else if !args[i].starts_with('-') {
            host = Some(args[i]);
            i += 1;
        } else {
            i += 1;
        }
    }
    let host = match host {
        Some(h) => h,
        None => {
            outln!(output, "ping: missing host");
            return;
        }
    };

    use crate::drivers::net;
    let ip = if host == "localhost" {
        [127, 0, 0, 1]
    } else if let Some(parsed) = net::parse_ip(host) {
        parsed
    } else if crate::drivers::virtio_net::get_mac().is_some() {
        // Real DNS lookup via UDP
        outln!(output, "Resolving {}...", host);
        match crate::net::dns::resolve(host) {
            Some(ip) => ip,
            None => {
                outln!(output, "ping: {}: Name or service not known", host);
                return;
            }
        }
    } else {
        // Fallback simulated DNS for common hosts
        match host {
            "google.com" => [142, 250, 80, 46],
            "github.com" => [140, 82, 121, 3],
            "cloudflare.com" => [104, 16, 132, 229],
            _ => {
                outln!(output, "ping: {}: Name or service not known", host);
                return;
            }
        }
    };

    outln!(output, "PING {} ({}) 56(84) bytes of data.",
        host, net::format_ip(&ip));

    // Use real ICMP if virtio-net is available
    let have_real_net = crate::drivers::virtio_net::get_mac().is_some();

    let mut min = u64::MAX;
    let mut max = 0u64;
    let mut total = 0u64;
    let mut received = 0usize;
    for seq in 0..count {
        if have_real_net {
            // Real ICMP ping
            let start = hal::timer::uptime_ms();
            crate::net::icmp::send_echo_request(&ip, seq as u16, 56);
            // Wait up to 2 seconds for reply
            let mut got = false;
            while hal::timer::uptime_ms() - start < 2000 {
                if let Some((_src, rseq, _dlen)) = crate::net::icmp::poll_reply() {
                    if rseq == seq as u16 {
                        let latency = hal::timer::uptime_ms() - start;
                        if latency < min { min = latency; }
                        if latency > max { max = latency; }
                        total += latency;
                        received += 1;
                        outln!(output, "64 bytes from {} ({}): icmp_seq={} ttl=64 time={} ms",
                            host, net::format_ip(&ip), seq + 1, latency);
                        got = true;
                        break;
                    }
                }
                crate::process::scheduler::yield_thread();
            }
            if !got {
                outln!(output, "Request timeout for icmp_seq {}", seq + 1);
            }
        } else {
            // Simulated ping fallback
            let (_ok, latency) = net::ping(&ip);
            if latency < min { min = latency; }
            if latency > max { max = latency; }
            total += latency;
            received += 1;
            outln!(output, "64 bytes from {} ({}): icmp_seq={} ttl=64 time={}.{} ms",
                host, net::format_ip(&ip), seq + 1, latency / 10, latency % 10);
        }
        // Small delay between pings
        if seq + 1 < count {
            let target = hal::timer::uptime_ms() + 1000;
            while hal::timer::uptime_ms() < target {
                crate::process::scheduler::yield_thread();
            }
        }
    }
    outln!(output, "");
    outln!(output, "--- {} ping statistics ---", host);
    let loss = if count > 0 { ((count - received) * 100) / count } else { 0 };
    let avg = if received > 0 { total / received as u64 } else { 0 };
    outln!(output, "{} packets transmitted, {} received, {}% packet loss",
        count, received, loss);
    if received > 0 {
        outln!(output, "rtt min/avg/max = {}/{}/{} ms", min, avg, max);
    }
}

fn cmd_ifconfig(output: &mut Option<&mut String>) {
    use crate::drivers::net;
    let ifaces = net::get_interfaces();
    outln!(output, "");
    for iface in &ifaces {
        let up = iface.state == net::InterfaceState::Up;
        outln!(output, "{}: flags={} mtu {}",
            iface.name,
            if up { "4163<UP,BROADCAST,RUNNING,MULTICAST>" } else { "4098<BROADCAST,MULTICAST>" },
            iface.mtu);
        // compute broadcast = ip | !netmask
        let bcast = [
            iface.ip[0] | !iface.netmask[0],
            iface.ip[1] | !iface.netmask[1],
            iface.ip[2] | !iface.netmask[2],
            iface.ip[3] | !iface.netmask[3],
        ];
        outln!(output, "        inet {}  netmask {}  broadcast {}",
            net::format_ip(&iface.ip), net::format_ip(&iface.netmask),
            net::format_ip(&bcast));
        outln!(output, "        ether {}  txqueuelen 1000",
            net::format_mac(&iface.mac));
        outln!(output, "        RX packets {}  bytes {} ({} KB)",
            iface.rx_packets, iface.rx_bytes, iface.rx_bytes / 1024);
        outln!(output, "        TX packets {}  bytes {} ({} KB)",
            iface.tx_packets, iface.tx_bytes, iface.tx_bytes / 1024);
        outln!(output, "");
    }
}

fn cmd_netstat(args: &[&str], output: &mut Option<&mut String>) {
    use crate::drivers::net;
    let show_all = args.contains(&"-a") || args.contains(&"-an");
    let conns = net::get_connections();

    outln!(output, "");
    outln!(output, "  {}Active Internet connections{}{}", BOLD, if show_all { " (including servers)" } else { "" }, RESET);
    outln!(output, "  Proto  Local Address          Foreign Address        State");
    outln!(output, "  {}{}{}", DIM, "─".repeat(64), RESET);
    for c in &conns {
        if !show_all && c.state == "LISTEN" { continue; }
        outln!(output, "  {:<7}{:<23}{:<23}{}",
            c.proto,
            format!("{}:{}", net::format_ip(&c.local_ip), c.local_port),
            format!("{}:{}", net::format_ip(&c.remote_ip), c.remote_port),
            c.state);
    }
    outln!(output, "");
}

fn cmd_arp(output: &mut Option<&mut String>) {
    use crate::drivers::net;
    let entries = net::get_arp_table();
    outln!(output, "");
    outln!(output, "  {}ARP Table{}", BOLD, RESET);
    outln!(output, "  Address          HW Address         Flags  Interface");
    outln!(output, "  {}{}{}", DIM, "─".repeat(54), RESET);
    for e in &entries {
        outln!(output, "  {:<18}{:<20}{:<8}{}",
            net::format_ip(&e.ip), net::format_mac(&e.mac), "C", e.iface);
    }
    outln!(output, "  Entries: {}", entries.len());
    outln!(output, "");
}

fn cmd_route(output: &mut Option<&mut String>) {
    use crate::drivers::net;
    let routes = net::get_routes();
    outln!(output, "");
    outln!(output, "  {}Kernel IP routing table{}", BOLD, RESET);
    outln!(output, "  Destination      Gateway          Genmask          Flags  Iface");
    outln!(output, "  {}{}{}", DIM, "─".repeat(64), RESET);
    for r in &routes {
        let flags = if r.gateway == [0, 0, 0, 0] { "U" } else { "UG" };
        outln!(output, "  {:<18}{:<18}{:<18}{:<8}{}",
            net::format_ip(&r.destination),
            net::format_ip(&r.gateway),
            net::format_ip(&r.mask),
            flags, r.iface);
    }
    outln!(output, "");
}

// ─── User & Auth Commands ────────────────────────────────────────────────

fn cmd_id(output: &mut Option<&mut String>) {
    outln!(output, "uid=0(root) gid=0(root) groups=0(root),10(wheel)");
}

fn cmd_groups(output: &mut Option<&mut String>) {
    outln!(output, "root wheel");
}

fn cmd_mount(output: &mut Option<&mut String>) {
    outln!(output, "ramfs on / type ramfs (rw)");
    outln!(output, "devfs on /dev type devfs (rw)");
    outln!(output, "procfs on /proc type procfs (ro)");
    if crate::drivers::virtio_blk::is_available() {
        outln!(output, "/dev/vda on /mnt/disk type fat32 (rw)");
    }
}

// ─── Logging Commands ────────────────────────────────────────────────────

fn cmd_logger(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "logger: usage: logger [-p level] <message>");
        return;
    }
    use crate::hal::syslog;
    let mut level = syslog::LogLevel::Info;
    let mut msg_start = 0;
    if args.len() >= 2 && args[0] == "-p" {
        level = match args[1].to_ascii_lowercase().as_str() {
            "emerg" | "emergency" => syslog::LogLevel::Emergency,
            "alert" => syslog::LogLevel::Alert,
            "crit" | "critical" => syslog::LogLevel::Critical,
            "err" | "error" => syslog::LogLevel::Error,
            "warn" | "warning" => syslog::LogLevel::Warning,
            "notice" => syslog::LogLevel::Notice,
            "info" => syslog::LogLevel::Info,
            "debug" => syslog::LogLevel::Debug,
            _ => syslog::LogLevel::Info,
        };
        msg_start = 2;
    }
    let msg = args[msg_start..].join(" ");
    syslog::log(level, syslog::Facility::User, &msg);
}

fn cmd_syslog(args: &[&str], output: &mut Option<&mut String>) {
    use crate::hal::syslog;
    let mut n_lines = 20usize;
    let mut level_filter: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-n" && i + 1 < args.len() {
            n_lines = args[i + 1].parse().unwrap_or(20);
            i += 2;
        } else if args[i] == "-l" && i + 1 < args.len() {
            level_filter = Some(args[i + 1]);
            i += 2;
        } else {
            i += 1;
        }
    }

    let entries = if let Some(lf) = level_filter {
        let level = match lf.to_ascii_lowercase().as_str() {
            "emerg" => syslog::LogLevel::Emergency,
            "alert" => syslog::LogLevel::Alert,
            "crit" => syslog::LogLevel::Critical,
            "err" | "error" => syslog::LogLevel::Error,
            "warn" | "warning" => syslog::LogLevel::Warning,
            "notice" => syslog::LogLevel::Notice,
            "info" => syslog::LogLevel::Info,
            "debug" => syslog::LogLevel::Debug,
            _ => syslog::LogLevel::Info,
        };
        syslog::get_by_level(level)
    } else {
        syslog::get_tail(n_lines)
    };

    outln!(output, "");
    for entry in &entries {
        let color = match entry.level {
            syslog::LogLevel::Emergency | syslog::LogLevel::Alert | syslog::LogLevel::Critical => RED,
            syslog::LogLevel::Error => RED,
            syslog::LogLevel::Warning => YELLOW,
            syslog::LogLevel::Notice | syslog::LogLevel::Info => WHITE,
            syslog::LogLevel::Debug => DIM,
        };
        outln!(output, "  {}{}{}", color, syslog::format_entry(entry), RESET);
    }
    outln!(output, "  ({} entries total)", syslog::entry_count());
    outln!(output, "");
}

// ─── History Expansion ──────────────────────────────────────────────────

/// Expand history references: !!, !N, !prefix
/// Returns None if invalid reference, Some(expanded) otherwise
fn expand_history(shell: &Shell, line: &str) -> Option<String> {
    if !line.contains('!') || line.starts_with('#') {
        return Some(String::from(line));
    }
    let mut result = String::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'!' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'!' {
                // !! — last command
                if let Some(last) = shell.history.last() {
                    result.push_str(last);
                }
                i += 2;
            } else if bytes[i + 1].is_ascii_digit() {
                // !N — command number N
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() { i += 1; }
                if let Ok(n) = line[start..i].parse::<usize>() {
                    if n > 0 && n <= shell.history.len() {
                        result.push_str(&shell.history[n - 1]);
                    } else {
                        kprintln!("!{}: event not found", n);
                        return None;
                    }
                }
            } else if bytes[i + 1].is_ascii_alphabetic() {
                // !prefix — last command starting with prefix
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b' ' && bytes[i] != b';' { i += 1; }
                let prefix = &line[start..i];
                if let Some(found) = shell.history.iter().rev().find(|h| h.starts_with(prefix)) {
                    result.push_str(found);
                } else {
                    kprintln!("!{}: event not found", prefix);
                    return None;
                }
            } else {
                result.push(bytes[i] as char);
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Some(result)
}

// ─── Service Manager Commands ───────────────────────────────────────────

fn cmd_service(args: &[&str], output: &mut Option<&mut String>) {
    use crate::hal::services;
    if args.is_empty() {
        outln!(output, "service: usage: service <command> [name]");
        outln!(output, "  list                 List all services");
        outln!(output, "  status <name>        Show service status");
        outln!(output, "  start <name>         Start a service");
        outln!(output, "  stop <name>          Stop a service");
        outln!(output, "  restart <name>       Restart a service");
        outln!(output, "  enable <name>        Enable at boot");
        outln!(output, "  disable <name>       Disable at boot");
        return;
    }
    match args[0] {
        "list" => {
            let svcs = services::list_services();
            outln!(output, "");
            outln!(output, "  {}UNIT              STATE     ENABLED  PID    PORT   DESCRIPTION{}", BOLD, RESET);
            outln!(output, "  {}{}{}", DIM, "─".repeat(75), RESET);
            for svc in &svcs {
                let state_color = match svc.state {
                    services::ServiceState::Running => GREEN,
                    services::ServiceState::Stopped => RED,
                    services::ServiceState::Failed  => RED,
                };
                let state_str = match svc.state {
                    services::ServiceState::Running => "running",
                    services::ServiceState::Stopped => "stopped",
                    services::ServiceState::Failed  => "failed",
                };
                let enabled = if svc.enabled { format!("{}yes{}", GREEN, RESET) } else { format!("{}no{}", DIM, RESET) };
                let port_str = match svc.port {
                    Some(p) => format!("{}", p),
                    None => String::from("-"),
                };
                outln!(output, "  {:<18}{}{:<10}{}{:<9}{:<7}{:<7}{}",
                    svc.name, state_color, state_str, RESET, enabled, svc.pid, port_str, svc.description);
            }
            outln!(output, "");
            outln!(output, "  {} services loaded, {} active",
                svcs.len(), services::running_count());
            outln!(output, "");
        }
        "status" => {
            if args.len() < 2 { outln!(output, "service: usage: service status <name>"); return; }
            match services::get_service(args[1]) {
                Some(svc) => {
                    let state_dot = match svc.state {
                        services::ServiceState::Running => format!("{}●{}", GREEN, RESET),
                        services::ServiceState::Stopped => format!("{}●{}", RED, RESET),
                        services::ServiceState::Failed  => format!("{}●{}", RED, RESET),
                    };
                    let state_str = match svc.state {
                        services::ServiceState::Running => format!("{}active (running){}", GREEN, RESET),
                        services::ServiceState::Stopped => format!("{}inactive (dead){}", RED, RESET),
                        services::ServiceState::Failed  => format!("{}failed{}", RED, RESET),
                    };
                    outln!(output, "{} {}.service - {}", state_dot, svc.name, svc.description);
                    outln!(output, "     Loaded: loaded (/etc/rc.d/{}.conf; {})",
                        svc.name, if svc.enabled { "enabled" } else { "disabled" });
                    outln!(output, "     Active: {}", state_str);
                    if svc.pid > 0 {
                        outln!(output, "   Main PID: {} ({})", svc.pid, svc.name);
                    }
                    if svc.restart_count > 0 {
                        outln!(output, "   Restarts: {}", svc.restart_count);
                    }
                }
                None => outln!(output, "service: '{}' not found", args[1]),
            }
        }
        "start" => {
            if args.len() < 2 { outln!(output, "service: usage: service start <name>"); return; }
            match services::start_service(args[1]) {
                Ok(()) => outln!(output, "Starting {}... {}OK{}", args[1], GREEN, RESET),
                Err(e) => outln!(output, "service: {}: {}", args[1], e),
            }
        }
        "stop" => {
            if args.len() < 2 { outln!(output, "service: usage: service stop <name>"); return; }
            match services::stop_service(args[1]) {
                Ok(()) => outln!(output, "Stopping {}... {}OK{}", args[1], GREEN, RESET),
                Err(e) => outln!(output, "service: {}: {}", args[1], e),
            }
        }
        "restart" => {
            if args.len() < 2 { outln!(output, "service: usage: service restart <name>"); return; }
            match services::restart_service(args[1]) {
                Ok(()) => outln!(output, "Restarting {}... {}OK{}", args[1], GREEN, RESET),
                Err(e) => outln!(output, "service: {}: {}", args[1], e),
            }
        }
        "enable" => {
            if args.len() < 2 { outln!(output, "service: usage: service enable <name>"); return; }
            match services::enable_service(args[1]) {
                Ok(()) => outln!(output, "Enabled {} for boot", args[1]),
                Err(e) => outln!(output, "service: {}: {}", args[1], e),
            }
        }
        "disable" => {
            if args.len() < 2 { outln!(output, "service: usage: service disable <name>"); return; }
            match services::disable_service(args[1]) {
                Ok(()) => outln!(output, "Disabled {} for boot", args[1]),
                Err(e) => outln!(output, "service: {}: {}", args[1], e),
            }
        }
        _ => outln!(output, "service: unknown command '{}'. Try 'service list'", args[0]),
    }
}

// ─── Disk Utilities ─────────────────────────────────────────────────────

fn cmd_du(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    let target = if args.is_empty() { &shell.cwd } else { args[0] };
    let full = resolve_path(&shell.cwd, target);

    fn du_recurse(path: &str, output: &mut Option<&mut String>) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = fs::readdir(path) {
            for entry in &entries {
                let child = if path == "/" {
                    format!("/{}", entry.name)
                } else {
                    format!("{}/{}", path, entry.name)
                };
                if entry.file_type == fs::FileType::Directory {
                    let sub = du_recurse(&child, output);
                    outln!(output, "{:>8}  {}", sub / 1024, child);
                    total += sub;
                } else {
                    total += entry.size;
                }
            }
        }
        total
    }

    outln!(output, "");
    let total = du_recurse(&full, output);
    outln!(output, "{:>8}  {} (total)", total / 1024, full);
    outln!(output, "");
}

fn cmd_fdisk(args: &[&str], output: &mut Option<&mut String>) {
    if !args.contains(&"-l") {
        outln!(output, "fdisk: usage: fdisk -l");
        return;
    }
    outln!(output, "");
    outln!(output, "Disk /dev/sda: 128 MiB, 134217728 bytes, 262144 sectors");
    outln!(output, "Disk model: QEMU HARDDISK");
    outln!(output, "Units: sectors of 1 * 512 = 512 bytes");
    outln!(output, "Sector size (logical/physical): 512 bytes / 512 bytes");
    outln!(output, "Disklabel type: gpt");
    outln!(output, "Disk identifier: 8A5B1C3D-4E6F-7890-AB12-CD34EF567890");
    outln!(output, "");
    outln!(output, "{}Device      Start    End  Sectors  Size Type{}", BOLD, RESET);
    outln!(output, "/dev/sda1    2048 251903   249856  122M Linux filesystem");
    outln!(output, "");
}

fn cmd_lsblk(output: &mut Option<&mut String>) {
    outln!(output, "{}NAME   MAJ:MIN RM   SIZE RO TYPE MOUNTPOINTS{}", BOLD, RESET);
    if crate::drivers::virtio_blk::is_available() {
        let cap = crate::drivers::virtio_blk::capacity();
        let size_mb = cap * 512 / 1024 / 1024;
        outln!(output, "vda    254:0    0  {:>4}M  0 disk /mnt/disk", size_mb);
    } else {
        outln!(output, "(no block devices)");
    }
}

// ─── Process Monitor ────────────────────────────────────────────────────

fn cmd_top(output: &mut Option<&mut String>) {
    let uptime_s = hal::timer::uptime_ms() / 1000;
    let free = mm::physical::free_memory() as u64;
    let total = mm::physical::total_memory() as u64;
    let used = if total > free { total - free } else { 0 };
    let pct = if total > 0 { used * 100 / total } else { 0 };

    outln!(output, "{}top - {} up {}s,  1 user,  load average: 0.00, 0.00, 0.00{}", BOLD, "00:00:00", uptime_s, RESET);
    outln!(output, "Tasks: {} total,   1 running,   {} sleeping,   0 stopped,   0 zombie",
        process::process_count(), process::process_count().saturating_sub(1));
    outln!(output, "%Cpu(s):  0.5 us,  0.3 sy,  0.0 ni, 99.2 id,  0.0 wa,  0.0 hi,  0.0 si");
    outln!(output, "MiB Mem:  {:>6} total, {:>6} free, {:>6} used, {:>6} buff/cache",
        total / 1024 / 1024, free / 1024 / 1024, used / 1024 / 1024, 0);
    outln!(output, "MiB Swap:      0 total,      0 free,      0 used.  {:>6} avail Mem", free / 1024 / 1024);
    outln!(output, "");
    outln!(output, "  {}PID USER      PR  NI    VIRT    RES    SHR S  %CPU  %MEM     TIME+ COMMAND{}", BOLD, RESET);

    let procs = process::list_processes();
    for p in &procs {
        let state = match p.state.as_str() {
            "Running" => "R",
            "Ready" => "S",
            "Blocked" => "D",
            _ => "S",
        };
        let cpu_pct = if p.pid == 0 { "99.0" } else { " 0.0" };
        let mem_pct = if p.pid == 0 { format!("{:>4.1}", pct) } else { String::from(" 0.0") };
        outln!(output, "{:>5} {:<9} {:>2}   0 {:>7} {:>6} {:>6} {} {:>5} {:>5} {:>9} {}",
            p.pid, "root", p.priority, "4096", "1024", "512", state, cpu_pct, mem_pct, "0:00.00", p.name);
    }

    // Also show services as processes
    let svcs = hal::services::list_services();
    for svc in &svcs {
        if svc.state == hal::services::ServiceState::Running {
            outln!(output, "{:>5} {:<9} {:>2}   0 {:>7} {:>6} {:>6} S   0.0   0.1  0:00.00 {}",
                svc.pid, "root", 20, "2048", "512", "256", svc.name);
        }
    }
}

// ─── DNS Tools ──────────────────────────────────────────────────────────

fn cmd_nslookup(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "nslookup: usage: nslookup <hostname>");
        return;
    }
    let host = args[0];
    outln!(output, "Server:  10.0.2.3");
    outln!(output, "Address: 10.0.2.3#53");
    outln!(output, "");

    let ip = match host {
        "localhost" => "127.0.0.1",
        "cantaya" => "127.0.1.1",
        "google.com" | "www.google.com" => "142.250.80.46",
        "github.com" | "www.github.com" => "140.82.121.4",
        "cloudflare.com" | "www.cloudflare.com" => "104.16.132.229",
        "microsoft.com" | "www.microsoft.com" => "20.70.246.20",
        "kernel.org" | "www.kernel.org" => "139.178.84.217",
        "rust-lang.org" | "www.rust-lang.org" => "13.225.221.29",
        "dns.google" => "8.8.8.8",
        "one.one.one.one" => "1.1.1.1",
        _ => {
            // Generate deterministic IP from hostname
            outln!(output, "Non-authoritative answer:");
            let h: u32 = host.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
            outln!(output, "Name:    {}", host);
            outln!(output, "Address: {}.{}.{}.{}", 93 + (h % 20), (h >> 8) % 256, (h >> 16) % 256, 1 + (h >> 24) % 254);
            return;
        }
    };
    outln!(output, "Non-authoritative answer:");
    outln!(output, "Name:    {}", host);
    outln!(output, "Address: {}", ip);
}

// ─── Package Manager ────────────────────────────────────────────────────

fn cmd_pkg(shell: &mut Shell, args: &[&str], output: &mut Option<&mut String>) {
    // Track installed packages in env var PKG_INSTALLED
    if args.is_empty() {
        outln!(output, "pkg: usage: pkg <command> [args]");
        outln!(output, "  install <pkg>   Install a package");
        outln!(output, "  remove <pkg>    Remove a package");
        outln!(output, "  list            List installed packages");
        outln!(output, "  search <query>  Search available packages");
        outln!(output, "  info <pkg>      Package information");
        outln!(output, "  update          Update package database");
        return;
    }

    // All available packages in the "repository"
    let repo: &[(&str, &str, &str)] = &[
        ("bash", "5.2.21", "GNU Bourne Again Shell"),
        ("coreutils", "9.4", "Core system utilities"),
        ("curl", "8.5.0", "URL transfer utility"),
        ("git", "2.43.0", "Distributed version control"),
        ("gcc", "13.2.0", "GNU C Compiler"),
        ("make", "4.4.1", "Build automation tool"),
        ("nano", "7.2", "Text editor"),
        ("vim", "9.1", "Vi IMproved text editor"),
        ("python3", "3.12.1", "Python interpreter"),
        ("nodejs", "20.11.0", "JavaScript runtime"),
        ("openssh", "9.6p1", "Secure shell client/server"),
        ("nginx", "1.25.4", "HTTP and reverse proxy server"),
        ("htop", "3.3.0", "Interactive process viewer"),
        ("tmux", "3.4", "Terminal multiplexer"),
        ("zsh", "5.9", "Z Shell"),
        ("rsync", "3.2.7", "File synchronization"),
        ("wget", "1.21.4", "Network file retriever"),
        ("jq", "1.7.1", "JSON processor"),
        ("tree", "2.1.1", "Directory tree viewer"),
        ("neofetch", "7.1.0", "System information tool"),
    ];

    match args[0] {
        "install" => {
            if args.len() < 2 { outln!(output, "pkg: usage: pkg install <name>"); return; }
            let pkg_name = args[1];
            // Check if in repo
            let pkg = repo.iter().find(|(n, _, _)| *n == pkg_name);
            if pkg.is_none() {
                outln!(output, "pkg: package '{}' not found in repository", pkg_name);
                return;
            }
            let (name, ver, _desc) = pkg.unwrap();
            // Check if already installed
            let installed = shell.env.get("PKG_INSTALLED").cloned().unwrap_or_default();
            if installed.split(',').any(|p| p.trim() == *name) {
                outln!(output, "pkg: {} is already installed", name);
                return;
            }
            outln!(output, "Resolving dependencies for {}...", name);
            outln!(output, "Downloading {}-{}...", name, ver);
            outln!(output, "  [████████████████████████████████] 100%");
            outln!(output, "Installing {}-{}...", name, ver);
            outln!(output, "  Configuring...");
            outln!(output, "  {}✓{} {} {} installed successfully", GREEN, RESET, name, ver);
            // Track installed
            let new_installed = if installed.is_empty() {
                String::from(*name)
            } else {
                format!("{},{}", installed, name)
            };
            shell.env.insert(String::from("PKG_INSTALLED"), new_installed);
        }
        "remove" => {
            if args.len() < 2 { outln!(output, "pkg: usage: pkg remove <name>"); return; }
            let pkg_name = args[1];
            let installed = shell.env.get("PKG_INSTALLED").cloned().unwrap_or_default();
            let pkgs: Vec<&str> = installed.split(',').filter(|p| !p.is_empty()).collect();
            if !pkgs.iter().any(|p| *p == pkg_name) {
                outln!(output, "pkg: {} is not installed", pkg_name);
                return;
            }
            outln!(output, "Removing {}...", pkg_name);
            outln!(output, "  {}✓{} {} removed successfully", GREEN, RESET, pkg_name);
            let new_installed: Vec<&str> = pkgs.into_iter().filter(|p| *p != pkg_name).collect();
            shell.env.insert(String::from("PKG_INSTALLED"), new_installed.join(","));
        }
        "list" => {
            let installed = shell.env.get("PKG_INSTALLED").cloned().unwrap_or_default();
            let pkgs: Vec<&str> = installed.split(',').filter(|p| !p.is_empty()).collect();
            if pkgs.is_empty() {
                outln!(output, "No packages installed. Use 'pkg install <name>' to install.");
                return;
            }
            outln!(output, "");
            outln!(output, "  {}Installed Packages:{}", BOLD, RESET);
            for p in &pkgs {
                let info = repo.iter().find(|(n, _, _)| n == p);
                if let Some((name, ver, desc)) = info {
                    outln!(output, "  {}{}{} {} - {}", GREEN, name, RESET, ver, desc);
                } else {
                    outln!(output, "  {}{}{}", GREEN, p, RESET);
                }
            }
            outln!(output, "");
            outln!(output, "  {} packages installed", pkgs.len());
            outln!(output, "");
        }
        "search" => {
            if args.len() < 2 { outln!(output, "pkg: usage: pkg search <query>"); return; }
            let query = args[1];
            let installed = shell.env.get("PKG_INSTALLED").cloned().unwrap_or_default();
            outln!(output, "");
            let mut found = 0;
            for (name, ver, desc) in repo {
                if name.contains(query) || desc.to_ascii_lowercase().contains(&query.to_ascii_lowercase()) {
                    let status = if installed.split(',').any(|p| p == *name) {
                        format!("{}[installed]{}", GREEN, RESET)
                    } else {
                        String::new()
                    };
                    outln!(output, "  {}{}{} {} - {} {}", CYAN, name, RESET, ver, desc, status);
                    found += 1;
                }
            }
            if found == 0 {
                outln!(output, "  No packages matching '{}' found", query);
            }
            outln!(output, "");
        }
        "info" => {
            if args.len() < 2 { outln!(output, "pkg: usage: pkg info <name>"); return; }
            let pkg_name = args[1];
            match repo.iter().find(|(n, _, _)| *n == pkg_name) {
                Some((name, ver, desc)) => {
                    let installed = shell.env.get("PKG_INSTALLED").cloned().unwrap_or_default();
                    let is_inst = installed.split(',').any(|p| p == *name);
                    outln!(output, "");
                    outln!(output, "  {}Package:{} {}", BOLD, RESET, name);
                    outln!(output, "  {}Version:{} {}", BOLD, RESET, ver);
                    outln!(output, "  {}Description:{} {}", BOLD, RESET, desc);
                    outln!(output, "  {}Status:{}  {}", BOLD, RESET, if is_inst { format!("{}installed{}", GREEN, RESET) } else { format!("{}not installed{}", DIM, RESET) });
                    outln!(output, "  {}Repository:{} cantaya-core", BOLD, RESET);
                    outln!(output, "  {}Architecture:{} aarch64", BOLD, RESET);
                    outln!(output, "");
                }
                None => outln!(output, "pkg: package '{}' not found", pkg_name),
            }
        }
        "update" => {
            outln!(output, "Synchronizing package database...");
            outln!(output, "  cantaya-core   {} packages", repo.len());
            outln!(output, "  {}✓{} Package database updated", GREEN, RESET);
        }
        _ => outln!(output, "pkg: unknown command '{}'. Try 'pkg' for help.", args[0]),
    }
}

// ─── User Authentication ────────────────────────────────────────────────

fn cmd_passwd(output: &mut Option<&mut String>) {
    outln!(output, "Changing password for root.");
    outln!(output, "New password: ********");
    outln!(output, "Retype password: ********");
    outln!(output, "passwd: password updated successfully");
}

fn cmd_su(args: &[&str], output: &mut Option<&mut String>) {
    let user = if args.is_empty() { "root" } else { args[0] };
    if user == "root" {
        outln!(output, "root@cantaya:/$");
    } else {
        outln!(output, "su: user '{}' does not exist", user);
    }
}

fn cmd_login(output: &mut Option<&mut String>) {
    outln!(output, "cantaya login: root");
    outln!(output, "Password: ********");
    outln!(output, "Last login: Sun Feb  9 00:00:00 on tty0");
    outln!(output, "Welcome to CantayaOS!");
}

fn cmd_chmod(args: &[&str], output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "chmod: usage: chmod <mode> <file>");
        return;
    }
    outln!(output, "mode of '{}' changed to {}", args[1], args[0]);
}

fn cmd_chown(args: &[&str], output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "chown: usage: chown <owner[:group]> <file>");
        return;
    }
    outln!(output, "ownership of '{}' changed to {}", args[1], args[0]);
}

// ─── Text Processing Tools ──────────────────────────────────────────────

fn cmd_awk(args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    // Basic awk: supports '{print $N}' and '/pattern/{print $N}'
    if args.is_empty() {
        outln!(output, "awk: usage: awk '<program>' [file]");
        return;
    }
    let program = args[0];
    let content = if args.len() > 1 {
        // Could read from file but this is simplified
        if let Some(data) = stdin { String::from(data) } else { String::new() }
    } else if let Some(data) = stdin {
        String::from(data)
    } else {
        outln!(output, "awk: no input");
        return;
    };

    // Parse program: check for pattern/action
    let (pattern, action) = if program.starts_with('/') {
        // /pattern/{action}
        if let Some(slash2) = program[1..].find('/') {
            let pat = &program[1..1 + slash2];
            let rest = &program[2 + slash2..];
            let act = rest.trim_start_matches('{').trim_end_matches('}').trim();
            (Some(pat), act)
        } else {
            (None, program.trim_start_matches('{').trim_end_matches('}').trim())
        }
    } else {
        (None, program.trim_start_matches('{').trim_end_matches('}').trim())
    };

    for line in content.lines() {
        // Check pattern match
        if let Some(pat) = pattern {
            if !line.contains(pat) { continue; }
        }

        let fields: Vec<&str> = line.split_whitespace().collect();

        // Parse action
        if action.starts_with("print") {
            let print_args = action[5..].trim();
            if print_args.is_empty() || print_args == "$0" {
                outln!(output, "{}", line);
            } else {
                let mut first = true;
                for arg in print_args.split(',') {
                    let arg = arg.trim();
                    if !first { out!(output, " "); }
                    first = false;
                    if arg.starts_with('$') {
                        if let Ok(n) = arg[1..].parse::<usize>() {
                            if n == 0 {
                                out!(output, "{}", line);
                            } else if n <= fields.len() {
                                out!(output, "{}", fields[n - 1]);
                            }
                        }
                    } else if arg.starts_with('"') && arg.ends_with('"') {
                        out!(output, "{}", &arg[1..arg.len()-1]);
                    } else {
                        out!(output, "{}", arg);
                    }
                }
                outln!(output);
            }
        } else if action == "NR" || action == "print NR" {
            // Print line number and line
            // NR tracking would require state; just print the line
            outln!(output, "{}", line);
        }
    }
}

fn cmd_cut(args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    // cut -f N [-d 'delim']
    let mut field_num = 1usize;
    let mut delimiter = '\t';
    let mut i = 0;
    while i < args.len() {
        if args[i] == "-f" && i + 1 < args.len() {
            field_num = args[i + 1].parse().unwrap_or(1);
            i += 2;
        } else if args[i] == "-d" && i + 1 < args.len() {
            let d = args[i + 1];
            delimiter = d.trim_matches('\'').trim_matches('"').chars().next().unwrap_or('\t');
            i += 2;
        } else {
            i += 1;
        }
    }

    let content = stdin.unwrap_or("");
    for line in content.lines() {
        let fields: Vec<&str> = line.split(delimiter).collect();
        if field_num > 0 && field_num <= fields.len() {
            outln!(output, "{}", fields[field_num - 1]);
        } else {
            outln!(output, "{}", line);
        }
    }
}

fn cmd_tr(args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    if args.len() < 2 {
        outln!(output, "tr: usage: tr <from> <to>");
        return;
    }
    let from_chars: Vec<char> = args[0].chars().collect();
    let to_chars: Vec<char> = args[1].chars().collect();
    let content = stdin.unwrap_or("");

    for ch in content.chars() {
        if let Some(pos) = from_chars.iter().position(|c| *c == ch) {
            if pos < to_chars.len() {
                out!(output, "{}", to_chars[pos]);
            }
        } else {
            out!(output, "{}", ch);
        }
    }
}

fn cmd_rev(stdin: Option<&str>, output: &mut Option<&mut String>) {
    let content = stdin.unwrap_or("");
    for line in content.lines() {
        let reversed: String = line.chars().rev().collect();
        outln!(output, "{}", reversed);
    }
}

fn cmd_basename(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "basename: usage: basename <path> [suffix]");
        return;
    }
    let path = args[0].trim_end_matches('/');
    let name = match path.rfind('/') {
        Some(pos) => &path[pos + 1..],
        None => path,
    };
    let result = if args.len() > 1 {
        name.strip_suffix(args[1]).unwrap_or(name)
    } else {
        name
    };
    outln!(output, "{}", result);
}

fn cmd_dirname(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "dirname: usage: dirname <path>");
        return;
    }
    let path = args[0];
    match path.rfind('/') {
        Some(0) => outln!(output, "/"),
        Some(pos) => outln!(output, "{}", &path[..pos]),
        None => outln!(output, "."),
    }
}

fn cmd_xargs(shell: &mut Shell, args: &[&str], stdin: Option<&str>, output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "xargs: usage: cmd1 | xargs <cmd>");
        return;
    }
    let cmd_name = args[0];
    let extra_args: Vec<&str> = args[1..].to_vec();
    let content = stdin.unwrap_or("");
    // Build all items from stdin into one command
    let mut all_args: Vec<&str> = extra_args;
    for line in content.lines() {
        let line = line.trim();
        if !line.is_empty() {
            all_args.push(line);
        }
    }
    execute_command(shell, cmd_name, &all_args, None, None);
}

fn cmd_yes(args: &[&str], output: &mut Option<&mut String>) {
    let text = if args.is_empty() { "y" } else { args[0] };
    // Print limited lines to avoid infinite loop
    for _ in 0..20 {
        outln!(output, "{}", text);
    }
}

fn cmd_readlink(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "readlink: usage: readlink <path>");
        return;
    }
    let full = resolve_path(&shell.cwd, args[0]);
    // In our FS symlinks are just regular files/copies
    outln!(output, "{}", full);
}

// ─── Line Editor ─────────────────────────────────────────────────────────

fn cmd_edit(shell: &Shell, args: &[&str]) {
    if args.is_empty() {
        kprintln!("edit: usage: edit <file>");
        return;
    }
    let path = resolve_path(&shell.cwd, args[0]);

    // Load existing content or start empty
    let mut lines: Vec<String> = Vec::new();
    if let Some(text) = read_file_string(&path) {
        for line in text.lines() {
            lines.push(String::from(line));
        }
    }

    kprint!("\x1b[2J\x1b[H");
    kprintln!("{}{}CantayaOS Editor{} — {}  ({} lines)", BOLD, CYAN, RESET, args[0], lines.len());
    kprintln!("{}Commands: :w save | :q quit | :wq save+quit | :d N delete line | :i N insert before{}", DIM, RESET);
    kprintln!("{}{}{}", DIM, "─".repeat(60), RESET);

    // Display content
    let display_lines = |lines: &Vec<String>| {
        for (i, line) in lines.iter().enumerate() {
            kprintln!("{}{:>4}{} │ {}", DIM, i + 1, RESET, line);
        }
        if lines.is_empty() {
            kprintln!("{}  (empty file){}", DIM, RESET);
        }
    };
    display_lines(&lines);

    // Edit loop
    loop {
        kprint!("\n{}edit>{} ", GREEN, RESET);
        let mut input_buf = [0u8; 256];
        let n = hal::console::read_line(&mut input_buf);
        let input = core::str::from_utf8(&input_buf[..n]).unwrap_or("").trim();
        if input.is_empty() { continue; }

        if input == ":q" {
            kprintln!("Exiting editor.");
            break;
        } else if input == ":w" {
            save_file(&path, &lines);
            kprintln!("{}Saved {} ({} lines){}", GREEN, args[0], lines.len(), RESET);
        } else if input == ":wq" {
            save_file(&path, &lines);
            kprintln!("{}Saved {} ({} lines){}", GREEN, args[0], lines.len(), RESET);
            break;
        } else if input.starts_with(":d ") {
            // Delete line
            if let Ok(n) = input[3..].trim().parse::<usize>() {
                if n >= 1 && n <= lines.len() {
                    lines.remove(n - 1);
                    kprint!("\x1b[2J\x1b[H");
                    kprintln!("{}Deleted line {}{}", YELLOW, n, RESET);
                    display_lines(&lines);
                } else {
                    kprintln!("Invalid line number");
                }
            }
        } else if input.starts_with(":i ") {
            // Insert before line
            if let Ok(n) = input[3..].trim().parse::<usize>() {
                if n >= 1 && n <= lines.len() + 1 {
                    kprint!("  text> ");
                    let mut lb = [0u8; 256];
                    let ln = hal::console::read_line(&mut lb);
                    let new_text = core::str::from_utf8(&lb[..ln]).unwrap_or("");
                    lines.insert(n - 1, String::from(new_text.trim_end()));
                    kprint!("\x1b[2J\x1b[H");
                    kprintln!("{}Inserted at line {}{}", GREEN, n, RESET);
                    display_lines(&lines);
                } else {
                    kprintln!("Invalid line number");
                }
            }
        } else if input.starts_with(":r ") {
            // Replace line
            let rest = &input[3..];
            if let Some(sp) = rest.find(' ') {
                if let Ok(n) = rest[..sp].parse::<usize>() {
                    if n >= 1 && n <= lines.len() {
                        lines[n - 1] = String::from(&rest[sp + 1..]);
                        kprint!("\x1b[2J\x1b[H");
                        kprintln!("{}Replaced line {}{}", GREEN, n, RESET);
                        display_lines(&lines);
                    }
                }
            }
        } else if input == ":p" || input == ":print" {
            kprint!("\x1b[2J\x1b[H");
            kprintln!("{}{}CantayaOS Editor{} — {}", BOLD, CYAN, RESET, args[0]);
            kprintln!("{}{}{}", DIM, "─".repeat(60), RESET);
            display_lines(&lines);
        } else if input.starts_with(":a") {
            // Append a line at end
            kprint!("  text> ");
            let mut lb = [0u8; 256];
            let ln = hal::console::read_line(&mut lb);
            let new_text = core::str::from_utf8(&lb[..ln]).unwrap_or("");
            lines.push(String::from(new_text.trim_end()));
            kprintln!("{}Appended line {}{}", GREEN, lines.len(), RESET);
        } else {
            // Treat as a new line to append
            lines.push(String::from(input));
            kprintln!("{}Line {}: {}{}", DIM, lines.len(), input, RESET);
        }
    }
}

fn save_file(path: &str, lines: &Vec<String>) {
    let _ = fs::vfs::unlink(path);
    if let Ok(mut handle) = fs::create(path) {
        for (i, line) in lines.iter().enumerate() {
            let _ = fs::write(&mut handle, line.as_bytes());
            if i + 1 < lines.len() {
                let _ = fs::write(&mut handle, b"\n");
            }
        }
        if !lines.is_empty() {
            let _ = fs::write(&mut handle, b"\n");
        }
        fs::close(handle);
    }
}

// ─── Man Pages ───────────────────────────────────────────────────────────

fn cmd_man(args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "man: usage: man <command>");
        outln!(output, "Available topics: ls, cat, grep, ping, ps, ifconfig, netstat, edit, man, find, test");
        return;
    }
    let topic = args[0];
    outln!(output, "");
    outln!(output, "{}{}{}(1){}                CantayaOS Manual              {}{}{}(1){}", BOLD, WHITE, topic, RESET, BOLD, WHITE, topic, RESET);
    outln!(output, "");

    match topic {
        "ls" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       ls - list directory contents");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       ls [-l] [-a] [directory]");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       List information about files in the specified directory");
            outln!(output, "       (current directory by default).");
            outln!(output, "");
            outln!(output, "{}OPTIONS{}", BOLD, RESET);
            outln!(output, "       -l     use long listing format");
            outln!(output, "       -a     show hidden files (files starting with .)");
        }
        "cat" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       cat - concatenate files and print to standard output");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       cat [file...]");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Concatenate file(s) to standard output. With no file,");
            outln!(output, "       or when used in a pipeline, reads from standard input.");
        }
        "grep" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       grep - search for patterns in text");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       grep [-i] [-n] [-v] [-c] <pattern> [file...]");
            outln!(output, "");
            outln!(output, "{}OPTIONS{}", BOLD, RESET);
            outln!(output, "       -i     case insensitive search");
            outln!(output, "       -n     show line numbers");
            outln!(output, "       -v     invert match (show non-matching lines)");
            outln!(output, "       -c     count matching lines only");
        }
        "ping" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       ping - send ICMP ECHO_REQUEST to network hosts");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       ping [-c count] <host|ip>");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Send ICMP echo requests to the specified host.");
            outln!(output, "       Simulated network - supports localhost, LAN, and");
            outln!(output, "       well-known hosts (google.com, github.com, etc).");
            outln!(output, "");
            outln!(output, "{}OPTIONS{}", BOLD, RESET);
            outln!(output, "       -c N   stop after N packets (default: 4)");
        }
        "ps" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       ps - report process status");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Displays information about active processes including");
            outln!(output, "       PID, state, priority, thread count, and name.");
        }
        "ifconfig" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       ifconfig - configure or show network interface parameters");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Display configuration of all network interfaces.");
            outln!(output, "       Shows IP address, netmask, MAC address, MTU, and");
            outln!(output, "       packet/byte counters for each interface.");
        }
        "netstat" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       netstat - display network connections");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       netstat [-a]");
            outln!(output, "");
            outln!(output, "{}OPTIONS{}", BOLD, RESET);
            outln!(output, "       -a     show all connections including listening sockets");
        }
        "edit" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       edit - simple line editor");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       edit <file>");
            outln!(output, "");
            outln!(output, "{}COMMANDS{}", BOLD, RESET);
            outln!(output, "       :w          save file");
            outln!(output, "       :q          quit");
            outln!(output, "       :wq         save and quit");
            outln!(output, "       :d N        delete line N");
            outln!(output, "       :i N        insert before line N");
            outln!(output, "       :r N text   replace line N with text");
            outln!(output, "       :a          append new line at end");
            outln!(output, "       :p          print/redisplay file");
            outln!(output, "       <text>      append text as new line");
        }
        "find" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       find - search for files in a directory tree");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       find <path> [-name <pattern>]");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Search recursively for entries matching the name pattern.");
            outln!(output, "       Supports glob wildcards: *, prefix*, *suffix.");
        }
        "test" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       test - evaluate conditional expressions");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       test <expression>");
            outln!(output, "");
            outln!(output, "{}EXPRESSIONS{}", BOLD, RESET);
            outln!(output, "       -f <file>    true if file exists and is regular");
            outln!(output, "       -d <path>    true if path is a directory");
            outln!(output, "       -e <path>    true if path exists");
            outln!(output, "       -z <str>     true if string is empty");
            outln!(output, "       -n <str>     true if string is non-empty");
            outln!(output, "       s1 = s2      true if strings are equal");
            outln!(output, "       s1 != s2     true if strings differ");
        }
        "man" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       man - display manual pages");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       man <command>");
            outln!(output, "");
            outln!(output, "{}AVAILABLE PAGES{}", BOLD, RESET);
            outln!(output, "       ls, cat, grep, ping, ps, ifconfig, netstat, edit,");
            outln!(output, "       man, find, test, logger, for, if, while");
        }
        "logger" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       logger - log messages to system log");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       logger [-p level] <message>");
            outln!(output, "");
            outln!(output, "{}LEVELS{}", BOLD, RESET);
            outln!(output, "       emerg, alert, crit, err, warning, notice, info, debug");
        }
        "for" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       for - loop over a list of items");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       for <var> in <item1> <item2> ...; do <commands>; done");
            outln!(output, "");
            outln!(output, "{}EXAMPLE{}", BOLD, RESET);
            outln!(output, "       for f in /etc /tmp /home; do ls $f; done");
        }
        "if" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       if - conditional execution");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       if <command>; then <commands>; [elif <cmd>; then ...;] [else ...;] fi");
            outln!(output, "");
            outln!(output, "{}EXAMPLE{}", BOLD, RESET);
            outln!(output, "       if test -f /etc/motd; then cat /etc/motd; else echo no motd; fi");
        }
        "while" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       while - loop while condition is true");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       while <command>; do <commands>; done");
        }
        "service" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       service - manage system services");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       service list|start|stop|restart|status|enable|disable [name]");
            outln!(output, "");
            outln!(output, "{}COMMANDS{}", BOLD, RESET);
            outln!(output, "       list           list all services");
            outln!(output, "       start <svc>    start a stopped service");
            outln!(output, "       stop <svc>     stop a running service");
            outln!(output, "       restart <svc>  restart a service");
            outln!(output, "       status <svc>   show detailed service status");
            outln!(output, "       enable <svc>   enable auto-start at boot");
            outln!(output, "       disable <svc>  disable auto-start at boot");
        }
        "pkg" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       pkg - package manager");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       pkg install|remove|list|search|info|update [args]");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Manage software packages. Repository: cantaya-core.");
            outln!(output, "       Available packages include git, vim, python3, nodejs,");
            outln!(output, "       nginx, htop, tmux, gcc, make, curl, and more.");
        }
        "top" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       top - display system task overview");
            outln!(output, "");
            outln!(output, "{}DESCRIPTION{}", BOLD, RESET);
            outln!(output, "       Shows CPU, memory, and process information");
            outln!(output, "       similar to htop/top on Linux.");
        }
        "awk" => {
            outln!(output, "{}NAME{}", BOLD, RESET);
            outln!(output, "       awk - pattern scanning and text processing");
            outln!(output, "");
            outln!(output, "{}SYNOPSIS{}", BOLD, RESET);
            outln!(output, "       awk '{{print $N}}' [file]");
            outln!(output, "       awk '/pattern/{{print $N}}'");
            outln!(output, "");
            outln!(output, "{}EXAMPLES{}", BOLD, RESET);
            outln!(output, "       ps | awk '{{print $1, $5}}'");
            outln!(output, "       cat /etc/passwd | awk '{{print $1}}'");
        }
        _ => {
            outln!(output, "  No manual entry for '{}'", topic);
            outln!(output, "  Available: ls, cat, grep, ping, ps, ifconfig, netstat, edit,");
            outln!(output, "             man, find, test, logger, for, if, while, service,");
            outln!(output, "             pkg, top, awk, nslookup, du, fdisk, lsblk");
        }
    }
    outln!(output, "");
}

// ─── Control Flow ────────────────────────────────────────────────────────

/// Execute an if/then/elif/else/fi block
/// Supports: if <cmd>; then <cmds>; [elif <cmd>; then <cmds>;] [else <cmds>;] fi
/// Also supports: if <cmd>; then <cmds>; fi  (without semicolons around keywords)
fn execute_if(shell: &mut Shell, line: &str) {
    // Expand variables upfront (if has no loop vars that change per-iteration)
    let expanded_line = expand_vars(shell, line);
    let raw_parts: Vec<&str> = expanded_line.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    // Flatten: separate keyword prefixes from their trailing commands
    let mut tokens: Vec<String> = Vec::new();
    for part in &raw_parts {
        if let Some(rest) = part.strip_prefix("then ") {
            tokens.push(String::from("then"));
            if !rest.trim().is_empty() { tokens.push(String::from(rest.trim())); }
        } else if let Some(rest) = part.strip_prefix("else ") {
            tokens.push(String::from("else"));
            if !rest.trim().is_empty() { tokens.push(String::from(rest.trim())); }
        } else if let Some(_rest) = part.strip_prefix("elif ") {
            // Keep "elif <cond>" as one token for later parsing
            tokens.push(String::from(*part));
        } else if *part == "then" || *part == "else" || *part == "fi" {
            tokens.push(String::from(*part));
        } else {
            tokens.push(String::from(*part));
        }
    }

    let mut i = 0;
    let mut condition_met = false;

    // Parse "if <cmd>"
    if i >= tokens.len() || !tokens[i].starts_with("if ") { return; }
    let cond_cmd = String::from(tokens[i][3..].trim());
    i += 1;

    // Parse "then"
    if i >= tokens.len() || tokens[i] != "then" { return; }
    i += 1;

    // Execute condition
    execute_pipeline(shell, &cond_cmd);
    if shell.last_exit_code == 0 {
        condition_met = true;
        while i < tokens.len() {
            if tokens[i] == "fi" || tokens[i].starts_with("elif ") || tokens[i] == "else" { break; }
            execute_pipeline(shell, &tokens[i]);
            i += 1;
        }
    } else {
        while i < tokens.len() {
            if tokens[i] == "fi" || tokens[i].starts_with("elif ") || tokens[i] == "else" { break; }
            i += 1;
        }
    }

    // Handle elif chains
    while i < tokens.len() && tokens[i].starts_with("elif ") {
        if condition_met {
            i += 1;
            if i < tokens.len() && tokens[i] == "then" { i += 1; }
            while i < tokens.len() {
                if tokens[i] == "fi" || tokens[i].starts_with("elif ") || tokens[i] == "else" { break; }
                i += 1;
            }
            continue;
        }
        let elif_cmd = String::from(tokens[i][5..].trim());
        i += 1;
        if i >= tokens.len() || tokens[i] != "then" { break; }
        i += 1;
        execute_pipeline(shell, &elif_cmd);
        if shell.last_exit_code == 0 {
            condition_met = true;
            while i < tokens.len() {
                if tokens[i] == "fi" || tokens[i].starts_with("elif ") || tokens[i] == "else" { break; }
                execute_pipeline(shell, &tokens[i]);
                i += 1;
            }
        } else {
            while i < tokens.len() {
                if tokens[i] == "fi" || tokens[i].starts_with("elif ") || tokens[i] == "else" { break; }
                i += 1;
            }
        }
    }

    // Handle else
    if i < tokens.len() && tokens[i] == "else" {
        i += 1;
        if !condition_met {
            while i < tokens.len() {
                if tokens[i] == "fi" { break; }
                execute_pipeline(shell, &tokens[i]);
                i += 1;
            }
        }
    }
}

/// Execute a for/in/do/done loop
/// Format: for <var> in <items...>; do <cmds>; done
fn execute_for(shell: &mut Shell, line: &str) {
    let raw_parts: Vec<&str> = line.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    // Flatten "do" keyword prefix
    let mut tokens: Vec<String> = Vec::new();
    for part in &raw_parts {
        if let Some(rest) = part.strip_prefix("do ") {
            tokens.push(String::from("do"));
            if !rest.trim().is_empty() { tokens.push(String::from(rest.trim())); }
        } else if *part == "do" || *part == "done" {
            tokens.push(String::from(*part));
        } else {
            tokens.push(String::from(*part));
        }
    }

    if tokens.is_empty() { return; }

    // Expand vars on the "for" header to resolve e.g. $ITEMS
    let for_header = expand_vars(shell, &tokens[0]);
    let words: Vec<&str> = for_header.split_whitespace().collect();
    if words.len() < 4 || words[0] != "for" || words[2] != "in" { return; }
    let var_name = String::from(words[1]);
    let items: Vec<String> = words[3..].iter().map(|s| String::from(*s)).collect();

    // Find "do" and collect body commands until "done"
    let mut body_start = 1;
    if body_start < tokens.len() && tokens[body_start] == "do" {
        body_start += 1;
    }

    let mut body: Vec<String> = Vec::new();
    let mut end = body_start;
    while end < tokens.len() {
        if tokens[end] == "done" { break; }
        body.push(tokens[end].clone());
        end += 1;
    }

    // Execute loop
    let mut iterations = 0;
    for item in &items {
        shell.env.insert(var_name.clone(), item.clone());
        for cmd in &body {
            if !cmd.is_empty() {
                let expanded = expand_vars(shell, cmd);
                execute_pipeline(shell, &expanded);
            }
        }
        iterations += 1;
        if iterations > 100 { break; }
    }
}

/// Execute a while/do/done loop
/// Format: while <cmd>; do <cmds>; done
fn execute_while(shell: &mut Shell, line: &str) {
    let raw_parts: Vec<&str> = line.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

    // Flatten "do" keyword prefix
    let mut tokens: Vec<String> = Vec::new();
    for part in &raw_parts {
        if let Some(rest) = part.strip_prefix("do ") {
            tokens.push(String::from("do"));
            if !rest.trim().is_empty() { tokens.push(String::from(rest.trim())); }
        } else if *part == "do" || *part == "done" {
            tokens.push(String::from(*part));
        } else {
            tokens.push(String::from(*part));
        }
    }

    if tokens.is_empty() { return; }

    // Parse "while <cmd>"
    if !tokens[0].starts_with("while ") { return; }
    let cond_template = String::from(tokens[0][6..].trim());

    // Find "do" and collect body
    let mut body_start = 1;
    if body_start < tokens.len() && tokens[body_start] == "do" {
        body_start += 1;
    }

    let mut body: Vec<String> = Vec::new();
    let mut end = body_start;
    while end < tokens.len() {
        if tokens[end] == "done" { break; }
        body.push(tokens[end].clone());
        end += 1;
    }

    // Execute loop
    let mut iterations = 0;
    loop {
        let cond_expanded = expand_vars(shell, &cond_template);
        execute_pipeline(shell, &cond_expanded);
        if shell.last_exit_code != 0 { break; }
        for cmd in &body {
            if !cmd.is_empty() {
                let expanded = expand_vars(shell, cmd);
                execute_pipeline(shell, &expanded);
            }
        }
        iterations += 1;
        if iterations > 1000 { break; }
    }
}

// ─── ELF Commands ──────────────────────────────────────────────────────────

/// Display ELF file information (like readelf)
fn cmd_readelf(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "Usage: readelf <file>");
        return;
    }

    let path = resolve_path(&shell.cwd, args[0]);
    
    let data = match read_file_bytes(&path) {
        Some(d) => d,
        None => {
            outln!(output, "readelf: cannot read '{}'", args[0]);
            return;
        }
    };

    if !fs::elf::is_elf(&data) {
        outln!(output, "readelf: '{}' is not an ELF file", args[0]);
        return;
    }

    match fs::elf::parse_elf(&data) {
        Ok(info) => {
            outln!(output, "ELF Header:");
            outln!(output, "  Magic:   {:02x} {:02x} {:02x} {:02x}", 
                   data[0], data[1], data[2], data[3]);
            outln!(output, "  Class:                             ELF64");
            outln!(output, "  Data:                              little endian");
            outln!(output, "  Type:                              {}", 
                   if info.header.e_type == fs::elf::ET_EXEC { "EXEC" } 
                   else if info.header.e_type == fs::elf::ET_DYN { "DYN" }
                   else { "OTHER" });
            outln!(output, "  Machine:                           AArch64");
            outln!(output, "  Entry point address:               {:#x}", info.entry_point);
            outln!(output, "  Start of program headers:          {} (bytes)", info.header.e_phoff);
            outln!(output, "  Start of section headers:          {} (bytes)", info.header.e_shoff);
            outln!(output, "  Number of program headers:         {}", info.header.e_phnum);
            outln!(output, "  Number of section headers:         {}", info.header.e_shnum);
            outln!(output, "");
            outln!(output, "Program Headers:");
            outln!(output, "  Type      Offset     VirtAddr           FileSiz    MemSiz     Flg");
            for ph in &info.program_headers {
                let ptype = match ph.p_type {
                    fs::elf::PT_NULL => "NULL",
                    fs::elf::PT_LOAD => "LOAD",
                    fs::elf::PT_DYNAMIC => "DYNAMIC",
                    fs::elf::PT_INTERP => "INTERP",
                    fs::elf::PT_NOTE => "NOTE",
                    fs::elf::PT_PHDR => "PHDR",
                    _ => "...",
                };
                let flags = alloc::format!("{}{}{}",
                    if ph.is_readable() { "R" } else { " " },
                    if ph.is_writable() { "W" } else { " " },
                    if ph.is_executable() { "E" } else { " " });
                outln!(output, "  {:8} {:#010x} {:#018x} {:#010x} {:#010x} {}",
                       ptype, ph.p_offset, ph.p_vaddr, ph.p_filesz, ph.p_memsz, flags);
            }
        }
        Err(e) => {
            outln!(output, "readelf: error parsing ELF: {}", e.as_str());
        }
    }
}

/// Run/execute an ELF binary
fn cmd_run(shell: &Shell, args: &[&str], output: &mut Option<&mut String>) {
    if args.is_empty() {
        outln!(output, "Usage: run <elf-file> [&]");
        return;
    }

    // Check for background '&' suffix
    let background = args.last() == Some(&"&") || args[0].ends_with('&');
    let file_arg = if background && args.len() > 1 && args.last() == Some(&"&") {
        args[0]
    } else if background && args[0].ends_with('&') {
        &args[0][..args[0].len()-1]
    } else {
        args[0]
    };

    let path = resolve_path(&shell.cwd, file_arg);
    
    let data = match read_file_bytes(&path) {
        Some(d) => d,
        None => {
            outln!(output, "run: cannot read '{}'", file_arg);
            return;
        }
    };

    if !fs::elf::is_elf(&data) {
        outln!(output, "run: '{}' is not an ELF file", file_arg);
        return;
    }

    // Parse and describe the ELF
    outln!(output, "Loading: {}", fs::elf::describe_elf(&data));

    // Spawn as user-mode process
    let name = file_arg.rsplit('/').next().unwrap_or(file_arg);
    // Collect program arguments (program name + remaining args)
    let mut prog_args: Vec<&str> = Vec::new();
    prog_args.push(name);
    for &a in args.iter().skip(1) {
        if a == "&" { continue; }
        prog_args.push(a);
    }
    match crate::process::spawn_user_process(name, &data, 8, &prog_args) {
        Some(pid) => {
            if background {
                outln!(output, "  [bg] PID {} started", pid);
            } else {
                outln!(output, "  Spawned process PID {} (user-mode)", pid);
                // Set as foreground and wait
                crate::hal::console::set_foreground_pid(pid);
                let exit_code = crate::process::waitpid(pid);
                crate::hal::console::set_foreground_pid(0);
                if exit_code != 0 {
                    outln!(output, "  Process exited with code {}", exit_code);
                }
            }
        }
        None => {
            outln!(output, "run: failed to load or spawn process");
        }
    }
}
