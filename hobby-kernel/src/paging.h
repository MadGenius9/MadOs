#ifndef PAGING_H
#define PAGING_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define PG_PRESENT 0x001u
#define PG_RW 0x002u
#define PG_USER 0x004u

/* Virtual memory layout (every address space):
     0x00000000           unmapped NULL guard page
     0x00001000-0x007FFFFF identity map: low memory, VGA, kernel     (kernel)
     0x40000000-0xBFFFFFFF user programs: code at USER_BASE, stack below USER_TOP
     0xD0000000-0xDFFFFFFF kernel heap                                (kernel)
     0xE0000000-0xEFFFFFFF initial ramdisk, read-only                   (kernel)
     0xFFBFF000           scratch mapping for editing other page tables (kernel)
     0xFFC00000-0xFFFFFFFF recursive mapping of the current page tables */
#define USER_BASE 0x40000000u
#define USER_TOP 0xC0000000u
#define KERNEL_HEAP_BASE 0xD0000000u
#define KERNEL_HEAP_LIMIT 0xE0000000u
#define INITRD_BASE 0xE0000000u
#define INITRD_LIMIT 0xF0000000u

/* Identity-maps the first 8 MiB except page 0 and enables paging with a
   recursive page directory. Page tables for the kernel heap are created up
   front so every address space shares the same kernel mappings. */
void paging_init(void);

/* Maps one 4 KiB page in the current address space; allocates a page table
   from the PMM if needed. Returns false if already mapped or out of memory. */
bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags);

/* Maps [phys, phys + len) at virt in kernel space, page by page. Only for
   boot time, before the first process exists: page tables created here are
   then copied into every address space. */
bool paging_map_kernel_range(uintptr_t virt, uint32_t phys, size_t len, uint32_t flags);

/* Unmaps one page and returns the physical frame it pointed to (0 if it
   was not mapped). Does not free the frame. */
uint32_t paging_unmap_page(uintptr_t virt);

/* Returns the physical address for virt, or 0 if it is not mapped. */
uint32_t paging_virt_to_phys(uintptr_t virt);

/* Page directories, by physical address. */
uint32_t paging_kernel_directory(void);
uint32_t paging_current_directory(void);
void paging_switch_directory(uint32_t pd_phys);

/* A new address space: kernel mappings shared, user range empty.
   Returns its page directory, or 0 when out of memory. */
uint32_t paging_create_address_space(void);

/* Frees every user page, user page table and the directory itself.
   Must not be the current address space. */
void paging_destroy_address_space(uint32_t pd_phys);

/* True if [virt, virt + len) lies in the user range and every page in it is
   mapped present and user-accessible in the current address space. */
bool paging_user_range_ok(uintptr_t virt, size_t len);

#endif
