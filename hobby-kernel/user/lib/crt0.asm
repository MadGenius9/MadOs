; crt0.asm - process entry point for C programs.
; The kernel starts a process with ECX = argc and EDX = argv (an array of
; argc string pointers plus NULL, at the top of the user stack).

section .text
global _start
extern main
extern exit

_start:
    xor ebp, ebp            ; end of the frame-pointer chain
    and esp, -16            ; 16-byte stack alignment for the C ABI
    sub esp, 8
    push edx                ; argv
    push ecx                ; argc
    call main
    push eax
    call exit               ; exit(main's return value); never returns
.hang:
    jmp .hang

section .note.GNU-stack noalloc noexec nowrite progbits
