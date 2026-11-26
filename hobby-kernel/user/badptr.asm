; badptr - tries to reach kernel memory from ring 3, two ways:
;  1. passes a kernel address to write() and open(); the kernel must
;     refuse both (-EFAULT)
;  2. writes to the kernel image directly; the MMU must fault, and the
;     kernel must kill this process, not crash.
bits 32
section .text
global _start
%include "syscalls.inc"

_start:
    WRITE 0x00100000, 16        ; the kernel image starts at 1 MiB
    cmp eax, -EFAULT
    jne .accepted
    PRINT refused
    jmp .poke
.accepted:
    PRINT accepted
.poke:
    mov eax, SYS_OPEN           ; a kernel address as the path
    mov ebx, 0x00100000
    int 0x80
    cmp eax, -EFAULT
    jne .open_accepted
    PRINT open_refused
    jmp .direct
.open_accepted:
    PRINT open_accepted
.direct:
    PRINT poking
    mov dword [0x00100000], 0xDEADBEEF
    PRINT survived              ; never reached
    EXIT 1

STRING refused, "badptr: write() with a kernel pointer was refused (EFAULT)", 10
STRING accepted, "badptr: BUG: write() accepted a kernel pointer", 10
STRING open_refused, "badptr: open() with a kernel pointer was refused (EFAULT)", 10
STRING open_accepted, "badptr: BUG: open() accepted a kernel pointer", 10
STRING poking, "badptr: now writing to kernel memory directly...", 10
STRING survived, "badptr: BUG: the write to kernel memory succeeded", 10

section .note.GNU-stack noalloc noexec nowrite progbits
