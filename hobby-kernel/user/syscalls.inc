; syscalls.inc - system call numbers and helpers for user programs.
; Keep in sync with src/syscall.h. ABI: int 0x80, EAX = number,
; EBX/ECX/EDX = arguments, result in EAX (negative = error).

SYS_EXIT    equ 0
SYS_WRITE   equ 1
SYS_GETPID  equ 2
SYS_SLEEP   equ 3
SYS_YIELD   equ 4
SYS_OPEN    equ 5
SYS_READ    equ 6
SYS_CLOSE   equ 7
SYS_READDIR equ 8

EFAULT      equ 14


; write(buf, len)
%macro WRITE 2
    mov eax, SYS_WRITE
    mov ebx, %1
    mov ecx, %2
    int 0x80
%endmacro

; write a string defined with STRING
%macro PRINT 1
    WRITE %1, %1 %+ _len
%endmacro

; label: db "text" plus label_len
%macro STRING 2+
%1: db %2
%1 %+ _len equ $ - %1
%endmacro

%macro EXIT 1
    mov eax, SYS_EXIT
    mov ebx, %1
    int 0x80
%endmacro
