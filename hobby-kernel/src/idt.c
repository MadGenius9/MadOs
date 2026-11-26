#include "idt.h"

#include "gdt.h"
#include "kprintf.h"
#include "pic.h"
#include "vga.h"

struct idt_entry {
    uint16_t offset_low;
    uint16_t selector;
    uint8_t zero;
    uint8_t type_attr;
    uint16_t offset_high;
} __attribute__((packed));

struct idt_ptr {
    uint16_t limit;
    uint32_t base;
} __attribute__((packed));

/* Present, ring 0, 32-bit interrupt gate (the CPU clears IF on entry). */
#define IDT_INTERRUPT_GATE 0x8E

static struct idt_entry idt[256];
static struct idt_ptr idt_pointer;
static interrupt_handler_t handlers[256];

extern const uint32_t isr_stub_table[48]; /* isr.asm */

static const char *const exception_names[32] = {
    "Divide error",
    "Debug",
    "Non-maskable interrupt",
    "Breakpoint",
    "Overflow",
    "Bound range exceeded",
    "Invalid opcode",
    "Device not available",
    "Double fault",
    "Coprocessor segment overrun",
    "Invalid TSS",
    "Segment not present",
    "Stack-segment fault",
    "General protection fault",
    "Page fault",
    "Reserved",
    "x87 floating-point exception",
    "Alignment check",
    "Machine check",
    "SIMD floating-point exception",
    "Virtualization exception",
    "Control protection exception",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Reserved",
    "Hypervisor injection exception",
    "VMM communication exception",
    "Security exception",
    "Reserved",
};

static void set_gate(uint8_t vector, uint32_t handler, uint16_t selector, uint8_t type_attr)
{
    idt[vector].offset_low = handler & 0xFFFF;
    idt[vector].offset_high = (handler >> 16) & 0xFFFF;
    idt[vector].selector = selector;
    idt[vector].zero = 0;
    idt[vector].type_attr = type_attr;
}

void idt_init(void)
{
    for (uint8_t v = 0; v < 48; v++)
        set_gate(v, isr_stub_table[v], GDT_KERNEL_CODE, IDT_INTERRUPT_GATE);

    idt_pointer.limit = sizeof(idt) - 1;
    idt_pointer.base = (uint32_t)(uintptr_t)&idt;
    __asm__ volatile("lidt %0" : : "m"(idt_pointer));
}

void interrupt_register_handler(uint8_t vector, interrupt_handler_t handler)
{
    handlers[vector] = handler;
}

static void unhandled_exception(struct regs *r)
{
    vga_set_color(VGA_LIGHT_RED, VGA_BLACK);
    /* uint32_t is unsigned long with i686-elf-gcc, so cast for %u/%x. */
    kprintf("\nCPU exception %u: %s (error code 0x%x)\n", (unsigned)r->int_no, exception_names[r->int_no],
            (unsigned)r->err_code);
    kprintf("  eip=%08x cs=%04x eflags=%08x\n", (unsigned)r->eip, (unsigned)r->cs, (unsigned)r->eflags);
    kprintf("  eax=%08x ebx=%08x ecx=%08x edx=%08x\n", (unsigned)r->eax, (unsigned)r->ebx, (unsigned)r->ecx,
            (unsigned)r->edx);
    /* r->esp points at int_no; the interrupted esp is 5 dwords higher. */
    kprintf("  esi=%08x edi=%08x ebp=%08x esp=%08x\n", (unsigned)r->esi, (unsigned)r->edi, (unsigned)r->ebp,
            (unsigned)(r->esp + 20));
    panic("unhandled CPU exception");
}

void interrupt_dispatch(struct regs *r)
{
    if (r->int_no >= IRQ_BASE && r->int_no < IRQ_BASE + 16) {
        uint8_t irq = (uint8_t)(r->int_no - IRQ_BASE);
        if (pic_is_spurious(irq))
            return;
        if (handlers[r->int_no])
            handlers[r->int_no](r);
        pic_send_eoi(irq);
        return;
    }

    if (r->int_no < 256 && handlers[r->int_no]) {
        handlers[r->int_no](r);
        return;
    }
    unhandled_exception(r);
}
