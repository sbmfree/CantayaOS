#ifndef EFI_H
#define EFI_H

typedef unsigned long long EFI_STATUS;
typedef void* EFI_HANDLE;

typedef struct {
    int dummy;
} EFI_SYSTEM_TABLE;

#define EFIAPI

#define EFI_SUCCESS 0   // Add this line

// Function prototype
EFI_STATUS EFIAPI efi_main(EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE* SystemTable);

#endif // EFI_H