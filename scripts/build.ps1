# CantayaOS Build Script
# =======================
# This script builds all components and creates a bootable UEFI disk image.
#
# Usage:
#   .\scripts\build.ps1          # Build everything
#   .\scripts\build.ps1 -Release # Build in release mode
#
# Output:
#   target/esp/               — The EFI System Partition structure
#   target/esp/EFI/BOOT/      — Bootloader .efi file
#   target/esp/cantaya/       — Kernel ELF file

param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $ProjectRoot

$BuildMode = if ($Release) { "--release" } else { "" }
$TargetDir = if ($Release) { "release" } else { "debug" }

Write-Host "========================================" -ForegroundColor Cyan
Write-Host " CantayaOS Build System" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Step 1: Build the shared crate (compiled as part of other crates, but good to check)
Write-Host "[1/4] Checking shared crate..." -ForegroundColor Yellow
cargo check -p cantaya_shared
if ($LASTEXITCODE -ne 0) { throw "Shared crate check failed" }
Write-Host "  OK" -ForegroundColor Green

# Step 2: Build the bootloader (UEFI target)
Write-Host "[2/4] Building bootloader (x86_64-unknown-uefi)..." -ForegroundColor Yellow
cargo build -p cantaya_bootloader --target x86_64-unknown-uefi $BuildMode
if ($LASTEXITCODE -ne 0) { throw "Bootloader build failed" }
Write-Host "  OK" -ForegroundColor Green

# Step 3: Build the kernel (custom bare-metal target)
Write-Host "[3/4] Building kernel (x86_64-unknown-none)..." -ForegroundColor Yellow
cargo build -p cantaya_kernel --target x86_64-unknown-none "-Zbuild-std=core,alloc,compiler_builtins" "-Zbuild-std-features=compiler-builtins-mem" $BuildMode
if ($LASTEXITCODE -ne 0) { throw "Kernel build failed" }
Write-Host "  OK" -ForegroundColor Green

# Step 4: Create the ESP (EFI System Partition) directory structure
Write-Host "[4/4] Creating ESP directory structure..." -ForegroundColor Yellow

$EspDir = "target/esp"
$BootDir = "$EspDir/EFI/BOOT"
$KernelDir = "$EspDir/cantaya"

New-Item -ItemType Directory -Force -Path $BootDir | Out-Null
New-Item -ItemType Directory -Force -Path $KernelDir | Out-Null

# Copy the bootloader .efi to the standard UEFI boot path
$BootloaderSrc = "target/x86_64-unknown-uefi/$TargetDir/cantaya_bootloader.efi"
Copy-Item $BootloaderSrc "$BootDir/BOOTX64.EFI" -Force

# Copy the kernel ELF (note: custom target puts it in a slightly different path)
$KernelSrc = "target/x86_64-unknown-none/$TargetDir/cantaya_kernel"
if (Test-Path $KernelSrc) {
    Copy-Item $KernelSrc "$KernelDir/kernel.elf" -Force
} else {
    # Try with .exe extension (some Rust versions append it)
    $KernelSrc = "target/x86_64-unknown-none/$TargetDir/cantaya_kernel.exe"
    if (Test-Path $KernelSrc) {
        Copy-Item $KernelSrc "$KernelDir/kernel.elf" -Force
    } else {
        throw "Kernel binary not found at expected path"
    }
}

Write-Host "  OK" -ForegroundColor Green

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " Build complete!" -ForegroundColor Green
Write-Host " ESP directory: $EspDir" -ForegroundColor White
Write-Host " Bootloader:    $BootDir/BOOTX64.EFI" -ForegroundColor White
Write-Host " Kernel:        $KernelDir/kernel.elf" -ForegroundColor White
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "To run in QEMU: .\scripts\run-qemu.ps1" -ForegroundColor Yellow

Pop-Location
