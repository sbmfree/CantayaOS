#!/bin/bash
# CantayaOS Build Script

set -e

TARGET="aarch64-cantaya"
MODE="${1:-release}"

echo "=== Building CantayaOS ==="
echo "Mode: $MODE"
echo ""

# Ensure nightly toolchain
if ! rustup show active-toolchain 2>/dev/null | grep -q "nightly"; then
    echo "Switching to nightly toolchain..."
    rustup default nightly
fi

# Install required components
rustup component add rust-src llvm-tools-preview 2>/dev/null || true

# Build from workspace root (config in .cargo/config.toml)
echo "Building kernel..."

if [ "$MODE" == "release" ]; then
    cargo build --release 2>&1
    KERNEL_ELF="target/$TARGET/release/cantaya_kernel"
else
    cargo build 2>&1
    KERNEL_ELF="target/$TARGET/debug/cantaya_kernel"
fi

if [ ! -f "$KERNEL_ELF" ]; then
    echo "ERROR: Kernel ELF not found at $KERNEL_ELF"
    exit 1
fi

# Create flat binary from ELF
echo ""
echo "Creating kernel binary image..."

# Find objcopy (prefer the one from rustup's llvm-tools)
OBJCOPY=""
LLVM_TOOLS_PATH=$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | grep host | awk '{print $2}')/bin
if [ -x "$LLVM_TOOLS_PATH/llvm-objcopy" ]; then
    OBJCOPY="$LLVM_TOOLS_PATH/llvm-objcopy"
elif command -v rust-objcopy &> /dev/null; then
    OBJCOPY="rust-objcopy"
elif command -v llvm-objcopy &> /dev/null; then
    OBJCOPY="llvm-objcopy"
elif command -v aarch64-linux-gnu-objcopy &> /dev/null; then
    OBJCOPY="aarch64-linux-gnu-objcopy"
elif command -v objcopy &> /dev/null; then
    OBJCOPY="objcopy"
fi

if [ -n "$OBJCOPY" ]; then
    $OBJCOPY -O binary "$KERNEL_ELF" cantaya.bin
    echo "Used: $OBJCOPY"
else
    echo "Warning: No objcopy found. Using ELF directly with QEMU."
    cp "$KERNEL_ELF" cantaya.bin
fi

echo ""
echo "=== Build Complete ==="
echo "Kernel ELF: $KERNEL_ELF"
if [ -f cantaya.bin ]; then
    SIZE=$(stat -c%s cantaya.bin 2>/dev/null || stat -f%z cantaya.bin 2>/dev/null || echo "?")
    echo "Kernel BIN: cantaya.bin ($SIZE bytes)"
fi
