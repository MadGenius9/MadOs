#ifndef PIT_H
#define PIT_H

#include <stdint.h>

/* 8253/8254 programmable interval timer on IRQ 0. */
void pit_init(uint32_t hz);
uint32_t pit_ticks(void);
uint32_t pit_hz(void);

#endif
