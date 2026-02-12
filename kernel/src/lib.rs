//! CantayaOS Kernel — A Hybrid Kernel for ARM64
//!
//! This kernel follows a Windows NT-like hybrid architecture combining
//! microkernel modularity with monolithic kernel performance.
//!
//! Boot sequence:
//!   1. Bootloader sets up EL1, jumps to `_start` (boot.rs)
//!   2. `_start` sets up stack, clears BSS, calls `kernel_main`
//!   3. `kernel_main` initializes subsystems in order:
//!      HAL → Arch → Memory → Heap → Process → IPC → FS → Drivers
//!   4. Enters idle loop (scheduler takes over via timer IRQ)

#![no_std]
#![feature(alloc_error_handler)]

extern crate alloc;

pub mod arch;
pub mod sync;
pub mod mm;
pub mod process;
pub mod ipc;
pub mod hal;
pub mod drivers;
pub mod fs;
pub mod shell;
pub mod init;

use core::panic::PanicInfo;

/// Kernel version information
pub const KERNEL_VERSION: &str = "0.1.0";
pub const KERNEL_NAME: &str = "CantayaOS";
pub const KERNEL_ARCH: &str = "AArch64";

/// Kernel entry point called after bootloader
#[no_mangle]
pub extern "C" fn kernel_main() -> ! {
    // ---- Phase 0: Critical early init (before any interrupts can fire) ----
    // Initialize UART first so all subsequent prints work
    hal::console::init();
    
    // CPU features and exception vectors
    arch::aarch64::cpu::init();
    arch::aarch64::exceptions::init();
    
    // ---- Phase 1: Hardware Abstraction Layer (GIC, timer, syslog) ----
    hal::init();

    // ANSI color codes
    const CYAN: &str = "\x1b[36m";
    const GREEN: &str = "\x1b[32m";
    const YELLOW: &str = "\x1b[33m";
    const WHITE: &str = "\x1b[97m";
    const BOLD: &str = "\x1b[1m";
    const DIM: &str = "\x1b[2m";
    const RESET: &str = "\x1b[0m";
    const BLUE: &str = "\x1b[34m";
    const _MAGENTA: &str = "\x1b[35m";
    const _RED: &str = "\x1b[31m";

    kprintln!("");
    kprintln!("{}{}      ██████╗ █████╗ ███╗   ██╗████████╗ █████╗ ██╗   ██╗ █████╗ {}", BOLD, CYAN, RESET);
    kprintln!("{}{}     ██╔════╝██╔══██╗████╗  ██║╚══██╔══╝██╔══██╗╚██╗ ██╔╝██╔══██╗{}", BOLD, CYAN, RESET);
    kprintln!("{}{}     ██║     ███████║██╔██╗ ██║   ██║   ███████║ ╚████╔╝ ███████║{}", BOLD, CYAN, RESET);
    kprintln!("{}{}     ██║     ██╔══██║██║╚██╗██║   ██║   ██╔══██║  ╚██╔╝  ██╔══██║{}", BOLD, CYAN, RESET);
    kprintln!("{}{}     ╚██████╗██║  ██║██║ ╚████║   ██║   ██║  ██║   ██║   ██║  ██║{}", BOLD, CYAN, RESET);
    kprintln!("{}{}      ╚═════╝╚═╝  ╚═╝╚═╝  ╚═══╝   ╚═╝   ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝{}", BOLD, CYAN, RESET);
    kprintln!("{}{}                       ╔═════════════════╗{}", DIM, BLUE, RESET);
    kprintln!("{}{}                       ║  {}{}O S  v{} {}{}║{}", DIM, BLUE, BOLD, WHITE, KERNEL_VERSION, DIM, BLUE, RESET);
    kprintln!("{}{}                       ╚═════════════════╝{}", DIM, BLUE, RESET);
    kprintln!("");
    kprintln!("{}  Hybrid Kernel for ARM64  ·  NT-like Architecture{}", DIM, RESET);
    kprintln!("{}{}  ─────────────────────────────────────────────────────────{}", DIM, BLUE, RESET);
    kprintln!("");

    // ---- Phase 2: Architecture init (already done in Phase 0, just show status) ----
    boot_status(YELLOW, "ARCH", "Initializing CPU & exceptions...");
    // arch::aarch64::cpu::init(); -- done early in Phase 0
    // arch::aarch64::exceptions::init(); -- done early in Phase 0
    boot_ok(GREEN, "ARCH", "CPU features, exception vectors ready");

    // ---- Phase 3: Memory Management ----
    boot_status(YELLOW, "MEM ", "Initializing memory management...");
    mm::init();
    boot_ok(GREEN, "MEM ", "Page tables, MMU, heap initialized");

    // ---- Phase 4: Process Manager (Executive layer) ----
    boot_status(YELLOW, "PROC", "Initializing process manager...");
    process::init();
    boot_ok(GREEN, "PROC", "NT-like process manager ready");
    
    // ---- Phase 5: IPC subsystem ----
    boot_status(YELLOW, "IPC ", "Initializing IPC subsystem...");
    ipc::init();
    boot_ok(GREEN, "IPC ", "Mutex, pipes, events, semaphores ready");

    // ---- Phase 6: Filesystem ----
    boot_status(YELLOW, "FS  ", "Initializing filesystem...");
    fs::init();
    boot_ok(GREEN, "FS  ", "VFS, ramfs (/), devfs (/dev), procfs (/proc) mounted");
    
    // ---- Phase 7: Driver framework ----
    boot_status(YELLOW, "DRV ", "Initializing driver framework...");
    drivers::init();
    boot_ok(GREEN, "DRV ", "PCI enumeration, virtual NIC (eth0: 10.0.2.15) ready");

    // ---- Phase 8: Service Manager ----
    boot_status(YELLOW, "SVC ", "Starting system services...");
    hal::services::init();
    let svc_count = hal::services::running_count();
    boot_ok(GREEN, "SVC ", &alloc::format!("{} services started (sshd, crond, dhcpcd, ntpd)",
        svc_count));

    // Log boot events to syslog
    hal::syslog::log(hal::syslog::LogLevel::Info, hal::syslog::Facility::Kernel, "kernel: boot sequence completed");
    hal::syslog::log(hal::syslog::LogLevel::Info, hal::syslog::Facility::Kernel, "net: eth0 configured 10.0.2.15/24 gw 10.0.2.2");
    hal::syslog::log(hal::syslog::LogLevel::Info, hal::syslog::Facility::Kernel, "fs: ramfs mounted on / (rw)");
    hal::syslog::log(hal::syslog::LogLevel::Info, hal::syslog::Facility::Kernel, "fs: procfs mounted on /proc (ro)");
    hal::syslog::log(hal::syslog::LogLevel::Info, hal::syslog::Facility::Kernel, "fs: devfs mounted on /dev (rw)");
    hal::syslog::log(hal::syslog::LogLevel::Notice, hal::syslog::Facility::Daemon, "sshd: listening on 0.0.0.0:22");
    hal::syslog::log(hal::syslog::LogLevel::Notice, hal::syslog::Facility::Daemon, "dhcpcd: bound 10.0.2.15 lease 86400s");

    // ---- Boot complete ----
    kprintln!("");
    kprintln!("{}{}  ─────────────────────────────────────────────────────────{}", DIM, BLUE, RESET);
    kprintln!("");

    let free_kb = mm::physical::free_memory() / 1024;
    let free_mb = free_kb / 1024;
    kprintln!("  {}{}System Information:{}", BOLD, WHITE, RESET);
    kprintln!("  {}├─{} CPU        {}Cortex-A72 (AArch64){}", DIM, RESET, WHITE, RESET);
    kprintln!("  {}├─{} Memory     {}{} MB free ({} KB){}", DIM, RESET, WHITE, free_mb, free_kb, RESET);
    kprintln!("  {}├─{} Kernel     {}{} v{}{}", DIM, RESET, WHITE, KERNEL_NAME, KERNEL_VERSION, RESET);
    kprintln!("  {}├─{} Scheduler  {}32-level preemptive{}", DIM, RESET, WHITE, RESET);

    // Filesystem tree
    kprintln!("  {}└─{} Filesystem", DIM, RESET);
    if let Ok(entries) = fs::readdir("/") {
        let len = entries.len();
        for (i, entry) in entries.iter().enumerate() {
            let connector = if i == len - 1 { "└──" } else { "├──" };
            let icon = match entry.file_type {
                fs::FileType::Directory => "📁",
                fs::FileType::Regular => "📄",
                fs::FileType::Device => "⚙\u{fe0f}",
                _ => "📎",
            };
            kprintln!("     {} {} {}{}{}", connector, icon, CYAN, entry.name, RESET);
        }
    }
    
    kprintln!("");
    kprintln!("  {}{}✓ All subsystems initialized successfully.{}", BOLD, GREEN, RESET);
    kprintln!("");

    // ---- Spawn demo kernel tasks ----
    boot_status(YELLOW, "TASK", "Spawning kernel worker threads...");
    spawn_demo_tasks();
    boot_ok(GREEN, "TASK", "Worker threads ready");
    kprintln!("");

    // Enable interrupts
    arch::aarch64::cpu::enable_interrupts();
    
    // Start the scheduler (enables preemption)
    process::scheduler::start();
    
    // Launch the user-space init process
    init::launch();
    
    // Launch the interactive shell
    shell::run();
}

