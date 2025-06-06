# Makefile for CantayaOS

CROSS_COMPILE ?= x86_64-elf-
CC = $(CROSS_COMPILE)gcc
LD = $(CROSS_COMPILE)ld
OBJCOPY = $(CROSS_COMPILE)objcopy

BUILD_DIR = build
BOOTLOADER_DIR = bootloader
KERNEL_DIR = kernel

all: $(BUILD_DIR)/CantayaOS.iso

$(BUILD_DIR)/bootloader.efi:
	mkdir -p $(BUILD_DIR)
	$(CC) -I$(abspath gnu-efi/inc) -ffreestanding -fshort-wchar -mno-red-zone -m64 -nostdlib \
	-T $(abspath gnu-efi/gnuefi/elf_x86_64_efi.lds) \
	-L$(abspath gnu-efi/x86_64/lib) -lefi -lgnuefi \
	-o $@ $(BOOTLOADER_DIR)/bootloader.c

$(BUILD_DIR)/kernel.bin:
	$(CC) -ffreestanding -m64 -nostdlib -o $@ $(KERNEL_DIR)/kernel.c

$(BUILD_DIR)/CantayaOS.iso: $(BUILD_DIR)/bootloader.efi $(BUILD_DIR)/kernel.bin
	mkdir -p $(BUILD_DIR)/iso/EFI/BOOT
	cp $(BUILD_DIR)/bootloader.efi $(BUILD_DIR)/iso/EFI/BOOT/BOOTX64.EFI
	cp $(BUILD_DIR)/kernel.bin $(BUILD_DIR)/iso/kernel.bin
	genisoimage -o $@ -b EFI/BOOT/BOOTX64.EFI -no-emul-boot -boot-load-size 4 -boot-info-table $(BUILD_DIR)/iso

clean:
	rm -rf $(BUILD_DIR)

.PHONY: all clean
