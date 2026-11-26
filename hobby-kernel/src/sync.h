#ifndef SYNC_H
#define SYNC_H

#include <stdint.h>

/* The kernel runs on one CPU, so a critical section only has to keep
   interrupts (and therefore preemption) out:

       uint32_t flags = irq_save();
       ... touch shared data ...
       irq_restore(flags);

   Sections nest: irq_restore only re-enables interrupts if they were on at
   the matching irq_save. */

#ifdef UNIT_TEST /* host unit tests: no interrupts to disable */
static inline uint32_t irq_save(void)
{
    return 0;
}
static inline void irq_restore(uint32_t flags)
{
    (void)flags;
}
#else
#define EFLAGS_IF 0x200u

static inline uint32_t irq_save(void)
{
    uint32_t flags;
    __asm__ volatile("pushf; pop %0; cli" : "=r"(flags) : : "memory");
    return flags;
}

static inline void irq_restore(uint32_t flags)
{
    if (flags & EFLAGS_IF)
        __asm__ volatile("sti" : : : "memory");
}
#endif

#endif
