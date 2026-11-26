#ifndef IDT_H
#define IDT_H

#include <stdint.h>

#define IRQ_BASE 32
#define IRQ_VECTOR(irq) (IRQ_BASE + (irq))
#define SYSCALL_VECTOR 0x80 /* int 0x80, allowed from ring 3 */
#define YIELD_VECTOR 0x81   /* int 0x81, kernel only: run the scheduler */

/* Stack frame built by isr.asm, lowest address first. */
struct regs {
    uint32_t gs, fs, es, ds;
    uint32_t edi, esi, ebp, esp, ebx, edx, ecx, eax; /* pusha; esp is the value inside the stub */
    uint32_t int_no, err_code;
    uint32_t eip, cs, eflags; /* pushed by the CPU */
    uint32_t useresp, ss;     /* pushed by the CPU only when coming from ring 3 */
};

static inline int regs_from_user(const struct regs *r)
{
    return (r->cs & 3) == 3;
}

typedef void (*interrupt_handler_t)(struct regs *r);

void idt_init(void);

/* Install a C handler for a vector. For IRQ vectors the dispatcher sends the
   end-of-interrupt to the PIC after the handler returns. */
void interrupt_register_handler(uint8_t vector, interrupt_handler_t handler);

/* Called from isr.asm. Returns the frame to resume: the same one, or another
   task's if the scheduler decided to switch. */
struct regs *interrupt_dispatch(struct regs *r);

#endif
