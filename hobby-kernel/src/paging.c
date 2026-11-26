#include "paging.h"

#include "idt.h"
#include "kernel.h"
#include "kprintf.h"
#include "kstring.h"
#include "pmm.h"
#include "sync.h"
#include "task.h"
#include "vga.h"

#define ENTRIES 1024
#define IDENTITY_MAP_BYTES (8u << 20)
#define IDENTITY_TABLES (IDENTITY_MAP_BYTES / (ENTRIES * PAGE_SIZE))
#define RECURSIVE_SLOT 1023u
#define SCRATCH_SLOT 1022u
#define SCRATCH_VIRT 0xFFBFF000u /* last page covered by directory slot 1022 */
#define RECURSIVE_PD ((volatile uint32_t *)0xFFFFF000u)
#define RECURSIVE_PT(pdi) ((volatile uint32_t *)(0xFFC00000u + ((uint32_t)(pdi) << 12)))
#define FRAME_MASK 0xFFFFF000u

#define PDE_INDEX(addr) ((uint32_t)(addr) >> 22)
#define USER_PDE_FIRST PDE_INDEX(USER_BASE)
#define USER_PDE_END PDE_INDEX(USER_TOP)

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
    const char *what = (r->err_code & 1) ? "protection violation" : "page not present";
    const char *access = (r->err_code & 2) ? "write" : "read";
    if (regs_from_user(r))
        task_kill_current("Page fault at 0x%08x (%s, %s) eip=0x%08x", (unsigned)addr, what, access,
                          (unsigned)r->eip);

    vga_set_color(VGA_LIGHT_RED, VGA_BLACK);
    kprintf("\nPage fault at address 0x%08x (eip=0x%08x)\n", (unsigned)addr, (unsigned)r->eip);
    kprintf("  %s, %s access, %s mode%s\n", what, access, (r->err_code & 4) ? "user" : "kernel",
            (r->err_code & 16) ? ", instruction fetch" : "");
    panic("unhandled page fault");
}

/* Makes sure directory slot pdi has a page table (current address space). */
static bool ensure_table(uint32_t pdi, bool user)
{
    if (RECURSIVE_PD[pdi] & PG_PRESENT)
        return true;
    uint32_t table = pmm_alloc_frame();
    if (!table)
        return false;
    RECURSIVE_PD[pdi] = table | PG_PRESENT | PG_RW | (user ? PG_USER : 0);
    volatile uint32_t *pt = RECURSIVE_PT(pdi);
    invlpg((uintptr_t)pt);
    for (uint32_t i = 0; i < ENTRIES; i++)
        pt[i] = 0;
    return true;
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

    /* Kernel page tables that every future address space will share. */
    for (uint32_t pdi = PDE_INDEX(KERNEL_HEAP_BASE); pdi < PDE_INDEX(KERNEL_HEAP_LIMIT); pdi++) {
        if (!ensure_table(pdi, false))
            panic("out of memory for kernel heap page tables");
    }
    if (!ensure_table(SCRATCH_SLOT, false))
        panic("out of memory for the scratch page table");
}

bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags)
{
    uint32_t pdi = PDE_INDEX(virt);
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (pdi == RECURSIVE_SLOT)
        return false;

    uint32_t irq = irq_save();
    bool ok = ensure_table(pdi, flags & PG_USER);
    volatile uint32_t *pt = RECURSIVE_PT(pdi);
    if (ok && (pt[pti] & PG_PRESENT))
        ok = false;
    if (ok) {
        pt[pti] = (phys & FRAME_MASK) | (flags & 0xFFF) | PG_PRESENT;
        invlpg(virt);
    }
    irq_restore(irq);
    return ok;
}

uint32_t paging_unmap_page(uintptr_t virt)
{
    uint32_t pdi = PDE_INDEX(virt);
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (pdi == RECURSIVE_SLOT || !(RECURSIVE_PD[pdi] & PG_PRESENT))
        return 0;
    uint32_t irq = irq_save();
    volatile uint32_t *pt = RECURSIVE_PT(pdi);
    uint32_t entry = pt[pti];
    if (entry & PG_PRESENT) {
        pt[pti] = 0;
        invlpg(virt);
    }
    irq_restore(irq);
    return (entry & PG_PRESENT) ? entry & FRAME_MASK : 0;
}

