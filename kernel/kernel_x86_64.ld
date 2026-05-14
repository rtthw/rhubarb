/* The linker script for the kernel.
 */

OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start) /* Located in `src/main.rs`. */

KERNEL_BASE = 0x100000;

SECTIONS {
    /* Start mapping at the kernel's base address. */
    . = KERNEL_BASE;

    __kernel_start = .;

    /* Advance past the program headers... */
    . += SIZEOF_HEADERS;

    /* This is a trick adapted from Redox's kernel linker script that forces
     * the first page to be part of a segment. Without this, the boot loader
     * will map the kernel at the wrong offset.
     *
     * Redox does this to fix a Limine problem, but it also simplifies the
     * `bootloader::load_kernel` implementation.
     */
    .dummy : AT(ADDR(.dummy) - KERNEL_BASE) {}

    .text ALIGN(4K) : AT(ADDR(.text) - KERNEL_BASE) {
        __text_start = .;
        *(.text*)
    }

    .rodata ALIGN(4K) : AT(ADDR(.rodata) - KERNEL_BASE) {
        __text_end = .;
        __rodata_start = .;
        *(.rodata*)
    }

    .data ALIGN(4K) : AT(ADDR(.data) - KERNEL_BASE) {
        __rodata_end = .;
        *(.data*)
        . = ALIGN(4K);
        *(.bss*)
    }

    __kernel_end = .;

    /DISCARD/ : {
        *(.comment*)
        *(.note*)
        *(.eh_frame*)
        *(.rel.eh_frame*)
        *(.gcc_except_table*)
    }
}
