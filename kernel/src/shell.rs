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
    print_banner();

    let mut cmd_buf = [0u8; MAX_CMD_LEN];
    let mut cmd_len: usize = 0;
    let mut history = History::new();

    print_prompt();

    // Track last status bar update tick for periodic refresh
    let mut last_status_tick: u64 = 0;

    loop {
        // Periodically refresh the status bar (every ~1000 ticks = 1 second)
        let now = ticks();
        if now.wrapping_sub(last_status_tick) >= 1000 {
            last_status_tick = now;
            update_status_bar();
        }

        // Non-blocking key poll — if no key, HLT until next interrupt and retry
        let event = match keyboard::try_read_char() {
            Some(e) => e,
            None => {
                unsafe { core::arch::asm!("hlt"); }
                continue;
            }
        };

        match event.ascii {
            // Enter — execute command
            b'\n' => {
                console::print("\n");
                if cmd_len > 0 {
                    history.push(&cmd_buf, cmd_len);
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
                        if let Some((entry, len)) = history.go_up() {
                            for _ in 0..cmd_len { console::backspace(); }
                            cmd_buf[..len].copy_from_slice(entry);
                            cmd_len = len;
                            if let Ok(s) = core::str::from_utf8(&cmd_buf[..cmd_len]) {
                                console::print(s);
                            }
                        }
                    }
                    KeyCode::Down => {
                        if let Some((entry, len)) = history.go_down() {
                            for _ in 0..cmd_len { console::backspace(); }
                            cmd_buf[..len].copy_from_slice(entry);
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
        "acpi", "bootinfo", "cat", "cd", "clear", "cls", "color",
        "copy", "cp", "cpu", "date", "del", "desktop", "dir",
        "disk", "echo", "halt", "help", "hexdump",
        "interrupts", "irq", "kill", "ls", "lspci", "md", "mem",
        "memory", "memmap", "mkdir", "panic", "pci", "priority",
        "ps", "pwd", "reboot", "rm", "shutdown", "sleep", "spawn",
        "sysinfo", "tasks", "tick", "type", "uptime", "ver",
        "version", "write", "yield",
    ];
    COMMANDS.iter().find(|cmd| cmd.starts_with(prefix)).copied()
}

fn print_prompt() {
    update_status_bar();
    console::set_color(0x00, 0xCC, 0x00);
    console::print("cantaya");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::print("> ");
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
    write!(s, "CantayaOS v{} — x86_64 Hybrid Kernel", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);

    let total_mem_mib = crate::memory::frame_allocator::free_frame_count() * 4 / 1024;
    let mut info = String::new();
    write!(info, "{} MiB usable RAM | PIT 1000 Hz | 1920x1080 framebuffer", total_mem_mib).ok();
    console::set_color(0xAA, 0xAA, 0xAA);
    console::println(&info);
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("Type 'help' for available commands.\n");
}

/// Execute a shell command.
fn execute_command(input: &str) {
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
        "halt" | "shutdown" => cmd_halt(),
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
        "ls" | "dir" => cmd_ls(args),
        "cat" | "type" => cmd_cat(args),
        "write" => cmd_write(args),
        "mkdir" | "md" => cmd_mkdir(args),
        "rm" | "del" => cmd_rm(args),
        "cp" | "copy" => cmd_cp(args),
        "disk" => cmd_disk(),
        "cd" => cmd_cd(args),
        "pwd" => cmd_pwd(),
        _ => {
            let mut s = String::new();
            write!(s, "Unknown command: '{}'. Type 'help' for available commands.", cmd).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
        }
    }
}

// ============================================================================
// Command Implementations
// ============================================================================

fn cmd_help() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Available Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  help             Show this help message");
    console::println("  ver              Display kernel version");
    console::println("  sysinfo          System information dashboard");
    console::println("  mem              Display memory statistics");
    console::println("  memmap           Show memory map regions");
    console::println("  cpu              Display CPU information");
    console::println("  date             Show current date and time (RTC)");
    console::println("  desktop          Launch the graphical desktop environment");
    console::println("  tasks            List active tasks with priority/CPU");
    console::println("  spawn <task>     Spawn a task (counter/spinner/stress)");
    console::println("  kill <id>        Terminate a task by ID");
    console::println("  priority <id> <p> Set task priority (idle/low/normal/high/rt)");
    console::println("  sleep <ms>       Sleep current task for N milliseconds");
    console::println("  yield            Yield current time slice");
    console::println("  uptime           Show system uptime");
    console::println("  tick             Show timer tick count");
    console::println("  interrupts       Show IRQ statistics");
    console::println("  pci / lspci      Enumerate PCI devices");
    console::println("  acpi             Show ACPI information");
    console::println("  bootinfo         Show boot information");
    console::println("  hexdump <a> [n]  Dump n bytes at hex address a");
    console::println("  color <scheme>   Change color (green/amber/white/blue/default)");
    console::println("  echo <msg>       Echo text to console");
    console::println("  clear            Clear the screen");
    console::println("  panic            Trigger a kernel panic (for testing BSOD)");
    console::println("  halt             Shut down the system");
    console::println("  reboot           Reboot the system");
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("\nFilesystem Commands:");
    console::set_color(0xFF, 0xFF, 0xFF);
    console::println("  ls [path]        List directory contents");
    console::println("  cat <file>       Display file contents");
    console::println("  write <f> <text> Write text to a file");
    console::println("  mkdir <dir>      Create a directory");
    console::println("  rm <file|dir>    Delete a file or empty directory");
    console::println("  cp <src> <dst>   Copy a file");
    console::println("  cd <dir>         Change working directory");
    console::println("  pwd              Print working directory");
    console::println("  disk             Show disk/filesystem info");
    console::set_color(0xAA, 0xAA, 0xAA);
    console::println("\nTip: Up/Down arrows = history, Tab = completion");
    console::set_color(0xFF, 0xFF, 0xFF);
}

fn cmd_version() {
    let mut s = String::new();
    write!(s, "CantayaOS Kernel v{}", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("Architecture: x86_64 (AMD64)");
    console::println("Build: Rust nightly, no_std bare-metal");
    console::println("Kernel model: Hybrid (inspired by Windows NT)");
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
    match args.trim() {
        "green" | "matrix" => {
            console::set_color(0x00, 0xFF, 0x00);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: Matrix green");
        }
        "amber" => {
            console::set_color(0xFF, 0xB0, 0x00);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: Amber terminal");
        }
        "white" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x00, 0x00);
            console::clear();
            console::println("Color scheme: White on black");
        }
        "blue" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x00, 0xAA);
            console::clear();
            console::println("Color scheme: BSOD classic");
        }
        "default" => {
            console::set_color(0xFF, 0xFF, 0xFF);
            console::set_bg_color(0x00, 0x80, 0x80);
            console::clear();
            console::println("Color scheme: Default (teal)");
        }
        _ => {
            console::println("Usage: color <scheme>");
            console::println("  Schemes: green, amber, white, blue, default");
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

    unsafe {
        crate::hal::port::outb(0x604, 0x00);
        loop { core::arch::asm!("cli; hlt"); }
    }
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
    let entries = match vfs::list_dir(path) {
        Some(e) => e,
        None => {
            let mut s = String::new();
            write!(s, "ls: cannot access '{}': No such directory", path).ok();
            console::set_color(0xFF, 0x55, 0x55);
            console::println(&s);
            console::set_color(0xFF, 0xFF, 0xFF);
            return;
        }
    };

    if entries.is_empty() {
        console::set_color(0xAA, 0xAA, 0xAA);
        console::println("(empty directory)");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    // Print header
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("  Type       Size  Name");
    console::set_color(0xAA, 0xAA, 0xAA);
    console::println("  ----  ---------  ----");
    console::set_color(0xFF, 0xFF, 0xFF);

    for entry in &entries {
        let mut s = String::new();
        if entry.is_dir {
            console::set_color(0x55, 0xBB, 0xFF);
            write!(s, "  <DIR>            {}/", entry.name).ok();
        } else {
            console::set_color(0xFF, 0xFF, 0xFF);
            if entry.size >= 1024 * 1024 {
                write!(s, "  FILE  {:>5} MiB  {}", entry.size / (1024 * 1024), entry.name).ok();
            } else if entry.size >= 1024 {
                write!(s, "  FILE  {:>5} KiB  {}", entry.size / 1024, entry.name).ok();
            } else {
                write!(s, "  FILE  {:>5}   B  {}", entry.size, entry.name).ok();
            }
        }
        console::println(&s);
    }
    console::set_color(0xFF, 0xFF, 0xFF);

    // Summary
    let dirs = entries.iter().filter(|e| e.is_dir).count();
    let files = entries.iter().filter(|e| !e.is_dir).count();
    let total_size: u64 = entries.iter().map(|e| e.size as u64).sum();
    let mut s = String::new();
    console::set_color(0xAA, 0xAA, 0xAA);
    write!(s, "\n  {} file(s), {} dir(s), {} bytes total", files, dirs, total_size).ok();
    console::println(&s);
    console::set_color(0xFF, 0xFF, 0xFF);
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
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    if args.is_empty() {
        // Print current directory
        cmd_pwd();
        return;
    }

    if !vfs::cd(args) {
        let mut s = String::new();
        write!(s, "cd: '{}': No such directory", args).ok();
        console::set_color(0xFF, 0x55, 0x55);
        console::println(&s);
        console::set_color(0xFF, 0xFF, 0xFF);
    }
}

fn cmd_pwd() {
    use crate::storage::vfs;

    if !vfs::is_ready() {
        console::set_color(0xFF, 0x55, 0x55);
        console::println("No filesystem mounted.");
        console::set_color(0xFF, 0xFF, 0xFF);
        return;
    }

    console::println(&vfs::cwd());
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
