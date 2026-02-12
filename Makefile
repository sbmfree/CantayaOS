# CantayaOS Makefile
# Convenience wrapper around build.sh / run.sh

.PHONY: all build release debug run clean

all: release

release:
	@bash build.sh release

debug:
	@bash build.sh debug

run: release
	@bash run.sh

run-debug: debug
	@bash run.sh target/aarch64-cantaya/debug/cantaya_kernel

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
	@echo "  make release    - Build release kernel (default)"
	@echo "  make debug      - Build debug kernel"
	@echo "  make run        - Build & run in QEMU"
	@echo "  make run-debug  - Build debug & run in QEMU"
	@echo "  make clean      - Clean build artifacts"
	@echo "  make fmt        - Format all source code"
	@echo "  make check      - Type-check without building"
	@echo "  make help       - Show this help"
