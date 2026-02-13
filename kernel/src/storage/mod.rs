// Storage Subsystem
//
// This module provides the storage stack for CantayaOS:
//   - Block device abstraction (trait-based)
//   - FAT32 filesystem driver (read-write)
//   - Virtual Filesystem (VFS) layer with path-based operations
//
// Architecture:
//
//   ┌────────────────────────────┐
//   │   Shell / Applications     │
//   ├────────────────────────────┤
//   │   VFS (path resolution)    │
//   ├────────────────────────────┤
//   │   FAT32 Driver             │
//   ├────────────────────────────┤
//   │   Block Device Abstraction │
//   ├────────────────────────────┤
//   │   virtio-blk HAL Driver    │
//   └────────────────────────────┘

pub mod block;
pub mod fat32;
pub mod vfs;

/// Initialize the storage subsystem.
///
/// Discovers and initializes disk devices, mounts the root filesystem.
pub fn init() {
    log::info!("storage: initializing storage subsystem...");

    // Step 1: Initialize the virtio-blk driver
    if !crate::hal::virtio_blk::init() {
        log::info!("storage: no block device available — filesystem disabled");
        return;
    }

    // Step 2: Initialize the block device wrapper
    block::init();

    // Step 3: Mount the FAT32 filesystem
    match fat32::init() {
        Ok(()) => {
            log::info!("storage: FAT32 filesystem mounted");
        }
        Err(e) => {
            log::error!("storage: FAT32 mount failed: {}", e);
            return;
        }
    }

    // Step 4: Initialize VFS with FAT32 as root
    vfs::init();
    log::info!("storage: VFS initialized — filesystem ready");

    // Step 5: Provision Windows-like directory structure on first boot
    provision_filesystem();
}

// ============================================================================
// Windows-like Filesystem Provisioning
// ============================================================================

/// Create a Windows-like directory hierarchy on first boot.
///
/// Layout mirrors a typical Windows installation:
///
///   C:\
///   ├── Windows\              → /Windows
///   │   ├── System32\         → /Windows/System32
///   │   │   ├── Drivers\      → /Windows/System32/Drivers
///   │   │   └── Config\       → /Windows/System32/Config
///   │   ├── Fonts\            → /Windows/Fonts
///   │   ├── Temp\             → /Windows/Temp
///   │   └── Logs\             → /Windows/Logs
///   ├── Programs\             → /Programs  (≈ Program Files)
///   ├── PerfLogs\             → /PerfLogs
///   ├── Users\                → /Users
///   │   ├── Root\             → /Users/Root
///   │   │   ├── Desktop\      → /Users/Root/Desktop
///   │   │   ├── Docs\         → /Users/Root/Docs
///   │   │   ├── Download\     → /Users/Root/Download
///   │   │   ├── Music\        → /Users/Root/Music
///   │   │   ├── Pictures\     → /Users/Root/Pictures
///   │   │   └── Videos\       → /Users/Root/Videos
///   │   └── Public\           → /Users/Public
///   │       └── Desktop\      → /Users/Public/Desktop
///   └── system\               → /system  (legacy compat)
///
fn provision_filesystem() {
    // Check if already provisioned by looking for the marker file
    if vfs::exists("/Windows/System32/Config/init.ok") {
        log::info!("storage: filesystem already provisioned");
        // Migrate legacy configs if present
        migrate_legacy_configs();
        return;
    }

    log::info!("storage: provisioning Windows-like filesystem...");

    // ── Directory tree ──────────────────────────────────────────────────

    let dirs: &[&str] = &[
        // Windows system directories
        "/Windows",
        "/Windows/System32",
        "/Windows/System32/Drivers",
        "/Windows/System32/Config",
        "/Windows/Fonts",
        "/Windows/Temp",
        "/Windows/Logs",
        // Program Files equivalent
        "/Programs",
        // Performance logs
        "/PerfLogs",
        // User profiles
        "/Users",
        "/Users/Root",
        "/Users/Root/Desktop",
        "/Users/Root/Docs",
        "/Users/Root/Download",
        "/Users/Root/Music",
        "/Users/Root/Pictures",
        "/Users/Root/Videos",
        "/Users/Public",
        "/Users/Public/Desktop",
        // Legacy /system dir for backward compat
        "/system",
    ];

    let mut created = 0u32;
    for dir in dirs {
        if !vfs::exists(dir) {
            if vfs::mkdir(dir) {
                created += 1;
            } else {
                log::warn!("storage: failed to create directory {}", dir);
            }
        }
    }

    log::info!("storage: created {} directories", created);

    // ── System files ────────────────────────────────────────────────────

    // Version info  (like C:\Windows\version.txt)
    let version = concat!(
        "CantayaOS v", env!("CARGO_PKG_VERSION"), "\r\n",
        "Architecture: x86_64\r\n",
        "Kernel: Hybrid Monolithic\r\n",
        "Filesystem: FAT32\r\n",
        "Boot: UEFI\r\n",
    );
    vfs::write_file("/Windows/version.txt", version.as_bytes());

    // System configuration
    let sys_cfg = concat!(
        "# CantayaOS System Configuration\r\n",
        "# Located at C:\\Windows\\System32\\Config\\system.cfg\r\n",
        "\r\n",
        "[system]\r\n",
        "os_name=CantayaOS\r\n",
        "arch=x86_64\r\n",
        "shell=csh\r\n",
        "default_user=Root\r\n",
        "\r\n",
        "[display]\r\n",
        "resolution=auto\r\n",
        "theme=default\r\n",
        "\r\n",
        "[storage]\r\n",
        "filesystem=FAT32\r\n",
        "volume=CANTAYAOS\r\n",
    );
    vfs::write_file("/Windows/System32/Config/system.cfg", sys_cfg.as_bytes());

    // Default hostname configuration
    vfs::write_file("/Windows/System32/Config/hostname.cfg", b"CantayaOS");

    // Default autoexec script
    let autoexec = concat!(
        "# CantayaOS Startup Script\r\n",
        "# Located at C:\\Windows\\System32\\Config\\autoexec.cfg\r\n",
        "# Lines starting with # are comments\r\n",
    );
    vfs::write_file("/Windows/System32/Config/autoexec.cfg", autoexec.as_bytes());

    // Legacy copies for backward compatibility
    vfs::write_file("/system/hostname.cfg", b"CantayaOS");
    vfs::write_file("/system/autoexec.cfg", autoexec.as_bytes());

    // Driver registry  (like C:\Windows\System32\Drivers\drivers.cfg)
    let drivers = concat!(
        "# CantayaOS Driver Registry\r\n",
        "\r\n",
        "[keyboard]\r\n",
        "driver=ps2_kbd\r\n",
        "irq=1\r\n",
        "status=active\r\n",
        "\r\n",
        "[mouse]\r\n",
        "driver=ps2_mouse\r\n",
        "irq=12\r\n",
        "status=active\r\n",
        "\r\n",
        "[timer]\r\n",
        "driver=pit8253\r\n",
        "irq=0\r\n",
        "freq=1000\r\n",
        "status=active\r\n",
        "\r\n",
        "[storage]\r\n",
        "driver=virtio_blk\r\n",
        "type=PCI\r\n",
        "status=active\r\n",
        "\r\n",
        "[speaker]\r\n",
        "driver=pc_speaker\r\n",
        "io=0x61\r\n",
        "status=active\r\n",
    );
    vfs::write_file("/Windows/System32/Drivers/drivers.cfg", drivers.as_bytes());

    // Boot log  (like C:\Windows\Logs\boot.log)
    let boot_log = concat!(
        "CantayaOS Boot Log\r\n",
        "==================\r\n",
        "UEFI firmware initialized\r\n",
        "Kernel loaded at 0xFFFFFFFF80000000\r\n",
        "GDT/IDT configured\r\n",
        "Memory manager initialized\r\n",
        "PIT timer started (1000 Hz)\r\n",
        "PS/2 keyboard & mouse initialized\r\n",
        "PCI bus enumerated\r\n",
        "VirtIO-blk storage initialized\r\n",
        "FAT32 filesystem mounted\r\n",
        "VFS initialized\r\n",
        "Filesystem provisioned\r\n",
        "Shell ready\r\n",
    );
    vfs::write_file("/Windows/Logs/boot.log", boot_log.as_bytes());

    // User profile readme (like C:\Users\Root\Desktop\readme.txt)
    let readme = concat!(
        "Welcome to CantayaOS!\r\n",
        "=====================\r\n",
        "\r\n",
        "Your home directory is C:\\Users\\Root\r\n",
        "\r\n",
        "Quick Start:\r\n",
        "  help       - List all commands\r\n",
        "  dir / ls   - List files and folders\r\n",
        "  cd <dir>   - Change directory\r\n",
        "  type <f>   - Display file contents\r\n",
        "  edit <f>   - Open built-in text editor\r\n",
        "  sysinfo    - System information\r\n",
        "  desktop    - Switch to graphical desktop\r\n",
        "  shutdown   - Power off the system\r\n",
        "\r\n",
        "Filesystem Layout:\r\n",
        "  C:\\Windows\\         - System files\r\n",
        "  C:\\Programs\\        - Installed programs\r\n",
        "  C:\\Users\\Root\\      - Your profile\r\n",
        "  C:\\Users\\Public\\    - Shared files\r\n",
    );
    vfs::write_file("/Users/Root/Desktop/readme.txt", readme.as_bytes());

    // NTUSER.DAT equivalent — user preferences
    let user_cfg = concat!(
        "# User Profile Configuration\r\n",
        "# C:\\Users\\Root\\ntuser.cfg\r\n",
        "\r\n",
        "[user]\r\n",
        "name=Root\r\n",
        "shell=csh\r\n",
        "home=C:\\Users\\Root\r\n",
        "\r\n",
        "[preferences]\r\n",
        "theme=default\r\n",
        "prompt_style=windows\r\n",
        "color_scheme=default\r\n",
    );
    vfs::write_file("/Users/Root/ntuser.cfg", user_cfg.as_bytes());

    // Provisioning complete marker
    vfs::write_file("/Windows/System32/Config/init.ok", b"OK");

    log::info!("storage: Windows-like filesystem provisioned successfully");
}

/// Migrate legacy /system/ configs to the new Windows-like paths.
fn migrate_legacy_configs() {
    // If legacy hostname exists but the Windows one doesn't, copy it
    if vfs::exists("/system/hostname.cfg") && !vfs::exists("/Windows/System32/Config/hostname.cfg") {
        if let Some(data) = vfs::read_file("/system/hostname.cfg") {
            vfs::write_file("/Windows/System32/Config/hostname.cfg", &data);
            log::info!("storage: migrated hostname.cfg to Windows path");
        }
    }
    // If legacy autoexec exists but the Windows one doesn't, copy it
    if vfs::exists("/system/autoexec.cfg") && !vfs::exists("/Windows/System32/Config/autoexec.cfg") {
        if let Some(data) = vfs::read_file("/system/autoexec.cfg") {
            vfs::write_file("/Windows/System32/Config/autoexec.cfg", &data);
            log::info!("storage: migrated autoexec.cfg to Windows path");
        }
    }
}
