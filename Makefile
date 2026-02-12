# CantayaOS Makefile
# Convenience wrapper around build.sh / run.sh

.PHONY: all build release debug run clean

all: release

release:
	@bash build.sh release

debug:
	@bash build.sh debug

# x86_64 builds
release-x86_64:
	@ARCH=x86_64 bash build.sh release

debug-x86_64:
	@ARCH=x86_64 bash build.sh debug

# Run targets
run: release
	@bash run.sh

run-debug: debug
	@bash run.sh target/aarch64-cantaya/debug/cantaya_kernel

run-x86_64: release-x86_64
	@ARCH=x86_64 bash run.sh

run-debug-x86_64: debug-x86_64
	@ARCH=x86_64 bash run.sh target/x86_64-cantaya/debug/cantaya_kernel

clean:
	cargo clean
	rm -f cantaya.bin

fmt:
	cargo fmt --all

check:
	cargo check

help:
	@echo "CantayaOS Build System"
	@echo ""
	@echo "Targets:"
	@echo "  make release          - Build release kernel for AARCH64 (default)"
	@echo "  make debug            - Build debug kernel for AARCH64"
	@echo "  make release-x86_64   - Build release kernel for x86_64"
	@echo "  make debug-x86_64     - Build debug kernel for x86_64"
	@echo "  make run              - Build & run AARCH64 in QEMU"
	@echo "  make run-debug        - Build debug & run AARCH64 in QEMU"
	@echo "  make run-x86_64       - Build & run x86_64 in QEMU"
	@echo "  make run-debug-x86_64 - Build debug & run x86_64 in QEMU"
	@echo "  make clean            - Clean build artifacts"
	@echo "  make fmt              - Format all source code"
	@echo "  make check            - Type-check without building"
	@echo "  make help             - Show this help"
