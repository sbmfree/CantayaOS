#!/bin/bash
# CantayaOS Build Script

set -e

# Architecture selection
ARCH="${ARCH:-aarch64}"  # Default to aarch64, can be overridden with ARCH=x86_64

if [ "$ARCH" == "x86_64" ]; then
    TARGET="x86_64-cantaya"
    USERSPACE_TARGET="x86_64-unknown-none"
else
    TARGET="aarch64-cantaya"
    USERSPACE_TARGET="aarch64-unknown-none"
fi

MODE="${1:-release}"

echo "=== Building CantayaOS ==="
echo "Architecture: $ARCH"
echo "Mode: $MODE"
echo ""

# Ensure nightly toolchain
if ! rustup show active-toolchain 2>/dev/null | grep -q "nightly"; then
    echo "Switching to nightly toolchain..."
    rustup default nightly
fi

# Install required components
rustup component add rust-src llvm-tools-preview 2>/dev/null || true
rustup target add $USERSPACE_TARGET 2>/dev/null || true

# -----------------------------------------------------------------------
# Step 1: Build userspace programs
# -----------------------------------------------------------------------
echo "Building userspace programs..."

pushd userspace > /dev/null

if [ "$MODE" == "release" ]; then
    cargo build --release -Z build-std=core,compiler_builtins --target $USERSPACE_TARGET 2>&1
    USPACE_DIR="target/$USERSPACE_TARGET/release"
else
    cargo build -Z build-std=core,compiler_builtins --target $USERSPACE_TARGET 2>&1
    USPACE_DIR="target/$USERSPACE_TARGET/debug"
fi

echo "Userspace binaries:"
for BIN in cantaya_init hello shell_hello echo cat draw http_get; do
    if [ -f "$USPACE_DIR/$BIN" ]; then
        SIZE=$(stat -c%s "$USPACE_DIR/$BIN" 2>/dev/null || stat -f%z "$USPACE_DIR/$BIN" 2>/dev/null || echo "?")
        echo "  $BIN: $SIZE bytes"
    else
        echo "  WARNING: $BIN not found!"
    fi
done

popd > /dev/null
echo ""

# -----------------------------------------------------------------------
# Step 2: Build kernel (embeds userspace ELFs via include_bytes!)
# -----------------------------------------------------------------------
echo "Building kernel..."

if [ "$MODE" == "release" ]; then
    cargo build --release --target "kernel/$TARGET.json" 2>&1
    KERNEL_ELF="target/$TARGET/release/cantaya_kernel"
else
    cargo build --target "kernel/$TARGET.json" 2>&1
    KERNEL_ELF="target/$TARGET/debug/cantaya_kernel"
fi

if [ ! -f "$KERNEL_ELF" ]; then
    echo "ERROR: Kernel ELF not found at $KERNEL_ELF"
    exit 1
fi

# -----------------------------------------------------------------------
# Step 3: Create flat binary from ELF
# -----------------------------------------------------------------------
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
