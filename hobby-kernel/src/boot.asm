; boot.asm - Multiboot2 header and kernel entry point.
;
; GRUB loads the ELF, finds this header in the first 32 KiB, and jumps to
; _start in 32-bit protected mode with paging off, interrupts off,
; EAX = 0x36D76289 (Multiboot2 magic) and EBX = physical address of the
; Multiboot2 information structure.

MB2_MAGIC        equ 0xE85250D6
MB2_ARCH_I386    equ 0
MB2_HEADER_LEN   equ mb2_header_end - mb2_header_start
MB2_CHECKSUM     equ 0x100000000 - (MB2_MAGIC + MB2_ARCH_I386 + MB2_HEADER_LEN)

STACK_SIZE       equ 16384

section .multiboot_header
align 8
mb2_header_start:
    dd MB2_MAGIC
    dd MB2_ARCH_I386
    dd MB2_HEADER_LEN
    dd MB2_CHECKSUM
    ; No framebuffer tag: we want the legacy 80x25 VGA text mode.
    ; End tag (type 0, flags 0, size 8).
    align 8
    dw 0
    dw 0
    dd 8
mb2_header_end:

section .bss
align 16
stack_bottom:
    resb STACK_SIZE
stack_top:

section .text
global _start
extern kernel_main

_start:
    cli
    mov esp, stack_top

    ; Start from a known EFLAGS state (IF clear, DF clear).
    push 0
    popf

    ; kernel_main(uint32_t magic, uint32_t mbi_phys). Keep the stack 16-byte
    ; aligned at the call as the i386 System V ABI expects.
    sub esp, 8
    push ebx
    push eax
    call kernel_main

    ; kernel_main must not return; if it does, halt forever.
.hang:
    cli
    hlt
    jmp .hang

; Mark the stack non-executable for the linker.
section .note.GNU-stack noalloc noexec nowrite progbits