/// Spawn demo kernel worker tasks for multitasking demonstration
fn spawn_demo_tasks() {
    use process::scheduler::{PRIORITY_BELOW_NORMAL, PRIORITY_LOWEST};
    
    // Worker task 1: simulates periodic background work
    process::spawn_kernel_task("worker1", worker_task_1, PRIORITY_BELOW_NORMAL);
    
    // Worker task 2: simulates low-priority housekeeping
    process::spawn_kernel_task("worker2", worker_task_2, PRIORITY_LOWEST);
}

/// Demo worker task 1 - periodic background worker
fn worker_task_1() -> ! {
    loop {
        // Do some simulated work
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        // Yield to other threads
        process::scheduler::yield_thread();
    }
}

/// Demo worker task 2 - low priority housekeeping
fn worker_task_2() -> ! {
    loop {
        // Do some simulated housekeeping
        for _ in 0..5000 {
            core::hint::spin_loop();
        }
        // Yield to other threads
        process::scheduler::yield_thread();
    }
}

/// Panic handler
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    arch::aarch64::cpu::disable_interrupts();
    
    kprintln!("");
    kprintln!("\x1b[1m\x1b[31m  ╔══════════════════════════════════════╗\x1b[0m");
    kprintln!("\x1b[1m\x1b[31m  ║         !!! KERNEL PANIC !!!        ║\x1b[0m");
    kprintln!("\x1b[1m\x1b[31m  ╚══════════════════════════════════════╝\x1b[0m");
    kprintln!("");
    kprintln!("\x1b[97m  {}\x1b[0m", info);
    kprintln!("");
    kprintln!("\x1b[2m  System halted.\x1b[0m");
    
    loop {
        arch::aarch64::cpu::halt();
    }
}

/// Boot status helpers
fn boot_status(color: &str, tag: &str, msg: &str) {
    kprintln!("  \x1b[2m[\x1b[0m{}{}\x1b[0m\x1b[2m]\x1b[0m {}", color, tag, msg);
}

fn boot_ok(color: &str, tag: &str, msg: &str) {
    kprintln!("  \x1b[2m[\x1b[0m{}{}\x1b[0m\x1b[2m]\x1b[0m \x1b[32m✓\x1b[0m {}", color, tag, msg);
}

/// Kernel print macro
#[macro_export]
macro_rules! kprint {
    ($($arg:tt)*) => {
        {
            let args = format_args!($($arg)*);
            $crate::hal::console::_print(args);
            $crate::hal::klog::log(format_args!($($arg)*));
        }
    };
}

/// Kernel println macro
#[macro_export]
macro_rules! kprintln {
    () => ($crate::kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)))
}
