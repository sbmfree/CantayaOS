#ifndef EFI_H
#define EFI_H

#include "wchar.h"

#define EFI_SUCCESS 0
#define FALSE 0

typedef unsigned long long EFI_STATUS;
typedef void *EFI_HANDLE;

typedef struct {
    EFI_STATUS (*Reset)(void *This, int ExtendedVerification);
    EFI_STATUS (*ReadKeyStroke)(void *This, void *Key);
} EFI_SIMPLE_TEXT_INPUT_PROTOCOL;

typedef struct {
    void *FirmwareVendor;
    unsigned int FirmwareRevision;
    void *ConsoleInHandle;
    EFI_SIMPLE_TEXT_INPUT_PROTOCOL *ConIn;
    void *ConsoleOutHandle;
    void *ConOut;
    void *StandardErrorHandle;
    void *StdErr;
    void *RuntimeServices;
    void *BootServices;
    unsigned long long NumberOfTableEntries;
    void *ConfigurationTable;
} EFI_SYSTEM_TABLE;

typedef struct {
    unsigned short ScanCode;
    wchar_t UnicodeChar;
} EFI_INPUT_KEY;

#endif // EFI_H