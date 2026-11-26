#ifndef GDT_H
#define GDT_H

#include <stdint.h>

#define GDT_KERNEL_CODE 0x08
#define GDT_KERNEL_DATA 0x10
#define GDT_USER_CODE 0x18
#define GDT_USER_DATA 0x20
#define GDT_TSS 0x28
/* Selectors with RPL 3, as loaded into segment registers in ring 3. */
#define USER_CODE_SELECTOR (GDT_USER_CODE | 3)
#define USER_DATA_SELECTOR (GDT_USER_DATA | 3)

/* Flat 4 GiB segments for ring 0 and ring 3. Paging does the real memory
   protection; segmentation is set up only because the CPU requires it. */
void gdt_init(void);

/* The task state segment only matters for one field here: esp0, the kernel
   stack the CPU switches to when an interrupt arrives in ring 3. The
   scheduler sets it to the running task's kernel stack. */
void tss_set_kernel_stack(uint32_t esp0);

#endif
