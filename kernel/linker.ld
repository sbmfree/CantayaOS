/* CantayaOS Kernel Linker Script for AArch64 */

ENTRY(_start)

MEMORY
{
    RAM (rwx) : ORIGIN = 0x40080000, LENGTH = 128M
}

SECTIONS
{
    . = 0x40080000;

    .text.boot : {
        KEEP(*(.text.boot))
    } > RAM

    .vectors : ALIGN(2048) {
        KEEP(*(.vectors))
    } > RAM

    .text : {
        *(.text .text.*)
    } > RAM

    .rodata : ALIGN(4096) {
        *(.rodata .rodata.*)
    } > RAM

    .data : ALIGN(4096) {
        __data_start = .;
        *(.data .data.*)
        __data_end = .;
    } > RAM

    .bss : ALIGN(4096) {
        __bss_start = .;
        *(.bss .bss.*)
        *(COMMON)
        __bss_end = .;
    } > RAM

    .bss.stack : ALIGN(4096) {
        *(.bss.stack)
    } > RAM

    __kernel_end = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
