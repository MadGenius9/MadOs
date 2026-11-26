#include "shell.h"

#include <stddef.h>
#include <stdint.h>

#include "heap.h"
#include "io.h"
#include "kernel.h"
#include "keyboard.h"
#include "kprintf.h"
#include "kstring.h"
#include "pit.h"
#include "pmm.h"
#include "vga.h"

#define LINE_MAX 128

static size_t read_line(char *buf, size_t max)
{
    size_t len = 0;
    for (;;) {
        char c = keyboard_getchar();
        if (c == '\n') {
            kputc('\n');
            buf[len] = '\0';
            return len;
        }
        if (c == '\b') {
            if (len > 0) {
                len--;
                kputc('\b');
            }
            continue;
        }
        if ((unsigned char)c < 32 || c == 127)
            continue; /* ignore other control keys */
        if (len < max - 1) {
            buf[len++] = c;
            kputc(c);
        }
    }
}

static void cmd_help(void)
{
    kputs("Commands:\n"
          "  help           this list\n"
          "  clear          clear the screen\n"
          "  echo TEXT      print TEXT\n"
          "  mem            physical memory usage\n"
          "  heap           kernel heap usage\n"
          "  memtest        allocate, verify and free heap memory\n"
          "  uptime         time since boot\n"
          "  crash null     read address 0 (page fault)\n"
          "  crash div      divide by zero (CPU exception)\n"
          "  reboot         reset the machine\n");
}

static void cmd_mem(void)
{
    unsigned usable = pmm_usable_frames();
    unsigned free = pmm_free_frames();
    kprintf("Physical memory: %u KiB usable, %u KiB used, %u KiB free (%u/%u frames free)\n", usable * 4,
            (usable - free) * 4, free * 4, free, usable);
}

static void cmd_heap(void)
{
    struct heap_stats s;
    heap_get_stats(&s);
    kprintf("Heap: %u KiB mapped, %u bytes used in %u blocks, %u bytes free in %u blocks\n",
            (unsigned)(s.mapped_bytes / 1024), (unsigned)s.used_bytes, (unsigned)s.used_blocks,
            (unsigned)s.free_bytes, (unsigned)s.free_blocks);
}

static void cmd_memtest(void)
{
    enum { COUNT = 64 };
    uint8_t *blocks[COUNT];
    size_t sizes[COUNT];
    uint32_t seed = pit_ticks() * 2654435761u + 1;

    for (int i = 0; i < COUNT; i++) {
        seed = seed * 1103515245u + 12345u;
        sizes[i] = 1 + (seed >> 8) % 8192;
        blocks[i] = kmalloc(sizes[i]);
        if (!blocks[i]) {
            kprintf("memtest: FAILED, kmalloc(%u) returned NULL\n", (unsigned)sizes[i]);
            return;
        }
        memset(blocks[i], (uint8_t)i, sizes[i]);
    }
    size_t big_size = 1u << 20;
    uint8_t *big = kmalloc(big_size);
    if (!big) {
        kputs("memtest: FAILED, could not allocate 1 MiB\n");
        return;
    }
    memset(big, 0xA5, big_size);

    for (int i = 0; i < COUNT; i++) {
        for (size_t j = 0; j < sizes[i]; j++) {
            if (blocks[i][j] != (uint8_t)i) {
                kprintf("memtest: FAILED, block %d corrupted at byte %u\n", i, (unsigned)j);
                return;
            }
        }
    }
    for (size_t j = 0; j < big_size; j++) {
        if (big[j] != 0xA5) {
            kprintf("memtest: FAILED, 1 MiB block corrupted at byte %u\n", (unsigned)j);
            return;
        }
    }

    for (int i = 1; i < COUNT; i += 2)
        kfree(blocks[i]);
    kfree(big);
    for (int i = 0; i < COUNT; i += 2)
        kfree(blocks[i]);

    struct heap_stats s;
    heap_get_stats(&s);
    if (s.used_blocks != 0 || s.free_blocks != 1) {
        kprintf("memtest: FAILED, heap not fully coalesced (%u used, %u free blocks)\n",
                (unsigned)s.used_blocks, (unsigned)s.free_blocks);
        return;
    }
    kprintf("memtest: OK (%d blocks + 1 MiB written, verified and freed; heap has %u KiB mapped)\n", COUNT,
            (unsigned)(s.mapped_bytes / 1024));
}

static void cmd_uptime(void)
{
    unsigned ticks = pit_ticks();
    unsigned hz = pit_hz();
    unsigned secs = ticks / hz;
    kprintf("Up %u.%02u s (%u ticks at %u Hz)\n", secs, (ticks % hz) * 100 / hz, ticks, hz);
}

static void cmd_crash(const char *what)
{
    if (strcmp(what, "null") == 0) {
        kputs("Reading address 0...\n");
        __asm__ volatile("movl 0, %%eax" : : : "eax", "memory");
    } else if (strcmp(what, "div") == 0) {
        kputs("Dividing by zero...\n");
        __asm__ volatile("xorl %%ecx, %%ecx; divl %%ecx" : : : "eax", "ecx", "edx");
    } else {
        kputs("usage: crash null|div\n");
    }
}

__attribute__((noreturn)) static void cmd_reboot(void)
{
    kputs("Rebooting...\n");
    __asm__ volatile("cli");
    while (inb(0x64) & 0x02) {
        /* wait for the keyboard controller's input buffer to empty */
    }
    outb(0x64, 0xFE); /* pulse the CPU reset line */

    /* Fallback: triple fault with an empty IDT. */
    struct {
        uint16_t limit;
        uint32_t base;
    } __attribute__((packed)) empty_idt = {0, 0};
    __asm__ volatile("lidt %0; int $3" : : "m"(empty_idt));
    for (;;)
        __asm__ volatile("hlt");
}

static void execute(char *line)
{
    while (*line == ' ')
        line++;
    char *args = line;
    while (*args && *args != ' ')
        args++;
    if (*args) {
        *args++ = '\0';
        while (*args == ' ')
            args++;
    }

    if (*line == '\0')
        return;
    if (strcmp(line, "help") == 0)
        cmd_help();
    else if (strcmp(line, "clear") == 0)
        vga_clear();
    else if (strcmp(line, "echo") == 0)
        kprintf("%s\n", args);
    else if (strcmp(line, "mem") == 0)
        cmd_mem();
    else if (strcmp(line, "heap") == 0)
        cmd_heap();
    else if (strcmp(line, "memtest") == 0)
        cmd_memtest();
    else if (strcmp(line, "uptime") == 0)
        cmd_uptime();
    else if (strcmp(line, "crash") == 0)
        cmd_crash(args);
    else if (strcmp(line, "reboot") == 0)
        cmd_reboot();
    else
        kprintf("%s: command not found (try 'help')\n", line);
}

void shell_run(void)
{
    char line[LINE_MAX];
    kputs("\nType 'help' for a list of commands.\n");
    for (;;) {
        vga_set_color(VGA_LIGHT_GREEN, VGA_BLACK);
        kputs(KERNEL_NAME);
        vga_set_color(VGA_LIGHT_GREY, VGA_BLACK);
        kputs("> ");
        read_line(line, sizeof(line));
        execute(line);
    }
}
