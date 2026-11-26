#include "shell.h"

#include <stdbool.h>
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
#include "programs.h"
#include "task.h"
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
          "  ps             list tasks\n"
          "  run NAME [&]   run a user program (& = in the background)\n"
          "  run            list the user programs\n"
          "  kill PID       stop a task\n"
          "  spawn ticker   start a kernel thread that ticks 3 times\n"
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
    struct heap_stats before;
    heap_get_stats(&before); /* other tasks' allocations stay in use */

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
    if (s.used_blocks != before.used_blocks || s.used_bytes != before.used_bytes) {
        kprintf("memtest: FAILED, heap usage changed (%u bytes in %u blocks, was %u in %u)\n",
                (unsigned)s.used_bytes, (unsigned)s.used_blocks, (unsigned)before.used_bytes,
                (unsigned)before.used_blocks);
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

static void print_task(const struct task *t, void *ctx)
{
    (void)ctx;
    kprintf("%5u  %-8s  %-6s  %6u  %s\n", (unsigned)t->pid, task_state_name(t->state), t->user ? "user" : "kernel",
            (unsigned)t->cpu_ticks, t->name);
}

static void cmd_ps(void)
{
    kputs("  PID  STATE     TYPE    TICKS  NAME\n");
    task_for_each(print_task, NULL);
}

static uint32_t parse_uint(const char *s, bool *ok)
{
    uint32_t v = 0;
    *ok = *s != '\0';
    for (; *s; s++) {
        if (*s < '0' || *s > '9' || v > 100000000u) {
            *ok = false;
            return 0;
        }
        v = v * 10 + (uint32_t)(*s - '0');
    }
    return v;
}

/* Waits for a foreground task to finish; Ctrl+C kills it. */
static void wait_for(uint32_t pid)
{
    for (;;) {
        struct task *t = task_find(pid);
        if (!t || t->state == TASK_DEAD)
            return;
        char c;
        if (keyboard_trygetchar(&c) && c == 3) {
            kputs("^C\n");
            task_kill(pid);
            return;
        }
        task_sleep_ms(20);
    }
}

static void cmd_run(char *args)
{
    if (*args == '\0') {
        kputs("User programs:\n");
        for (size_t i = 0; i < user_program_count; i++)
            kprintf("  %-8s %s\n", user_programs[i].name, user_programs[i].description);
        return;
    }
    bool background = false;
    size_t len = strlen(args);
    while (len && args[len - 1] == ' ')
        args[--len] = '\0';
    if (len && args[len - 1] == '&') {
        background = true;
        args[--len] = '\0';
        while (len && args[len - 1] == ' ')
            args[--len] = '\0';
    }
    const struct user_program *prog = program_find(args);
    if (!prog) {
        kprintf("run: no program named '%s' (type 'run' for the list)\n", args);
        return;
    }
    struct task *t = task_create_user(prog->name, prog->start, (size_t)(prog->end - prog->start));
    if (!t) {
        kputs("run: out of memory\n");
        return;
    }
    uint32_t pid = t->pid;
    if (background)
        kprintf("[started process %u (%s) in the background]\n", (unsigned)pid, prog->name);
    else
        wait_for(pid);
}

static void cmd_kill(const char *args)
{
    bool ok;
    uint32_t pid = parse_uint(args, &ok);
    if (!ok) {
        kputs("usage: kill PID\n");
        return;
    }
    if (!task_kill(pid))
        kprintf("kill: cannot kill task %u\n", (unsigned)pid);
}

static void ticker(void *arg)
{
    (void)arg;
    for (int i = 1; i <= 3; i++) {
        task_sleep_ms(300);
        kprintf("[ticker: tick %d from kernel thread %u]\n", i, (unsigned)task_current()->pid);
    }
    kputs("[ticker: done]\n");
}

static void cmd_spawn(const char *what)
{
    if (strcmp(what, "ticker") != 0) {
        kputs("usage: spawn ticker\n");
        return;
    }
    struct task *t = task_create_kernel("ticker", ticker, NULL);
    if (t)
        kprintf("[started kernel thread %u (ticker)]\n", (unsigned)t->pid);
    else
        kputs("spawn: out of memory\n");
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
    else if (strcmp(line, "ps") == 0)
        cmd_ps();
    else if (strcmp(line, "run") == 0)
        cmd_run(args);
    else if (strcmp(line, "kill") == 0)
        cmd_kill(args);
    else if (strcmp(line, "spawn") == 0)
        cmd_spawn(args);
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
