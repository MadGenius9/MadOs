#include "paging.h"

#include "idt.h"
#include "kernel.h"
#include "kprintf.h"
#include "kstring.h"
#include "pmm.h"
#include "vga.h"

#define ENTRIES 1024
#define IDENTITY_MAP_BYTES (8u << 20)
#define IDENTITY_TABLES (IDENTITY_MAP_BYTES / (ENTRIES * PAGE_SIZE))
#define RECURSIVE_SLOT 1023u
#define RECURSIVE_PD ((volatile uint32_t *)0xFFFFF000u)
#define RECURSIVE_PT(pdi) ((volatile uint32_t *)(0xFFC00000u + ((uint32_t)(pdi) << 12)))
#define FRAME_MASK 0xFFFFF000u

#define CR0_WP (1u << 16) /* honour read-only pages in ring 0 too */
#define CR0_PG (1u << 31)

/* Static so that their physical address equals their (identity-mapped)
   virtual address; GRUB loaded the kernel at 1 MiB with paging off. */
static uint32_t page_directory[ENTRIES] __attribute__((aligned(4096)));
static uint32_t identity_tables[IDENTITY_TABLES][ENTRIES] __attribute__((aligned(4096)));

static inline void invlpg(uintptr_t virt)
{
    __asm__ volatile("invlpg (%0)" : : "r"(virt) : "memory");
}

static inline uint32_t read_cr2(void)
{
    uint32_t value;
    __asm__ volatile("mov %%cr2, %0" : "=r"(value));
    return value;
}

static void page_fault_handler(struct regs *r)
{
    uint32_t addr = read_cr2();
    vga_set_color(VGA_LIGHT_RED, VGA_BLACK);
    kprintf("\nPage fault at address 0x%08x (eip=0x%08x)\n", (unsigned)addr, (unsigned)r->eip);
    kprintf("  %s, %s access, %s mode%s\n", (r->err_code & 1) ? "protection violation" : "page not present",
            (r->err_code & 2) ? "write" : "read", (r->err_code & 4) ? "user" : "kernel",
            (r->err_code & 16) ? ", instruction fetch" : "");
    panic("unhandled page fault");
}

void paging_init(void)
{
    if ((uintptr_t)_kernel_end > IDENTITY_MAP_BYTES)
        panic("kernel image ends at %p, beyond the identity map", (void *)_kernel_end);

    for (uint32_t t = 0; t < IDENTITY_TABLES; t++) {
        for (uint32_t i = 0; i < ENTRIES; i++) {
            uint32_t phys = (t * ENTRIES + i) * PAGE_SIZE;
            identity_tables[t][i] = phys | PG_PRESENT | PG_RW;
        }
        page_directory[t] = (uint32_t)(uintptr_t)identity_tables[t] | PG_PRESENT | PG_RW;
    }
    identity_tables[0][0] = 0; /* leave page 0 unmapped: NULL faults */
    page_directory[RECURSIVE_SLOT] = (uint32_t)(uintptr_t)page_directory | PG_PRESENT | PG_RW;

    interrupt_register_handler(14, page_fault_handler);

    __asm__ volatile("mov %0, %%cr3" : : "r"(page_directory) : "memory");
    uint32_t cr0;
    __asm__ volatile("mov %%cr0, %0" : "=r"(cr0));
    cr0 |= CR0_PG | CR0_WP;
    __asm__ volatile("mov %0, %%cr0" : : "r"(cr0) : "memory");
}

bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags)
{
    uint32_t pdi = virt >> 22;
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (pdi == RECURSIVE_SLOT)
        return false;

    volatile uint32_t *pt = RECURSIVE_PT(pdi);
    if (!(RECURSIVE_PD[pdi] & PG_PRESENT)) {
        uint32_t table = pmm_alloc_frame();
        if (!table)
            return false;
        RECURSIVE_PD[pdi] = table | PG_PRESENT | PG_RW | (flags & PG_USER);
        invlpg((uintptr_t)pt);
        for (uint32_t i = 0; i < ENTRIES; i++)
            pt[i] = 0;
    }
    if (pt[pti] & PG_PRESENT)
        return false;
    pt[pti] = (phys & FRAME_MASK) | (flags & 0xFFF) | PG_PRESENT;
    invlpg(virt);
    return true;
}

uint32_t paging_unmap_page(uintptr_t virt)
{
    uint32_t pdi = virt >> 22;
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (pdi == RECURSIVE_SLOT || !(RECURSIVE_PD[pdi] & PG_PRESENT))
        return 0;
    volatile uint32_t *pt = RECURSIVE_PT(pdi);
    uint32_t entry = pt[pti];
    if (!(entry & PG_PRESENT))
        return 0;
    pt[pti] = 0;
    invlpg(virt);
    return entry & FRAME_MASK;
}

uint32_t paging_virt_to_phys(uintptr_t virt)
{
    uint32_t pdi = virt >> 22;
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (!(RECURSIVE_PD[pdi] & PG_PRESENT))
        return 0;
    uint32_t entry = RECURSIVE_PT(pdi)[pti];
    if (!(entry & PG_PRESENT))
        return 0;
    return (entry & FRAME_MASK) | (virt & 0xFFF);
}
