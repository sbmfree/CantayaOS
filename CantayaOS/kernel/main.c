#include <efi.h>
#include <efilib.h>
#include "kernel.h"

EFI_STATUS
EFIAPI
UefiMain(EFI_HANDLE ImageHandle, EFI_SYSTEM_TABLE *SystemTable) {
    InitializeLib(ImageHandle, SystemTable);
    
    // Initialize the kernel
    KernelInitialize();

    // Main loop for handling tasks
    while (1) {
        // Kernel task handling logic goes here
        KernelMainLoop();
    }

    return EFI_SUCCESS;
}