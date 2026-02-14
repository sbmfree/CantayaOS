// CantayaOS Shell — System Commands

extern crate alloc;

use alloc::string::String;
use crate::graphics::console;
use core::fmt::Write;

pub(crate) fn cmd_help() {
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
    console::println("  vmm              Virtual memory manager status");
    console::println("  vtop <vaddr>     Translate virtual address to physical");
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

pub(crate) fn cmd_version() {
    let mut s = String::new();
    console::println("");
    write!(s, "CantayaOS [Version {}]", env!("CARGO_PKG_VERSION")).ok();
    console::println(&s);
    console::println("Architecture: x86_64 (AMD64)");
    console::println("Kernel: Hybrid (Rust, no_std bare-metal)");
    console::println("Filesystem: FAT32 with Windows-like hierarchy");
    console::println("Boot: UEFI");
}

pub(crate) fn cmd_memory() {
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

pub(crate) fn cmd_cpu() {
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

pub(crate) fn cmd_uptime() {
    let tick_count = crate::shell::ticks();
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

pub(crate) fn cmd_tick() {
    let mut s = String::new();
    let rate = crate::hal::pit::tick_rate_hz();
    write!(s, "Timer ticks: {} ({} Hz, {} ms/tick)", crate::shell::ticks(), rate,
        if rate > 0 { 1000 / rate } else { 0 }).ok();
    console::println(&s);
}

pub(crate) fn cmd_clear() {
    console::clear();
}

pub(crate) fn cmd_tasks() {
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

pub(crate) fn cmd_spawn(args: &str) {
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

pub(crate) fn cmd_kill(args: &str) {
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

pub(crate) fn cmd_yield() {
    crate::core_kernel::scheduler::yield_now();
    console::println("Yielded time slice");
}

pub(crate) fn cmd_halt() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Shutting down CantayaOS...");
    console::set_color(0xFF, 0xFF, 0xFF);
    log::info!("System halt requested by user");
    crate::hal::speaker::beep(440, 100);
    crate::hal::acpi::acpi_shutdown();
}

pub(crate) fn cmd_reboot() {
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

pub(crate) fn cmd_sysinfo() {
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

    let ms = crate::hal::pit::ticks_to_ms(crate::shell::ticks());
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

pub(crate) fn cmd_bootinfo() {
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

pub(crate) fn cmd_memmap() {
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

pub(crate) fn cmd_panic() {
    console::set_color(0xFF, 0xFF, 0x00);
    console::println("Triggering kernel panic for BSOD test...");
    console::set_color(0xFF, 0xFF, 0xFF);
    panic!("User-triggered panic via 'panic' command");
}

pub(crate) fn cmd_interrupts() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("IRQ Statistics:");
    console::set_color(0xFF, 0xFF, 0xFF);

    let timer = crate::shell::IRQ_TIMER_COUNT.load(core::sync::atomic::Ordering::Relaxed);
    let kbd = crate::shell::IRQ_KEYBOARD_COUNT.load(core::sync::atomic::Ordering::Relaxed);
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

pub(crate) fn cmd_hexdump(args: &str) {
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
pub(crate) fn cmd_pci() {
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

pub(crate) fn cmd_acpi() {
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

pub(crate) fn cmd_sleep(args: &str) {
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

pub(crate) fn cmd_priority(args: &str) {
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

pub(crate) fn cmd_desktop() {
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

pub(crate) fn cmd_date() {
    use crate::hal::rtc;

    let dt = rtc::read_datetime();
    let mut s = String::new();
    let mut buf = [0u8; 20];
    let formatted = dt.format(&mut buf);
    write!(s, "Date/Time: {}", formatted).ok();
    console::println(&s);
}

pub(crate) fn cmd_dmesg(args: &str) {
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

pub(crate) fn cmd_free() {
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

pub(crate) fn cmd_top() {
    use crate::core_kernel::scheduler;

    // Interactive task monitor — refreshes until 'q' is pressed
    loop {
        console::clear();
        console::set_color(0xFF, 0xFF, 0x55);

        // Header with uptime and general stats
        let tick_count = crate::shell::ticks();
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
        let end_tick = crate::shell::ticks() + 1000;
        loop {
            if crate::shell::ticks() >= end_tick {
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

pub(crate) fn cmd_drivers() {
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

pub(crate) fn cmd_neofetch() {
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
    let tick_count = crate::shell::ticks();
    let ms = crate::hal::pit::ticks_to_ms(tick_count);
    let uptime_s = ms / 1000;
    let hours = uptime_s / 3600;
    let minutes = (uptime_s % 3600) / 60;
    let secs = uptime_s % 60;

    let free_frames = crate::memory::frame_allocator::free_frame_count();
    let total_frames = crate::memory::frame_allocator::total_frame_count();
    let used_frames = total_frames - free_frames;

    let hostname = crate::shell::get_hostname();

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

pub(crate) fn cmd_lsblk() {
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

/// Display VMM (Virtual Memory Manager) status and statistics.
pub(crate) fn cmd_vmm() {
    console::set_color(0xFF, 0xFF, 0x55);
    console::println("Virtual Memory Manager Status:");
    console::set_color(0xFF, 0xFF, 0xFF);

    match crate::memory::vmm::stats() {
        Some(stats) => {
            let mut s = String::new();

            write!(s, "  PML4 (CR3):          {:#x}", crate::memory::paging::kernel_pml4()).ok();
            console::println(&s);

            s.clear();
            write!(s, "  Total mapped:        {} KiB ({} pages)",
                stats.total_mapped_bytes / 1024,
                stats.total_mapped_bytes / 4096).ok();
            console::println(&s);

            s.clear();
            write!(s, "  Active regions:      {}", stats.region_count).ok();
            console::println(&s);

            s.clear();
            write!(s, "  Heap mapped:         {} KiB", stats.heap_used_bytes / 1024).ok();
            console::println(&s);

            s.clear();
            write!(s, "  VMM region used:     {} KiB / {} MiB",
                stats.vmm_region_used / 1024,
                (crate::memory::vmm::VMM_REGION_END - crate::memory::vmm::VMM_REGION_START) / (1024 * 1024)).ok();
            console::println(&s);

            s.clear();
            write!(s, "  Next alloc addr:     {:#x}", stats.next_alloc_addr).ok();
            console::println(&s);

            s.clear();
            write!(s, "  Free regions:        {}", stats.free_region_count).ok();
            console::println(&s);

            // Show virt-to-phys translation test
            console::println("");
            console::set_color(0xAA, 0xAA, 0xAA);
            console::println("  Address Translation Test:");
            let test_addrs: &[(u64, &str)] = &[
                (0xFFFFFFFF80000000, "kernel .text"),
                (crate::memory::vmm::HEAP_REGION_START, "heap start"),
            ];
            for &(addr, label) in test_addrs {
                s.clear();
                match crate::memory::vmm::virt_to_phys(addr) {
                    Some(phys) => write!(s, "    {:#018x} ({}) -> {:#x}", addr, label, phys).ok(),
                    None       => write!(s, "    {:#018x} ({}) -> NOT MAPPED", addr, label).ok(),
                };
                console::println(&s);
            }
        }
        None => {
            console::println("  VMM not initialized");
        }
    }

    console::set_color(0xFF, 0xFF, 0xFF);
}

/// Translate a virtual address to physical using the VMM.
pub(crate) fn cmd_vtop(args: &str) {
    let addr_str = args.trim();
    if addr_str.is_empty() {
        console::println("Usage: vtop <virtual_address>");
        console::println("  Example: vtop 0xFFFFFFFF80000000");
        return;
    }

    let addr = if let Some(hex) = addr_str.strip_prefix("0x").or_else(|| addr_str.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).ok()
    } else {
        addr_str.parse::<u64>().ok()
    };

    match addr {
        Some(vaddr) => {
            let mut s = String::new();
            match crate::memory::vmm::virt_to_phys(vaddr) {
                Some(phys) => {
                    write!(s, "  Virtual  {:#018x}", vaddr).ok();
                    console::println(&s);
                    s.clear();
                    write!(s, "  Physical {:#018x}", phys).ok();
                    console::println(&s);
                }
                None => {
                    write!(s, "  {:#018x} is NOT MAPPED", vaddr).ok();
                    console::println(&s);
                }
            }
        }
        None => {
            console::println("Invalid address format");
        }
    }
}
