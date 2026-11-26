#include "pit.h"

#include "idt.h"
#include "io.h"
#include "pic.h"

#define PIT_CHANNEL0 0x40
#define PIT_COMMAND 0x43
#define PIT_BASE_HZ 1193182u
/* Channel 0, low then high byte, mode 3 (square wave), binary. */
#define PIT_MODE_SQUARE 0x36

static volatile uint32_t ticks;
static uint32_t frequency;

static void on_tick(struct regs *r)
{
    (void)r;
    ticks++;
}

void pit_init(uint32_t hz)
{
    uint32_t divisor = PIT_BASE_HZ / hz;
    frequency = hz;
    outb(PIT_COMMAND, PIT_MODE_SQUARE);
    outb(PIT_CHANNEL0, (uint8_t)(divisor & 0xFF));
    outb(PIT_CHANNEL0, (uint8_t)((divisor >> 8) & 0xFF));
    interrupt_register_handler(IRQ_VECTOR(0), on_tick);
    pic_unmask(0);
}

uint32_t pit_ticks(void)
{
    return ticks;
}

uint32_t pit_hz(void)
{
    return frequency;
}
