#include <efi.h>
#include <efilib.h>

// Minimal UEFI bootloader for CantayaOS

EFI_STATUS
efi_main (EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE *SystemTable) {
    InitializeLib(ImageHandle, SystemTable);
    Print(L"CantayaOS UEFI Bootloader Loaded!\n");
    // Halt the system (for now)
    while (1) {}
    return EFI_SUCCESS;
}
