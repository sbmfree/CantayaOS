#ifndef EFILIB_H
#define EFILIB_H

#include "efi.h"
#include "wchar.h"

#define EFIAPI __attribute__((ms_abi))

void InitializeLib(EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE *SystemTable);
void Print(const wchar_t *fmt, ...);

#endif // EFILIB_H

#include <wchar.h>

void Print(const wchar_t *fmt, ...) {
    // Placeholder implementation for wide string printing
}