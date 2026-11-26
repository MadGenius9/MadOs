/* user.ld - links user programs at the start of the user address range. */

ENTRY(_start)

SECTIONS
{
    . = 0x40000000;

    .text : { *(.text .text.*) }
    .rodata : ALIGN(16) { *(.rodata .rodata.*) }
    .data : ALIGN(4096) { *(.data .data.*) }
    .bss : ALIGN(16) { *(COMMON) *(.bss .bss.*) }

    /DISCARD/ :
    {
        *(.comment)
        *(.note .note.*)
        *(.eh_frame .eh_frame_hdr)
    }
}
