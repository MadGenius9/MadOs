#ifndef PMM_H
#define PMM_H

#include <stdint.h>

#include "multiboot2.h"

#define PAGE_SIZE 4096u

/* Bitmap physical frame allocator covering the 32-bit physical address
   space (4 GiB = 1M frames, 128 KiB of bitmap). One bit per 4 KiB frame:
   1 = used or not RAM, 0 = free. */

/* Frees every frame the Multiboot2 memory map reports as available, then
   reserves the first MiB, the kernel image and the Multiboot2 info. */
void pmm_init(const struct mb2_info *info);

/* Marks frames overlapping [base, base + length) as used. */
void pmm_reserve_range(uint64_t base, uint64_t length);

/* Returns the physical address of a free frame, or 0 when out of memory
   (frame 0 is always reserved, so 0 is never a valid result). */
uint32_t pmm_alloc_frame(void);
void pmm_free_frame(uint32_t phys);

uint32_t pmm_usable_frames(void); /* RAM frames reported by the bootloader */
uint32_t pmm_free_frames(void);

#endif
