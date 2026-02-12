#!/bin/bash
# Run CantayaOS in QEMU

# Architecture selection
ARCH="${ARCH:-aarch64}"  # Default to aarch64, can be overridden with ARCH=x86_64

if [ "$ARCH" == "x86_64" ]; then
    DEFAULT_KERNEL="target/x86_64-cantaya/release/cantaya_kernel"
else
    DEFAULT_KERNEL="target/aarch64-cantaya/release/cantaya_kernel"
fi

KERNEL="${1:-$DEFAULT_KERNEL}"
RAM="${2:-2G}"

if [ ! -f "$KERNEL" ]; then
    echo "Kernel not found: $KERNEL"
    echo "Run ./build.sh first"
    exit 1
fi

# Create disk image if it doesn't exist
if [ ! -f "disk.img" ]; then
    echo "Creating 64MB FAT32 disk image..."
    dd if=/dev/zero of=disk.img bs=1M count=64 status=none
    mkfs.fat -F 32 disk.img > /dev/null 2>&1
    echo "disk.img created"
fi

echo "Starting CantayaOS in QEMU..."
echo "  Architecture: $ARCH"
echo "  Kernel: $KERNEL"
echo "  RAM:    $RAM"
MODE="${3:-window}"

if [ "$ARCH" == "x86_64" ]; then
    echo "  CPU:    qemu64"
    if [ "$MODE" = "window" ]; then
        echo "  Display: QEMU window (graphical + serial)"
        echo "  Close the window to exit QEMU"
        echo ""
        qemu-system-x86_64 \
            -cpu qemu64 \
            -m "$RAM" \
            -kernel "$KERNEL" \
            -device VGA \
            -device virtio-keyboard-pci \
            -device virtio-tablet-pci \
            -device virtio-net-pci,netdev=net0 \
            -netdev user,id=net0 \
            -drive file=disk.img,if=none,format=raw,id=hd0 \
            -device virtio-blk-pci,drive=hd0 \
            -serial vc \
            -d guest_errors
    else
        echo "  Press Ctrl-A X to exit QEMU"
        echo ""
        qemu-system-x86_64 \
            -cpu qemu64 \
            -m "$RAM" \
            -kernel "$KERNEL" \
            -device VGA \
            -device virtio-keyboard-pci \
            -device virtio-tablet-pci \
            -device virtio-net-pci,netdev=net0 \
            -netdev user,id=net0 \
            -drive file=disk.img,if=none,format=raw,id=hd0 \
            -device virtio-blk-pci,drive=hd0 \
            -serial mon:stdio \
            -d guest_errors
    fi
else
    echo "  CPU:    cortex-a72"
    if [ "$MODE" = "window" ]; then
        echo "  Display: QEMU window (graphical + serial)"
        echo "  Close the window to exit QEMU"
        echo ""
        qemu-system-aarch64 \
            -M virt,gic-version=3 \
            -cpu cortex-a72 \
            -m "$RAM" \
            -kernel "$KERNEL" \
            -device ramfb \
            -device virtio-keyboard-device \
            -device virtio-tablet-device \
            -device virtio-net-device,netdev=net0 \
            -netdev user,id=net0 \
            -drive file=disk.img,if=none,format=raw,id=hd0 \
            -device virtio-blk-device,drive=hd0 \
            -serial vc \
            -d guest_errors
    else
        echo "  Press Ctrl-A X to exit QEMU"
        echo ""
        qemu-system-aarch64 \
            -M virt,gic-version=3 \
            -cpu cortex-a72 \
            -m "$RAM" \
            -kernel "$KERNEL" \
            -device ramfb \
            -device virtio-keyboard-device \
            -device virtio-tablet-device \
            -device virtio-net-device,netdev=net0 \
            -netdev user,id=net0 \
            -drive file=disk.img,if=none,format=raw,id=hd0 \
            -device virtio-blk-device,drive=hd0 \
            -serial mon:stdio \
            -d guest_errors
    fi
fi
