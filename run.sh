#!/bin/bash
# Run CantayaOS in QEMU

KERNEL="${1:-target/aarch64-cantaya/release/cantaya_kernel}"
RAM="${2:-128M}"

if [ ! -f "$KERNEL" ]; then
    echo "Kernel not found: $KERNEL"
    echo "Run ./build.sh first"
    exit 1
fi

echo "Starting CantayaOS in QEMU..."
echo "  Kernel: $KERNEL"
echo "  RAM:    $RAM"
echo "  CPU:    cortex-a72"
MODE="${3:-window}"

if [ "$MODE" = "window" ]; then
    echo "  Display: QEMU window (serial console)"
    echo "  Close the window to exit QEMU"
    echo ""
    qemu-system-aarch64 \
        -M virt,gic-version=3 \
        -cpu cortex-a72 \
        -m "$RAM" \
        -kernel "$KERNEL" \
        -serial vc \
        -d guest_errors
else
    echo "  Press Ctrl-A X to exit QEMU"
    echo ""
    qemu-system-aarch64 \
        -M virt,gic-version=3 \
        -cpu cortex-a72 \
        -m "$RAM" \
        -nographic \
        -kernel "$KERNEL" \
        -serial mon:stdio \
        -d guest_errors
fi
