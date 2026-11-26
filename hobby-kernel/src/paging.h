#ifndef PAGING_H
#define PAGING_H

#include <stdbool.h>
#include <stdint.h>

#define PG_PRESENT 0x001u
#define PG_RW 0x002u
#define PG_USER 0x004u

/* Identity-maps the first 8 MiB (kernel, VGA memory, page tables) except
   page 0, so NULL dereferences fault. The page directory's last entry
   points at itself (recursive mapping): page tables are then visible at
   0xFFC00000 and the directory at 0xFFFFF000. */
void paging_init(void);

/* Maps one 4 KiB page; allocates a page table from the PMM if needed.
   Returns false if the page is already mapped or memory ran out. */
bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags);

/* Unmaps one page and returns the physical frame it pointed to (0 if it
   was not mapped). Does not free the frame. */
uint32_t paging_unmap_page(uintptr_t virt);

/* Returns the physical address for virt, or 0 if it is not mapped. */
uint32_t paging_virt_to_phys(uintptr_t virt);

#endif
