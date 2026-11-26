; priv - executes a privileged instruction (cli) in ring 3, which raises a
; general protection fault; the kernel kills the process.
bits 32
section .text
global _start
%include "syscalls.inc"

_start:
    PRINT trying
    cli
    PRINT survived              ; never reached
    EXIT 1

STRING trying, "priv: executing cli in ring 3...", 10
STRING survived, "priv: BUG: cli was allowed in ring 3", 10

section .note.GNU-stack noalloc noexec nowrite progbits
