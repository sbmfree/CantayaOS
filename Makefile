# Compiler and tools
CC = clang
LD = lld-link
OBJCOPY = llvm-objcopy # Add objcopy for creating raw kernel binary if needed

# CFLAGS for bootloader - target for Windows PE/EFI
BOOT_CFLAGS = -ffreestanding -mno-red-zone -Wall -Wextra -Iboot --target=x86_64-pc-win32 -c
# CFLAGS for kernel - freestanding, no target, position independent (for now, will become clear later)
KERNEL_CFLAGS = -ffreestanding -mno-red-zone -Wall -Wextra -Ikernel -c

# LDFLAGS for bootloader - PE/EFI executable
BOOT_LDFLAGS = /entry:efi_main /subsystem:efi_application /nodefaultlib

# LDFLAGS for kernel - raw binary or ELF
# For a raw binary, we link it to a specific base address and then convert.
# For simplicity, let's produce an ELF first and then perhaps a raw binary from it.
KERNEL_LDFLAGS = -nostdlib -m elf_x86_64 # For a standalone ELF
# If you want a flat binary without ELF headers, you'd use a linker script and objcopy:
# KERNEL_LDFLAGS_FLAT = -nostdlib -T kernel/kernel.ld # Example linker script

# Directories
BOOT_DIR = boot
KERNEL_DIR = kernel
BUILD_DIR = build
EFI_DIR = efi # Directory for the EFI system partition content

# Files
BOOT_SRC = $(BOOT_DIR)/efi_entry.c
KERNEL_SRC = $(KERNEL_DIR)/main.c
BOOT_OBJ = $(BUILD_DIR)/efi_entry.o
KERNEL_OBJ = $(BUILD_DIR)/main.o

EFI_FILE = $(EFI_DIR)/EFI/BOOT/BOOTx64.efi # Standard EFI boot path
KERNEL_ELF = $(BUILD_DIR)/kernel.elf
KERNEL_BIN = $(EFI_DIR)/kernel.bin # The raw kernel file to be loaded

# Build rules
all: $(EFI_FILE) $(KERNEL_BIN)

$(BUILD_DIR):
	mkdir -p $(BUILD_DIR)

$(EFI_DIR)/EFI/BOOT:
	mkdir -p $@

$(BOOT_OBJ): $(BOOT_SRC) | $(BUILD_DIR)
	$(CC) $(BOOT_CFLAGS) $< -o $@

$(KERNEL_OBJ): $(KERNEL_SRC) | $(BUILD_DIR)
	$(CC) $(KERNEL_CFLAGS) $< -o $@

$(KERNEL_ELF): $(KERNEL_OBJ)
	$(LD) $(KERNEL_LDFLAGS) $^ -o $@

$(KERNEL_BIN): $(KERNEL_ELF)
	$(OBJCOPY) -O binary $< $@ # Convert ELF to a raw binary

$(EFI_FILE): $(BOOT_OBJ) | $(EFI_DIR)/EFI/BOOT
	$(LD) $(BOOT_LDFLAGS) $^ /out:$@

clean:
	rm -rf $(BUILD_DIR) $(EFI_DIR) disk.img

run: all disk.img
	qemu-system-x86_64 -bios /usr/share/ovmf/x64/OVMF_CODE.fd -drive format=raw,file=disk.img

# Create a FAT32 disk image and copy files
disk.img: $(EFI_FILE) $(KERNEL_BIN)
	dd if=/dev/zero of=disk.img bs=1M count=64 # Create a 64MB disk image
	mkfs.fat -F 32 disk.img
	mmd -i disk.img ::EFI
	mmd -i disk.img ::EFI/BOOT
	mcopy -i disk.img $(EFI_FILE) ::EFI/BOOT
	mcopy -i disk.img $(KERNEL_BIN) ::
