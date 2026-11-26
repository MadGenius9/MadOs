; badptr - tries to reach kernel memory from ring 3, two ways:
;  1. passes a kernel address to write(); the kernel must refuse (-EFAULT)
;  2. writes to the kernel image directly; the MMU must fault, and the
;     kernel must kill this process, not crash.
bits 32
org 0x40000000
%include "syscalls.inc"

start:
    WRITE 0x00100000, 16        ; the kernel image starts at 1 MiB
    cmp eax, -EFAULT
    jne .accepted
    PRINT refused
    jmp .poke
.accepted:
    PRINT accepted
.poke:
    PRINT poking
    mov dword [0x00100000], 0xDEADBEEF
    PRINT survived              ; never reached
    EXIT 1

STRING refused, "badptr: write() with a kernel pointer was refused (EFAULT)", 10
STRING accepted, "badptr: BUG: write() accepted a kernel pointer", 10
STRING poking, "badptr: now writing to kernel memory directly...", 10
STRING survived, "badptr: BUG: the write to kernel memory succeeded", 10
