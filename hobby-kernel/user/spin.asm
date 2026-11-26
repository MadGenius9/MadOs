; spin - burns CPU forever without system calls. The timer still preempts
; it, so the shell stays responsive; stop it with `kill PID` or Ctrl+C.
bits 32
section .text
global _start

_start:
    jmp _start

section .note.GNU-stack noalloc noexec nowrite progbits
