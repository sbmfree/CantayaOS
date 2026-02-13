# CantayaOS Disk Image Creator
# =============================
# Creates a 32 MiB raw disk image formatted as FAT32 for use with virtio-blk.
#
# Usage:
#   .\scripts\create-disk.ps1           # Create disk.img in project root
#   .\scripts\create-disk.ps1 -Force    # Overwrite existing disk image

param(
    [switch]$Force
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$DiskImage = Join-Path $ProjectRoot "disk.img"
$SizeMB = 32

if ((Test-Path $DiskImage) -and -not $Force) {
    Write-Host "Disk image already exists: $DiskImage" -ForegroundColor Yellow
    Write-Host "Use -Force to overwrite." -ForegroundColor Yellow
    exit 0
}

Write-Host "Creating ${SizeMB} MiB FAT32 disk image..." -ForegroundColor Cyan

# Create a raw disk image filled with zeros
$SizeBytes = $SizeMB * 1024 * 1024
$buf = New-Object byte[] (1024 * 1024)  # 1 MiB buffer
$stream = [System.IO.File]::Create($DiskImage)
try {
    for ($i = 0; $i -lt $SizeMB; $i++) {
        $stream.Write($buf, 0, $buf.Length)
    }
} finally {
    $stream.Close()
}

Write-Host "Raw image created: $DiskImage ($SizeBytes bytes)" -ForegroundColor Green

# Format using qemu-img + mkfs if available, otherwise write FAT32 BPB manually
# We'll write a minimal FAT32 boot sector + FAT tables directly

Write-Host "Writing FAT32 filesystem structures..." -ForegroundColor Cyan

# FAT32 parameters for 32 MiB disk:
# - 512 bytes/sector
# - 8 sectors/cluster (4 KiB clusters)
# - 32 reserved sectors
# - 2 FATs
# - Total sectors = 32 * 1024 * 1024 / 512 = 65536
# - Data sectors = 65536 - 32 - (2 * FAT_size)
# - Clusters needed ≈ (65536 - 32) / 8 ≈ 8188
# - FAT entries needed = 8188 → FAT size = ceil(8188 * 4 / 512) = 65 sectors
# - Actual: let's use 64 sectors per FAT for simplicity (covers 8192 entries)

$TotalSectors = $SizeBytes / 512          # 65536
$BytesPerSector = 512
$SectorsPerCluster = 8                    # 4 KiB clusters
$ReservedSectors = 32
$NumFATs = 2
$SectorsPerFAT = 64                       # Enough for 8192 clusters
$RootCluster = 2                          # First data cluster

$DataStartSector = $ReservedSectors + ($NumFATs * $SectorsPerFAT)  # 32 + 128 = 160
$DataSectors = $TotalSectors - $DataStartSector                     # 65376
$TotalClusters = [math]::Floor($DataSectors / $SectorsPerCluster)  # 8172

Write-Host "  Total sectors:    $TotalSectors"
Write-Host "  Reserved sectors: $ReservedSectors"
Write-Host "  Sectors per FAT:  $SectorsPerFAT"
Write-Host "  Data start:       sector $DataStartSector"
Write-Host "  Total clusters:   $TotalClusters"
Write-Host "  Cluster size:     $($SectorsPerCluster * $BytesPerSector) bytes"

# Open the file for binary writing
$stream = [System.IO.File]::Open($DiskImage, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite)
$writer = New-Object System.IO.BinaryWriter($stream)

try {
    # ---- Boot Sector (sector 0) ----
    $stream.Position = 0

    # Jump instruction (3 bytes)
    $writer.Write([byte]0xEB)  # JMP short
    $writer.Write([byte]0x58)  # offset to boot code
    $writer.Write([byte]0x90)  # NOP

    # OEM Name (8 bytes)
    $oem = [System.Text.Encoding]::ASCII.GetBytes("CANTAYOS")
    $writer.Write($oem)

    # BPB (BIOS Parameter Block)
    $writer.Write([uint16]$BytesPerSector)          # Bytes per sector
    $writer.Write([byte]$SectorsPerCluster)          # Sectors per cluster
    $writer.Write([uint16]$ReservedSectors)          # Reserved sectors
    $writer.Write([byte]$NumFATs)                    # Number of FATs
    $writer.Write([uint16]0)                         # Root entry count (0 for FAT32)
    $writer.Write([uint16]0)                         # Total sectors 16-bit (0 for FAT32)
    $writer.Write([byte]0xF8)                        # Media type (fixed disk)
    $writer.Write([uint16]0)                         # Sectors per FAT (16-bit, 0 for FAT32)
    $writer.Write([uint16]63)                        # Sectors per track
    $writer.Write([uint16]255)                       # Number of heads
    $writer.Write([uint32]0)                         # Hidden sectors
    $writer.Write([uint32]$TotalSectors)             # Total sectors 32-bit

    # FAT32-specific BPB
    $writer.Write([uint32]$SectorsPerFAT)            # Sectors per FAT (32-bit)
    $writer.Write([uint16]0)                         # Extended flags
    $writer.Write([uint16]0)                         # FS Version
    $writer.Write([uint32]$RootCluster)              # Root directory cluster
    $writer.Write([uint16]1)                         # FSInfo sector
    $writer.Write([uint16]6)                         # Backup boot sector
    # Reserved (12 bytes)
    for ($i = 0; $i -lt 12; $i++) { $writer.Write([byte]0) }

    $writer.Write([byte]0x80)                        # Drive number
    $writer.Write([byte]0)                           # Reserved
    $writer.Write([byte]0x29)                        # Boot signature
    $writer.Write([uint32]0x12345678)                # Volume serial number
    $volLabel = [System.Text.Encoding]::ASCII.GetBytes("CANTAYAOS  ")  # 11 bytes
    $writer.Write($volLabel)
    $fsType = [System.Text.Encoding]::ASCII.GetBytes("FAT32   ")       # 8 bytes
    $writer.Write($fsType)

    # Boot code area (fill to offset 510)
    $currentPos = $stream.Position
    $remaining = 510 - $currentPos
    for ($i = 0; $i -lt $remaining; $i++) { $writer.Write([byte]0) }

    # Boot signature
    $writer.Write([byte]0x55)
    $writer.Write([byte]0xAA)

    # ---- FSInfo Sector (sector 1) ----
    $stream.Position = 512

    $writer.Write([uint32]0x41615252)  # FSInfo signature 1
    # Reserved (480 bytes)
    for ($i = 0; $i -lt 480; $i++) { $writer.Write([byte]0) }
    $writer.Write([uint32]0x61417272)  # FSInfo signature 2
    $writer.Write([uint32]($TotalClusters - 1))  # Free cluster count
    $writer.Write([uint32]3)                      # Next free cluster hint
    # Reserved (12 bytes)
    for ($i = 0; $i -lt 12; $i++) { $writer.Write([byte]0) }
    # FSInfo trail signature: 0xAA550000
    $writer.Write([byte]0x00)
    $writer.Write([byte]0x00)
    $writer.Write([byte]0x55)
    $writer.Write([byte]0xAA)

    # ---- Backup boot sector (sector 6) ----
    # Copy sector 0 to sector 6
    $stream.Position = 0
    $bootSector = New-Object byte[] 512
    $stream.Read($bootSector, 0, 512) | Out-Null
    $stream.Position = 6 * 512
    $writer.Write($bootSector)

    # ---- FAT 1 (starts at sector 32) ----
    $fat1Start = $ReservedSectors * $BytesPerSector
    $stream.Position = $fat1Start

    # Entry 0: Media type marker
    $writer.Write([uint32]0x0FFFFFF8)
    # Entry 1: End-of-chain marker
    $writer.Write([uint32]0x0FFFFFFF)
    # Entry 2: Root directory (end-of-chain, single cluster)
    $writer.Write([uint32]0x0FFFFFFF)

    # ---- FAT 2 (mirror of FAT 1) ----
    $fat2Start = ($ReservedSectors + $SectorsPerFAT) * $BytesPerSector
    $stream.Position = $fat2Start

    $writer.Write([uint32]0x0FFFFFF8)
    $writer.Write([uint32]0x0FFFFFFF)
    $writer.Write([uint32]0x0FFFFFFF)

    # ---- Root Directory (cluster 2, at sector 160) ----
    $rootDirStart = $DataStartSector * $BytesPerSector
    $stream.Position = $rootDirStart

    # Volume label directory entry
    $label = [System.Text.Encoding]::ASCII.GetBytes("CANTAYAOS  ")  # 11 bytes
    $writer.Write($label)
    $writer.Write([byte]0x08)  # Attribute: Volume Label
    $writer.Write([byte]0)     # Reserved
    $writer.Write([byte]0)     # Create time (tenths)
    $writer.Write([uint16]0)   # Create time
    $writer.Write([uint16]0)   # Create date
    $writer.Write([uint16]0)   # Access date
    $writer.Write([uint16]0)   # First cluster high
    $writer.Write([uint16]0)   # Write time
    $writer.Write([uint16]0)   # Write date
    $writer.Write([uint16]0)   # First cluster low
    $writer.Write([uint32]0)   # File size

    Write-Host "FAT32 filesystem written successfully!" -ForegroundColor Green

} finally {
    $writer.Close()
    $stream.Close()
}

Write-Host ""
Write-Host "Disk image ready: $DiskImage" -ForegroundColor Green
Write-Host "Mount in QEMU with: -drive file=disk.img,if=none,id=drive0,format=raw -device virtio-blk-pci,drive=drive0" -ForegroundColor DarkGray
