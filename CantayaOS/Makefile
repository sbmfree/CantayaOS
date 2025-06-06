# Use Homebrew's Clang
CC = /opt/homebrew/opt/llvm/bin/clang

# Compiler flags for a UEFI target
CFLAGS = -target x86_64-efi -ffreestanding -nostdlib -Wall -Wextra -I./boot -I./kernel -I./drivers

# Linker flags: force LLD's GNU flavor with -Wl,-flavor,gnu so that it accepts PE/COFF flags
LDFLAGS = -nostdlib -fuse-ld=lld -Wl,-flavor,gnu -Wl,-subsystem:efi_application -Wl,-Tefi_entry.ld -Wl,-shared -Wl,-Bsymbolic

# Source and output settings
SOURCES_BOOT = boot/efi_entry.c
OBJECTS_BOOT = $(SOURCES_BOOT:.c=.o)
TARGET = CantayaOS.efi

all: $(TARGET)

%.o: %.c
	$(CC) $(CFLAGS) -c $< -o $@

$(TARGET): $(OBJECTS_BOOT)
	$(CC) $(OBJECTS_BOOT) $(LDFLAGS) -o $(TARGET)

clean:
	rm -f $(OBJECTS_BOOT) $(TARGET)