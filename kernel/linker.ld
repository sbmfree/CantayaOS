/* CantayaOS Kernel Linker Script
 *
 * This linker script places the kernel in the higher half of the virtual address space,
 * starting at 0xFFFFFFFF80000000 (the last 2 GiB of the 64-bit address space).
 *
 * Higher-half kernel design rationale:
 *   - User processes get the lower 128 TiB of virtual address space
 *   - The kernel is mapped into every process's address space at the same high address
 *   - This allows the kernel to access both its own data and user memory without page table switches
 *   - This is the same approach used by Windows (upper 128 TiB) and Linux
 *
 * The kernel is loaded physically at a lower address by the bootloader,
 * then paging maps it to these virtual addresses.
 */

ENTRY(_start)

/* Higher-half kernel virtual base address */
KERNEL_VADDR = 0xFFFFFFFF80000000;

SECTIONS
{
    /* Place all sections starting at the higher-half virtual address */
    . = KERNEL_VADDR;

    /* Code section - must come first for the entry point */
    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_VADDR)
    {
        __text_start = .;
        *(.text .text.*)
        __text_end = .;
    }

    /* Read-only data - constants, string literals, etc. */
    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_VADDR)
    {
        __rodata_start = .;
        *(.rodata .rodata.*)
        __rodata_end = .;
    }

    /* Initialized read-write data */
    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_VADDR)
    {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    }

    /* Uninitialized data (zeroed by the bootloader/startup code) */
    .bss ALIGN(4K) : AT(ADDR(.bss) - KERNEL_VADDR)
    {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)

        /* Kernel bootstrap stack — 64 KiB, 16-byte aligned for System V ABI.
         * The stack grows downward on x86, so __kernel_stack_top is the initial RSP. */
        . = ALIGN(16);
        __kernel_stack_bottom = .;
        . += 0x10000;   /* 64 KiB */
        __kernel_stack_top = .;

        __bss_end = .;
    }

    /* Total kernel size tracking for the bootloader */
    __kernel_start = KERNEL_VADDR;
    __kernel_end = .;

    /* Discard unnecessary sections to keep the binary clean */
    /DISCARD/ :
    {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
