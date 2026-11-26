; spin - burns CPU forever without system calls. The timer still preempts
; it, so the shell stays responsive; stop it with `kill PID` or Ctrl+C.
bits 32
org 0x40000000

start:
    jmp start
