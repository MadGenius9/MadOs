#include "gdt.h"

#include <stdint.h>

struct gdt_entry {
    uint16_t limit_low;
    uint16_t base_low;
    uint8_t base_mid;
    uint8_t access;
    uint8_t granularity; /* flags (high nibble) | limit bits 16-19 */
    uint8_t base_high;
} __attribute__((packed));

struct gdt_ptr {
    uint16_t limit;
    uint32_t base;
} __attribute__((packed));

struct tss {
    uint32_t prev_tss;
    uint32_t esp0, ss0;
    uint32_t esp1, ss1;
    uint32_t esp2, ss2;
    uint32_t cr3, eip, eflags;
    uint32_t eax, ecx, edx, ebx, esp, ebp, esi, edi;
    uint32_t es, cs, ss, ds, fs, gs, ldt;
    uint16_t trap;
    uint16_t iomap_base;
} __attribute__((packed));

/* Access byte: present | ring | descriptor type | executable | rw. */
#define ACCESS_KERNEL_CODE 0x9A
#define ACCESS_KERNEL_DATA 0x92
#define ACCESS_USER_CODE 0xFA
#define ACCESS_USER_DATA 0xF2
#define ACCESS_TSS 0x89 /* present, ring 0, 32-bit available TSS */
/* Flags: 4 KiB granularity, 32-bit protected mode. */
#define FLAGS_4K_32BIT 0xC0

static struct gdt_entry gdt[6];
static struct gdt_ptr gdt_pointer;
static struct tss tss;

extern void gdt_flush(uint32_t gdt_ptr_addr); /* gdt.asm */

static void set_entry(int i, uint32_t base, uint32_t limit, uint8_t access, uint8_t flags)
{
    gdt[i].base_low = base & 0xFFFF;
    gdt[i].base_mid = (base >> 16) & 0xFF;
    gdt[i].base_high = (base >> 24) & 0xFF;
    gdt[i].limit_low = limit & 0xFFFF;
    gdt[i].granularity = (uint8_t)(((limit >> 16) & 0x0F) | (flags & 0xF0));
    gdt[i].access = access;
}

void gdt_init(void)
{
    set_entry(0, 0, 0, 0, 0); /* the null descriptor */
    set_entry(1, 0, 0xFFFFF, ACCESS_KERNEL_CODE, FLAGS_4K_32BIT);
    set_entry(2, 0, 0xFFFFF, ACCESS_KERNEL_DATA, FLAGS_4K_32BIT);
    set_entry(3, 0, 0xFFFFF, ACCESS_USER_CODE, FLAGS_4K_32BIT);
    set_entry(4, 0, 0xFFFFF, ACCESS_USER_DATA, FLAGS_4K_32BIT);

    tss.ss0 = GDT_KERNEL_DATA;
    /* An I/O bitmap offset past the end of the TSS means "no bitmap": with
       IOPL 0, any port access from ring 3 raises a general protection fault. */
    tss.iomap_base = sizeof(tss);
    set_entry(5, (uint32_t)(uintptr_t)&tss, sizeof(tss) - 1, ACCESS_TSS, 0);

    gdt_pointer.limit = sizeof(gdt) - 1;
    gdt_pointer.base = (uint32_t)(uintptr_t)&gdt;
    gdt_flush((uint32_t)(uintptr_t)&gdt_pointer);
    __asm__ volatile("ltr %0" : : "r"((uint16_t)GDT_TSS));
}

void tss_set_kernel_stack(uint32_t esp0)
{
    tss.esp0 = esp0;
}
