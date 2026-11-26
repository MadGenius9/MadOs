#ifndef GDT_H
#define GDT_H

#define GDT_KERNEL_CODE 0x08
#define GDT_KERNEL_DATA 0x10
#define GDT_USER_CODE 0x18
#define GDT_USER_DATA 0x20

/* Flat 4 GiB segments for ring 0 and ring 3. Paging does the real memory
   protection; segmentation is set up only because the CPU requires it. */
void gdt_init(void);

#endif
