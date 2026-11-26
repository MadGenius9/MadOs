/* linker.ld - lays the kernel out at 1 MiB physical (identity mapped). */

ENTRY(_start)

SECTIONS
{
    . = 1M;
    _kernel_start = .;

    /* The Multiboot2 header must be within the first 32 KiB of the file,
       so it goes first in the first loaded section. */
    .text BLOCK(4K) : ALIGN(4K)
    {
        KEEP(*(.multiboot_header))
        *(.text .text.*)
    }

    .rodata BLOCK(4K) : ALIGN(4K)
    {
        *(.rodata .rodata.*)
    }

    .data BLOCK(4K) : ALIGN(4K)
    {
        *(.data .data.*)
    }

    .bss BLOCK(4K) : ALIGN(4K)
    {
        *(COMMON)
        *(.bss .bss.*)
    }

    . = ALIGN(4K);
    _kernel_end = .;

    /DISCARD/ :
    {
        *(.comment)
        *(.note .note.*)
        *(.eh_frame .eh_frame_hdr)
    }
}
