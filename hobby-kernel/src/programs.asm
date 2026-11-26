; programs.asm - embeds the user programs (flat binaries assembled from
; user/*.asm into build/user/) into the kernel image. src/programs.c lists them.

section .rodata

%macro PROGRAM 2
global program_%1_start
global program_%1_end
align 16
program_%1_start:
    incbin %2
program_%1_end:
%endmacro

PROGRAM hello, "hello.bin"
PROGRAM counter, "counter.bin"
PROGRAM spin, "spin.bin"
PROGRAM badptr, "badptr.bin"
PROGRAM priv, "priv.bin"

section .note.GNU-stack noalloc noexec nowrite progbits
