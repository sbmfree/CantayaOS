# CantayaOS QEMU Runner
# ======================
# Launches CantayaOS in QEMU with UEFI firmware (OVMF).
#
# Prerequisites:
#   - QEMU installed and in PATH (qemu-system-x86_64)
#   - OVMF UEFI firmware (downloaded automatically or manually placed)
#
# Usage:
#   .\scripts\run-qemu.ps1            # Run normally
#   .\scripts\run-qemu.ps1 -Debug     # Run with GDB server for debugging
#   .\scripts\run-qemu.ps1 -Serial    # Show serial output in console
#
# The script mounts the ESP directory as a FAT filesystem, which QEMU
# presents to the UEFI firmware. The firmware then loads BOOTX64.EFI
# from the standard path.

param(
    [switch]$Debug,
    [switch]$Serial,
    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $ProjectRoot

# Build first (unless -NoBuild is specified)
if (-not $NoBuild) {
    Write-Host "Building CantayaOS..." -ForegroundColor Yellow
    & "$ProjectRoot\scripts\build.ps1"
    if ($LASTEXITCODE -ne 0) { throw "Build failed" }
}

# OVMF firmware — use local code.fd and vars.fd from project root
$OvmfCode = "$ProjectRoot\code.fd"
$OvmfVars = "$ProjectRoot\vars.fd"

if (-not (Test-Path $OvmfCode)) {
    throw "OVMF code.fd not found at $OvmfCode"
}
if (-not (Test-Path $OvmfVars)) {
    throw "OVMF vars.fd not found at $OvmfVars"
}

Write-Host "Using OVMF Code: $OvmfCode" -ForegroundColor Cyan
Write-Host "Using OVMF Vars: $OvmfVars" -ForegroundColor Cyan

# Build QEMU command
$QemuArgs = @(
    # Machine configuration
    "-machine", "q35",                    # Modern chipset (PCIe, AHCI)
    "-cpu", "qemu64",                     # 64-bit CPU
    "-m", "256M",                         # 256 MiB RAM

    # UEFI firmware (pflash for proper OVMF setup)
    "-drive", "if=pflash,format=raw,readonly=on,file=$OvmfCode",
    "-drive", "if=pflash,format=raw,file=$OvmfVars",

    # Mount our ESP directory as a FAT filesystem
    "-drive", "format=raw,file=fat:rw:target/esp",

    # Virtio-blk data disk (FAT32 formatted)
    "-drive", "file=$ProjectRoot\disk.img,if=none,id=drive0,format=raw",
    "-device", "virtio-blk-pci,drive=drive0",

    # Display
    "-vga", "std",                        # Standard VGA (GOP compatible)

    # Serial output to stdio (for kernel debug messages)
    "-serial", "stdio",

    # No default network
    "-net", "none"
)

# Debug mode: start GDB server and wait for connection
if ($Debug) {
    $QemuArgs += @("-s", "-S")  # -s = GDB on port 1234, -S = pause at start
    Write-Host "Debug mode: waiting for GDB connection on port 1234..." -ForegroundColor Yellow
}

# Additional serial output
if ($Serial) {
    Write-Host "Serial output enabled on stdio" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " Launching CantayaOS in QEMU" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Launch QEMU
$QemuCmd = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $QemuCmd)) {
    # Fallback to PATH
    $QemuCmd = "qemu-system-x86_64"
}
Write-Host "Command: $QemuCmd $($QemuArgs -join ' ')" -ForegroundColor DarkGray
& $QemuCmd @QemuArgs

Pop-Location
