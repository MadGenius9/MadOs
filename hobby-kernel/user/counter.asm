; counter - prints "counter N: k" five times, sleeping 300 ms in between,
; where N is its pid. Run two in the background to watch them interleave.
bits 32
section .text
global _start
%include "syscalls.inc"

_start:
    mov eax, SYS_GETPID
    int 0x80
    add al, '0'                 ; single-digit pids are enough for a demo
    cmp al, '9'
    jbe .pid_ok
    mov al, '#'
.pid_ok:
    mov [line + 8], al
    mov esi, '1'
.loop:
    mov eax, esi
    mov [line + 11], al
    PRINT line
    mov eax, SYS_SLEEP
    mov ebx, 300
    int 0x80
    inc esi
    cmp esi, '5'
    jbe .loop
    EXIT 0

STRING line, "counter ?: ?", 10

section .note.GNU-stack noalloc noexec nowrite progbits
