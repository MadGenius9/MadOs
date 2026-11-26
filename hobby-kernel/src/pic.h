#ifndef PIC_H
#define PIC_H

#include <stdbool.h>
#include <stdint.h>

/* The two cascaded 8259 PICs power up mapped to vectors 0x08-0x0F and
   0x70-0x77, which collide with CPU exceptions. pic_remap moves them to
   master_offset.. and slave_offset.. and masks every line. */
void pic_remap(uint8_t master_offset, uint8_t slave_offset);
void pic_unmask(uint8_t irq);
void pic_mask(uint8_t irq);
void pic_send_eoi(uint8_t irq);

/* True for a spurious IRQ 7/15, which must not get a normal EOI. */
bool pic_is_spurious(uint8_t irq);

#endif
