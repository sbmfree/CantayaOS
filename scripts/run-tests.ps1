# CantayaOS Test Runner
# =====================
# Runs kernel unit tests in QEMU with the ISA debug-exit device.
#
# The kernel detects the `test` boot command-line argument (or the
# presence of the debug-exit device) and runs built-in tests instead
# of booting normally.
#
# Exit codes:
#   33 (0x21) = All tests passed
#   35 (0x23) = One or more tests failed
#   Other     = Kernel crashed / QEMU error
#
# Usage:
#   .\scripts\run-tests.ps1

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Push-Location $ProjectRoot

# Build first
Write-Host "Building CantayaOS for testing..." -ForegroundColor Yellow
& "$ProjectRoot\scripts\build.ps1"
if ($LASTEXITCODE -ne 0) { throw "Build failed" }

# OVMF firmware
$OvmfCode = "$ProjectRoot\code.fd"
$OvmfVars = "$ProjectRoot\vars.fd"

if (-not (Test-Path $OvmfCode)) {
    throw "OVMF code.fd not found at $OvmfCode"
}
if (-not (Test-Path $OvmfVars)) {
    throw "OVMF vars.fd not found at $OvmfVars"
}

# Build QEMU command — same as run-qemu.ps1 but with:
#   - ISA debug-exit device for test result reporting
#   - -display none (headless)
#   - -no-reboot (don't restart on triple fault)
$QemuArgs = @(
    "-machine", "q35",
    "-cpu", "qemu64",
    "-m", "256M",

    # UEFI firmware
    "-drive", "if=pflash,format=raw,readonly=on,file=$OvmfCode",
    "-drive", "if=pflash,format=raw,file=$OvmfVars",

    # ESP
    "-drive", "format=raw,file=fat:rw:target/esp",

    # Virtio-blk data disk
    "-drive", "file=$ProjectRoot\disk.img,if=none,id=drive0,format=raw",
    "-device", "virtio-blk-pci,drive=drive0",

    # ISA debug-exit device — allows kernel to signal test results
    "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04",

    # Headless — no GUI window
    "-display", "none",

    # Serial to stdio — test output appears in console
    "-serial", "stdio",

    # Don't reboot on crash
    "-no-reboot",

    # No network
    "-net", "none"
)

Write-Host ""
Write-Host "========================================" -ForegroundColor Cyan
Write-Host " CantayaOS Test Runner" -ForegroundColor Cyan
Write-Host "========================================" -ForegroundColor Cyan
Write-Host ""

# Launch QEMU
$QemuCmd = "C:\Program Files\qemu\qemu-system-x86_64.exe"
if (-not (Test-Path $QemuCmd)) {
    $QemuCmd = "qemu-system-x86_64"
}

& $QemuCmd @QemuArgs
$ExitCode = $LASTEXITCODE

Write-Host ""

switch ($ExitCode) {
    33 {
        Write-Host "========================================" -ForegroundColor Green
        Write-Host " ALL TESTS PASSED" -ForegroundColor Green
        Write-Host "========================================" -ForegroundColor Green
        $host.SetShouldExit(0)
    }
    35 {
        Write-Host "========================================" -ForegroundColor Red
        Write-Host " TESTS FAILED" -ForegroundColor Red
        Write-Host "========================================" -ForegroundColor Red
        $host.SetShouldExit(1)
    }
    default {
        Write-Host "========================================" -ForegroundColor Red
        Write-Host " UNEXPECTED EXIT CODE: $ExitCode" -ForegroundColor Red
        Write-Host " (Kernel may have crashed)" -ForegroundColor Red
        Write-Host "========================================" -ForegroundColor Red
        $host.SetShouldExit(2)
    }
}

Pop-Location
