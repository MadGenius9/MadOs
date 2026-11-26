/* kernel.c - kernel_main, called from boot.asm. Brings up each subsystem in
   roadmap order, then starts the shell. */

#include <stdbool.h>
#include <stdint.h>

#include "fs.h"
#include "gdt.h"
#include "heap.h"
#include "idt.h"
#include "kernel.h"
#include "keyboard.h"
#include "kprintf.h"
#include "multiboot1.h"
#include "multiboot2.h"
#include "paging.h"
#include "pic.h"
#include "pit.h"
#include "pmm.h"
#include "serial.h"
#include "shell.h"
#include "syscall.h"
#include "task.h"
#include "vga.h"

#define TIMER_HZ 100

static void ok(const char *what)
{
    vga_set_color(VGA_LIGHT_GREY, VGA_BLACK);
    kputs("[ ");
    vga_set_color(VGA_LIGHT_GREEN, VGA_BLACK);
    kputs("OK");
    vga_set_color(VGA_LIGHT_GREY, VGA_BLACK);
    kputs(" ] ");
    kputs(what);
}

/* Exercises allocation, reuse, splitting and coalescing; panics on failure. */
static void heap_self_test(void)
{
    uint8_t *a = kmalloc(100);
    uint8_t *b = kmalloc(200);
    uint8_t *c = kmalloc(3 * PAGE_SIZE); /* forces the heap to grow */
    if (!a || !b || !c)
        panic("heap self-test: allocation failed");
    if ((uintptr_t)a % 16 || (uintptr_t)b % 16 || (uintptr_t)c % 16)
        panic("heap self-test: misaligned allocation");
    for (int i = 0; i < 100; i++)
        a[i] = (uint8_t)i;

    kfree(b);
    uint8_t *d = kmalloc(150);
    if (d != b)
        panic("heap self-test: freed block was not reused");
    for (int i = 0; i < 100; i++) {
        if (a[i] != (uint8_t)i)
            panic("heap self-test: data corrupted");
    }
    kfree(a);
    kfree(c);
    kfree(d);

    struct heap_stats s;
    heap_get_stats(&s);
    if (s.used_blocks != 0 || s.free_blocks != 1)
        panic("heap self-test: blocks did not coalesce");
}

static void print_banner(void)
{
    vga_set_color(VGA_LIGHT_CYAN, VGA_BLACK);
    kputs(KERNEL_NAME " " KERNEL_VERSION);
    vga_set_color(VGA_LIGHT_GREY, VGA_BLACK);
    kputs(" - 32-bit x86\n\n");
}

__attribute__((noreturn)) void kernel_main(uint32_t magic, uint32_t mbi_phys)
{
    /* Phase 4: console */
    vga_init();
    bool have_serial = serial_init();
    print_banner();

    const struct mb2_info *mbi;
    const char *protocol;
    if (magic == MB2_BOOTLOADER_MAGIC) {
        mbi = (const struct mb2_info *)(uintptr_t)mbi_phys;
        protocol = "Multiboot2";
    } else if (magic == MB1_BOOTLOADER_MAGIC) {
        mbi = mb1_convert(mbi_phys);
        protocol = "Multiboot1";
    } else {
        panic("not loaded by a Multiboot bootloader (eax=0x%08x)", (unsigned)magic);
    }

    const struct mb2_tag_string *loader = (const struct mb2_tag_string *)mb2_find_tag(mbi, MB2_TAG_BOOTLOADER_NAME);
    ok(protocol);
    kprintf(" handoff (bootloader: %s)\n", loader ? loader->string : "unknown");
    ok("VGA text console");
    kputs(have_serial ? " + COM1 serial\n" : "\n");

    /* Phase 5: CPU tables */
    gdt_init();
    ok("GDT and TSS loaded (flat ring 0 and ring 3 segments)\n");
    idt_init();
    ok("IDT loaded (32 exception and 16 IRQ handlers)\n");

    /* Phase 6: interrupt controller and devices */
    pic_remap(IRQ_BASE, IRQ_BASE + 8);
    ok("PIC remapped to vectors 32-47\n");
    pit_init(TIMER_HZ);
    ok("");
    kprintf("PIT timer at %u Hz\n", TIMER_HZ);
    keyboard_init();
    ok("PS/2 keyboard\n");

    /* Phase 7: memory management. Read everything needed from the
       Multiboot2 info first: after paging only the first 8 MiB stay mapped. */
    pmm_init(mbi);
    uint32_t initrd_start = 0, initrd_end = 0;
    const struct mb2_tag_module *module = (const struct mb2_tag_module *)mb2_find_tag(mbi, MB2_TAG_MODULE);
    if (module) {
        initrd_start = module->mod_start;
        initrd_end = module->mod_end;
    }
    ok("");
    kprintf("Physical memory manager: %u MiB usable, %u frames free\n", (unsigned)(pmm_usable_frames() / 256),
            (unsigned)pmm_free_frames());
    paging_init();
    ok("Paging enabled (identity map 0-8 MiB, page 0 unmapped)\n");
    heap_init();
    heap_self_test();
    ok("Kernel heap at 0xD0000000 (self-test passed)\n");

    /* Phase 10: the initial ramdisk, mapped read-only into kernel space. This
       must happen before the first process: its page directory copies the
       kernel's page tables. */
    if (initrd_end > initrd_start) {
        uint32_t offset = initrd_start & (PAGE_SIZE - 1);
        size_t size = initrd_end - initrd_start;
        if (size + offset > INITRD_LIMIT - INITRD_BASE ||
            !paging_map_kernel_range(INITRD_BASE, initrd_start - offset, size + offset, PG_PRESENT))
            panic("cannot map the initial ramdisk (%u bytes)", (unsigned)size);
        size_t nodes = fs_init_tar((const uint8_t *)(uintptr_t)(INITRD_BASE + offset), size);
        if (nodes) {
            ok("");
            kprintf("Initial ramdisk: %u KiB, %u files and directories\n", (unsigned)(size / 1024),
                    (unsigned)nodes - 1);
        } else {
            kputs("[FAIL] Initial ramdisk is not a valid tar archive\n");
        }
    } else {
        kputs("[ -- ] No initial ramdisk: no programs to run\n");
    }

    /* Phase 8-9: tasks and user mode. This code becomes task 0, the shell. */
    tasking_init("shell");
    ok("Scheduler: preemptive round-robin, 50 ms time slices\n");
    syscall_init();
    ok("System calls on int 0x80; user programs run in ring 3\n");

    __asm__ volatile("sti");
    ok("Interrupts enabled\n");

    /* Marker for automated tests watching the serial port. */
    kputs("BOOT_OK\n");

    shell_run();
}