uint32_t paging_virt_to_phys(uintptr_t virt)
{
    uint32_t pdi = PDE_INDEX(virt);
    uint32_t pti = (virt >> 12) & 0x3FF;
    if (!(RECURSIVE_PD[pdi] & PG_PRESENT))
        return 0;
    uint32_t entry = RECURSIVE_PT(pdi)[pti];
    if (!(entry & PG_PRESENT))
        return 0;
    return (entry & FRAME_MASK) | (virt & 0xFFF);
}

uint32_t paging_kernel_directory(void)
{
    return (uint32_t)(uintptr_t)page_directory;
}

uint32_t paging_current_directory(void)
{
    uint32_t cr3;
    __asm__ volatile("mov %%cr3, %0" : "=r"(cr3));
    return cr3 & FRAME_MASK;
}

void paging_switch_directory(uint32_t pd_phys)
{
    if (paging_current_directory() != pd_phys)
        __asm__ volatile("mov %0, %%cr3" : : "r"(pd_phys) : "memory");
}

uint32_t paging_create_address_space(void)
{
    uint32_t pd = pmm_alloc_frame();
    if (!pd)
        return 0;

    uint32_t irq = irq_save();
    volatile uint32_t *scratch_pt = RECURSIVE_PT(SCRATCH_SLOT);
    scratch_pt[ENTRIES - 1] = pd | PG_PRESENT | PG_RW;
    invlpg(SCRATCH_VIRT);
    volatile uint32_t *new_pd = (volatile uint32_t *)SCRATCH_VIRT;
    for (uint32_t i = 0; i < ENTRIES; i++) {
        bool user_slot = i >= USER_PDE_FIRST && i < USER_PDE_END;
        new_pd[i] = user_slot ? 0 : RECURSIVE_PD[i];
    }
    new_pd[RECURSIVE_SLOT] = pd | PG_PRESENT | PG_RW;
    scratch_pt[ENTRIES - 1] = 0;
    invlpg(SCRATCH_VIRT);
    irq_restore(irq);
    return pd;
}

void paging_destroy_address_space(uint32_t pd_phys)
{
    uint32_t irq = irq_save();
    uint32_t previous = paging_current_directory();
    if (previous == pd_phys)
        panic("destroying the active address space");

    /* Switch in temporarily so the recursive mapping shows its tables; the
       kernel half is identical, so the code keeps running. */
    paging_switch_directory(pd_phys);
    for (uint32_t pdi = USER_PDE_FIRST; pdi < USER_PDE_END; pdi++) {
        uint32_t pde = RECURSIVE_PD[pdi];
        if (!(pde & PG_PRESENT))
            continue;
        volatile uint32_t *pt = RECURSIVE_PT(pdi);
        for (uint32_t i = 0; i < ENTRIES; i++) {
            if (pt[i] & PG_PRESENT)
                pmm_free_frame(pt[i] & FRAME_MASK);
        }
        pmm_free_frame(pde & FRAME_MASK);
    }
    paging_switch_directory(previous);
    pmm_free_frame(pd_phys);
    irq_restore(irq);
}

bool paging_user_range_ok(uintptr_t virt, size_t len)
{
    if (len == 0)
        return true;
    if (virt < USER_BASE || virt >= USER_TOP || len > USER_TOP - virt)
        return false;
    for (uintptr_t page = virt & FRAME_MASK; page < virt + len; page += PAGE_SIZE) {
        uint32_t pdi = PDE_INDEX(page);
        uint32_t pti = (page >> 12) & 0x3FF;
        if ((RECURSIVE_PD[pdi] & (PG_PRESENT | PG_USER)) != (PG_PRESENT | PG_USER))
            return false;
        if ((RECURSIVE_PT(pdi)[pti] & (PG_PRESENT | PG_USER)) != (PG_PRESENT | PG_USER))
            return false;
    }
    return true;
}
