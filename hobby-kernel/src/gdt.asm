; gdt.asm - load the GDT and reload every segment register.

section .text
global gdt_flush

; void gdt_flush(uint32_t gdt_ptr_addr)
gdt_flush:
    mov eax, [esp + 4]
    lgdt [eax]

    mov ax, 0x10            ; kernel data selector
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    jmp 0x08:.reload_cs     ; far jump loads CS with the kernel code selector
.reload_cs:
    ret

; Mark the stack non-executable for the linker.
section .note.GNU-stack noalloc noexec nowrite progbits
