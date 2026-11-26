; hello - greets from ring 3, prints its pid and exits with code 0.
bits 32
org 0x40000000
%include "syscalls.inc"

start:
    PRINT greeting

    mov eax, SYS_GETPID
    int 0x80

    ; Convert the pid in EAX to decimal, right to left, ending at digits_end.
    mov edi, digits_end
    mov ecx, 10
.next_digit:
    xor edx, edx
    div ecx
    add dl, '0'
    dec edi
    mov [edi], dl
    test eax, eax
    jnz .next_digit

    mov ecx, digits_end
    sub ecx, edi
    WRITE edi, ecx
    PRINT newline
    EXIT 0

STRING greeting, "Hello from user mode (ring 3)! My pid is "
STRING newline, 10
digits: times 10 db 0
digits_end:
