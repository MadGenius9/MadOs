#include "pic.h"

#include "io.h"

#define PIC1_COMMAND 0x20
#define PIC1_DATA 0x21
#define PIC2_COMMAND 0xA0
#define PIC2_DATA 0xA1

#define ICW1_INIT_WITH_ICW4 0x11
#define ICW4_8086 0x01
#define OCW3_READ_ISR 0x0B
#define PIC_EOI 0x20
#define CASCADE_IRQ 2

void pic_remap(uint8_t master_offset, uint8_t slave_offset)
{
    outb(PIC1_COMMAND, ICW1_INIT_WITH_ICW4);
    io_wait();
    outb(PIC2_COMMAND, ICW1_INIT_WITH_ICW4);
    io_wait();
    outb(PIC1_DATA, master_offset); /* ICW2: vector offsets */
    io_wait();
    outb(PIC2_DATA, slave_offset);
    io_wait();
    outb(PIC1_DATA, 1 << CASCADE_IRQ); /* ICW3: slave sits on master IRQ 2 */
    io_wait();
    outb(PIC2_DATA, CASCADE_IRQ); /* ICW3: slave's cascade identity */
    io_wait();
    outb(PIC1_DATA, ICW4_8086);
    io_wait();
    outb(PIC2_DATA, ICW4_8086);
    io_wait();

    /* Mask everything; drivers unmask the lines they handle. */
    outb(PIC1_DATA, 0xFF);
    outb(PIC2_DATA, 0xFF);
}

void pic_unmask(uint8_t irq)
{
    if (irq < 8) {
        outb(PIC1_DATA, inb(PIC1_DATA) & (uint8_t)~(1u << irq));
    } else {
        outb(PIC2_DATA, inb(PIC2_DATA) & (uint8_t)~(1u << (irq - 8)));
        pic_unmask(CASCADE_IRQ);
    }
}

void pic_mask(uint8_t irq)
{
    if (irq < 8)
        outb(PIC1_DATA, inb(PIC1_DATA) | (uint8_t)(1u << irq));
    else
        outb(PIC2_DATA, inb(PIC2_DATA) | (uint8_t)(1u << (irq - 8)));
}

void pic_send_eoi(uint8_t irq)
{
    if (irq >= 8)
        outb(PIC2_COMMAND, PIC_EOI);
    outb(PIC1_COMMAND, PIC_EOI);
}

static uint8_t read_isr(uint16_t command_port)
{
    outb(command_port, OCW3_READ_ISR);
    return inb(command_port);
}

bool pic_is_spurious(uint8_t irq)
{
    if (irq == 7 && !(read_isr(PIC1_COMMAND) & 0x80))
        return true;
    if (irq == 15 && !(read_isr(PIC2_COMMAND) & 0x80)) {
        /* The master did see a real request on the cascade line. */
        outb(PIC1_COMMAND, PIC_EOI);
        return true;
    }
    return false;
}
