#!/bin/bash
# Run CantayaOS in QEMU

KERNEL="${1:-target/aarch64-cantaya/release/cantaya_kernel}"
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
echo "  Kernel: $KERNEL"
echo "  RAM:    $RAM"
echo "  CPU:    cortex-a72"
MODE="${3:-window}"

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
