// CantayaOS Shell — Miscellaneous / Utility Commands

extern crate alloc;

use alloc::string::String;
use crate::graphics::console;
use core::fmt::Write;

pub(crate) fn cmd_echo(args: &str) {
    console::println(args);
}

pub(crate) fn cmd_beep(args: &str) {
    let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
    let freq = parts.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(440);
    let dur = parts.get(1).and_then(|s| s.parse::<u64>().ok()).unwrap_or(200);
    let dur = dur.min(5000);
    let mut s = String::new();
    write!(s, "Beep: {} Hz for {} ms", freq, dur).ok();
    console::println(&s);
    crate::hal::speaker::beep(freq, dur);
}

pub(crate) fn cmd_hostname(args: &str) {
    let name = args.trim();
    if name.is_empty() {
        console::println(&crate::shell::get_hostname());
    } else {
        crate::shell::set_hostname_value(name);
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

pub(crate) fn cmd_whoami() {
    let user = crate::shell::env_get("USER").unwrap_or_else(|| "root".into());
    console::println(&user);
}

pub(crate) fn cmd_history() {
    let hist = crate::shell::CMD_HISTORY.lock();
    if hist.count == 0 {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("(no command history)");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Command History:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let start = if hist.count < crate::shell::HISTORY_SIZE {
        0
    } else {
        hist.write_idx
    };

    for i in 0..hist.count {
        let idx = (start + i) % crate::shell::HISTORY_SIZE;
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

pub(crate) fn cmd_cal() {
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

pub(crate) fn cmd_fortune() {
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

    let tick = crate::shell::ticks() as usize;
    let idx = tick % FORTUNES.len();
    console::set_color(0xFF, 0xFF, 0x55);
    console::print("  ");
    console::println(FORTUNES[idx]);
    console::set_color(0xFF, 0xFF, 0xFF);
}

pub(crate) fn cmd_banner(args: &str) {
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

pub(crate) fn cmd_env(args: &str) {
    crate::shell::env_init();
    let args = args.trim();
    if args.is_empty() {
        // Show all env vars
        let env = crate::shell::ENV_VARS.lock();
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
            crate::shell::env_set(key, value);
            let mut s = String::new();
            write!(s, "{}={}", key, value).ok();
            console::println(&s);
        } else {
            // Show single var
            match crate::shell::env_get(args) {
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

pub(crate) fn cmd_unset(args: &str) {
    if args.is_empty() {
        console::println("Usage: unset <variable>");
        return;
    }
    crate::shell::env_remove(args.trim());
    console::set_color(0x55, 0xFF, 0x55);
    let mut s = String::new();
    write!(s, "Unset '{}'", args.trim()).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
}

pub(crate) fn cmd_alias(args: &str) {
    crate::shell::aliases_init();
    if args.is_empty() {
        // List all aliases
        let a = crate::shell::ALIASES.lock();
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
            crate::shell::alias_set(name, value);
            let mut s = String::new();
            write!(s, "Alias '{}' set to '{}'", name, value).ok();
            console::println(&s);
        } else {
            console::println("Usage: alias name=command");
        }
    } else {
        // Show single alias
        if let Some(value) = crate::shell::alias_get(args.trim()) {
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

pub(crate) fn cmd_unalias(args: &str) {
    if args.is_empty() {
        console::println("Usage: unalias <name>");
        return;
    }
    crate::shell::alias_remove(args.trim());
    let mut s = String::new();
    write!(s, "Alias '{}' removed.", args.trim()).ok();
    console::println(&s);
}

pub(crate) fn cmd_run(args: &str) {
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
                        crate::shell::execute_command(line);
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

pub(crate) fn cmd_time(args: &str) {
    if args.is_empty() {
        console::println("Usage: time <command>");
        return;
    }

    let start = crate::shell::ticks();
    crate::shell::execute_command(args);
    let elapsed = crate::shell::ticks().wrapping_sub(start);
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

pub(crate) fn cmd_which(args: &str) {
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
    } else if crate::shell::alias_get(cmd_name).is_some() {
        let alias_val = crate::shell::alias_get(cmd_name).unwrap();
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

pub(crate) fn cmd_calc(args: &str) {
    if args.is_empty() {
        console::println("Usage: calc <expression>");
        console::println("  Supports: +, -, *, /, %, ()");
        console::println("  Example:  calc 2 + 3 * 4");
        console::println("  Example:  calc (10 + 5) * 2");
        return;
    }

    match crate::shell::calc::eval_expr(args) {
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

pub(crate) fn cmd_color(args: &str) {
    let scheme = args.trim();
    if crate::shell::apply_color_scheme(scheme) {
        crate::shell::save_color_scheme(scheme);
    }
}
