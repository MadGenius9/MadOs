; isr.asm - entry stubs for CPU exceptions (vectors 0-31) and hardware IRQs
; (vectors 32-47 after the PIC remap).
;
; Every stub leaves the same frame on the stack (see struct regs in idt.h)
; and jumps to a common routine that calls interrupt_dispatch(struct regs *).
; The dispatcher returns the frame to resume, which is how the scheduler
; switches tasks: it returns another task's saved frame and the stub
; continues on that task's kernel stack.

extern interrupt_dispatch

; Exceptions where the CPU does not push an error code: push a dummy 0.
%macro ISR_NOERR 1
isr%1:
    push dword 0
    push dword %1
    jmp interrupt_common
%endmacro

; Exceptions where the CPU pushes an error code itself.
%macro ISR_ERR 1
isr%1:
    push dword %1
    jmp interrupt_common
%endmacro

; Hardware IRQ n arrives on vector 32 + n.
%macro IRQ 2
irq%1:
    push dword 0
    push dword %2
    jmp interrupt_common
%endmacro

section .text

ISR_NOERR 0     ; divide error
ISR_NOERR 1     ; debug
ISR_NOERR 2     ; NMI
ISR_NOERR 3     ; breakpoint
ISR_NOERR 4     ; overflow
ISR_NOERR 5     ; bound range exceeded
ISR_NOERR 6     ; invalid opcode
ISR_NOERR 7     ; device not available
ISR_ERR   8     ; double fault
ISR_NOERR 9     ; coprocessor segment overrun
ISR_ERR   10    ; invalid TSS
ISR_ERR   11    ; segment not present
ISR_ERR   12    ; stack-segment fault
ISR_ERR   13    ; general protection fault
ISR_ERR   14    ; page fault
ISR_NOERR 15    ; reserved
ISR_NOERR 16    ; x87 floating-point exception
ISR_ERR   17    ; alignment check
ISR_NOERR 18    ; machine check
ISR_NOERR 19    ; SIMD floating-point exception
ISR_NOERR 20    ; virtualization exception
ISR_ERR   21    ; control protection exception
ISR_NOERR 22
ISR_NOERR 23
ISR_NOERR 24
ISR_NOERR 25
ISR_NOERR 26
ISR_NOERR 27
ISR_NOERR 28
ISR_ERR   29    ; VMM communication exception
ISR_ERR   30    ; security exception
ISR_NOERR 31

IRQ 0, 32       ; PIT timer
IRQ 1, 33       ; PS/2 keyboard
IRQ 2, 34       ; cascade (never raised)
IRQ 3, 35       ; COM2
IRQ 4, 36       ; COM1
IRQ 5, 37
IRQ 6, 38       ; floppy
IRQ 7, 39       ; LPT1 / spurious
IRQ 8, 40       ; CMOS RTC
IRQ 9, 41
IRQ 10, 42
IRQ 11, 43
IRQ 12, 44      ; PS/2 mouse
IRQ 13, 45      ; FPU
IRQ 14, 46      ; primary ATA
IRQ 15, 47      ; secondary ATA / spurious

ISR_NOERR 128   ; int 0x80: system call (callable from ring 3)
ISR_NOERR 129   ; int 0x81: yield to the scheduler (kernel only)

interrupt_common:
    pusha                   ; edi, esi, ebp, esp, ebx, edx, ecx, eax
    push ds
    push es
    push fs
    push gs

    mov ax, 0x10            ; kernel data selector
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    cld                     ; the C ABI expects DF = 0

    push esp                ; argument: struct regs *
    call interrupt_dispatch
    mov esp, eax            ; frame to resume (possibly another task's)

    pop gs
    pop fs
    pop es
    pop ds
    popa
    add esp, 8              ; drop the vector number and error code
    iret

; Table of stub addresses for idt.c: 32 exceptions, 16 IRQs, then the
; system call and yield vectors.
section .rodata
global isr_stub_table
isr_stub_table:
%assign i 0
%rep 32
    dd isr %+ i
%assign i i+1
%endrep
%assign i 0
%rep 16
    dd irq %+ i
%assign i i+1
%endrep
    dd isr128
    dd isr129

; Mark the stack non-executable for the linker.
section .note.GNU-stack noalloc noexec nowrite progbits
