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

/* Access byte: present | ring | descriptor type | executable | rw. */
#define ACCESS_KERNEL_CODE 0x9A
#define ACCESS_KERNEL_DATA 0x92
#define ACCESS_USER_CODE 0xFA
#define ACCESS_USER_DATA 0xF2
/* Flags: 4 KiB granularity, 32-bit protected mode. */
#define FLAGS_4K_32BIT 0xC0

static struct gdt_entry gdt[5];
static struct gdt_ptr gdt_pointer;

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

    gdt_pointer.limit = sizeof(gdt) - 1;
    gdt_pointer.base = (uint32_t)(uintptr_t)&gdt;
    gdt_flush((uint32_t)(uintptr_t)&gdt_pointer);
}
