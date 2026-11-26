#include "task.h"

#include "elf.h"
#include "gdt.h"
#include "heap.h"
#include "kprintf.h"
#include "kstring.h"
#include "paging.h"
#include "pit.h"
#include "pmm.h"
#include "sync.h"

#define KERNEL_STACK_SIZE 16384u
#define USER_STACK_SIZE 16384u
#define TIME_SLICE_TICKS 5 /* 50 ms at 100 Hz */
#define EFLAGS_DEFAULT 0x202u /* IF set, reserved bit 1 set */

static struct task *task_list; /* all tasks, in creation order */
static struct task *current;
static struct task *idle_task;
static uint32_t next_pid;
static int slice_left;
static bool resched;

static void add_task(struct task *t)
{
    uint32_t irq = irq_save();
    t->pid = next_pid++;
    t->next = NULL;
    struct task **p = &task_list;
    while (*p)
        p = &(*p)->next;
    *p = t;
    irq_restore(irq);
}

static void set_name(struct task *t, const char *name)
{
    size_t n = strlen(name);
    if (n >= sizeof(t->name))
        n = sizeof(t->name) - 1;
    memcpy(t->name, name, n);
    t->name[n] = '\0';
}

static struct task *alloc_task(const char *name)
{
    struct task *t = kcalloc(1, sizeof(*t));
    if (!t)
        return NULL;
    t->kstack = kmalloc(KERNEL_STACK_SIZE);
    if (!t->kstack) {
        kfree(t);
        return NULL;
    }
    t->kstack_top = (uint32_t)(uintptr_t)t->kstack + KERNEL_STACK_SIZE;
    set_name(t, name);
    t->state = TASK_READY;
    return t;
}

/* Builds the frame that the first switch to this task "returns" through. */
static struct regs *initial_frame(struct task *t, bool user, uint32_t eip, uint32_t user_esp)
{
    struct regs *r = (struct regs *)(t->kstack_top - sizeof(struct regs));
    memset(r, 0, sizeof(*r));
    uint32_t data = user ? USER_DATA_SELECTOR : GDT_KERNEL_DATA;
    r->gs = r->fs = r->es = r->ds = data;
    r->cs = user ? USER_CODE_SELECTOR : GDT_KERNEL_CODE;
    r->eip = eip;
    r->eflags = EFLAGS_DEFAULT;
    r->useresp = user_esp; /* only popped by iret when returning to ring 3 */
    r->ss = data;
    return r;
}

static void kernel_thread_start(void)
{
    current->entry(current->arg);
    task_exit(0);
}

static void idle_loop(void *arg)
{
    (void)arg;
    for (;;)
        __asm__ volatile("sti; hlt");
}

static void yield_handler(struct regs *r)
{
    (void)r;
    resched = true;
}

void tasking_init(const char *name)
{
    struct task *boot = kcalloc(1, sizeof(*boot));
    if (!boot)
        panic("out of memory creating the boot task");
    set_name(boot, name);
    boot->state = TASK_RUNNING;
    boot->page_dir = paging_kernel_directory();
    add_task(boot);
    current = boot;
    slice_left = TIME_SLICE_TICKS;

    interrupt_register_handler(YIELD_VECTOR, yield_handler);
    idle_task = task_create_kernel("idle", idle_loop, NULL);
    if (!idle_task)
        panic("out of memory creating the idle task");
}

struct task *task_create_kernel(const char *name, void (*entry)(void *), void *arg)
{
    struct task *t = alloc_task(name);
    if (!t)
        return NULL;
    t->entry = entry;
    t->arg = arg;
    t->page_dir = paging_kernel_directory();
    t->regs = initial_frame(t, false, (uint32_t)(uintptr_t)kernel_thread_start, 0);
    add_task(t);
    return t;
}

static bool map_zeroed(uintptr_t start, size_t bytes)
{
    for (size_t off = 0; off < bytes; off += PAGE_SIZE) {
        uint32_t frame = pmm_alloc_frame();
        if (!frame)
            return false;
        if (!paging_map_page(start + off, frame, PG_RW | PG_USER)) {
            pmm_free_frame(frame);
            return false;
        }
        memset((void *)(start + off), 0, PAGE_SIZE);
    }
    return true;
}

/* Copies argv onto the top of the (active) user stack:
       [strings ...][pad][argv[0] .. argv[argc-1], NULL]  <- returned address
   The process starts with ECX = argc and EDX = argv (see user/lib/crt0.asm). */
static uint32_t push_args(int argc, const char *const *argv)
{
    uint32_t sp = USER_TOP;
    uint32_t pointers[TASK_MAX_ARGS + 1];
    for (int i = argc - 1; i >= 0; i--) {
        size_t len = strlen(argv[i]) + 1;
        sp -= (uint32_t)len;
        memcpy((void *)(uintptr_t)sp, argv[i], len);
        pointers[i] = sp;
    }
    pointers[argc] = 0;
    sp &= ~15u;
    sp -= (uint32_t)(argc + 1) * 4;
    sp &= ~15u;
    memcpy((void *)(uintptr_t)sp, pointers, (size_t)(argc + 1) * 4);
    return sp;
}

struct task *task_create_user(const char *name, const uint8_t *image, size_t size, int argc,
                              const char *const *argv, const char **error)
{
    const char *err = NULL;
    size_t arg_bytes = 0;
    for (int i = 0; i < argc; i++)
        arg_bytes += strlen(argv[i]) + 1;
    if (argc < 0 || argc > TASK_MAX_ARGS || arg_bytes > TASK_MAX_ARG_BYTES) {
        *error = "too many or too long arguments";
        return NULL;
    }
    struct task *t = alloc_task(name);
    if (!t) {
        *error = "out of memory";
        return NULL;
    }
    t->user = true;
    t->page_dir = paging_create_address_space();
    if (!t->page_dir) {
        kfree(t->kstack);
        kfree(t);
        *error = "out of memory";
        return NULL;
    }

    /* Fill the new address space while it is active; the kernel half (and
       so `image` and `argv`) stays visible. Interrupts stay off so no other
       task runs with this page directory loaded. */
    uint32_t irq = irq_save();
    uint32_t previous = paging_current_directory();
    paging_switch_directory(t->page_dir);
    uint32_t entry = 0;
    uint32_t stack_bottom = USER_TOP - USER_STACK_SIZE;
    err = elf_load(image, size, stack_bottom, &entry);
    if (!err && !map_zeroed(stack_bottom, USER_STACK_SIZE))
        err = "out of memory";
    uint32_t user_sp = err ? 0 : push_args(argc, argv);
    paging_switch_directory(previous);
    irq_restore(irq);

    if (err) {
        paging_destroy_address_space(t->page_dir);
        kfree(t->kstack);
        kfree(t);
        *error = err;
        return NULL;
    }
    t->regs = initial_frame(t, true, entry, user_sp);
    t->regs->ecx = (uint32_t)argc;
    t->regs->edx = user_sp;
    add_task(t);
    return t;
}

struct task *task_current(void)
{
    return current;
}

struct task *task_find(uint32_t pid)
{
    uint32_t irq = irq_save();
    struct task *t = task_list;
    while (t && t->pid != pid)
        t = t->next;
    irq_restore(irq);
    return t;
}

void task_yield(void)
{
    __asm__ volatile("int %0" : : "i"(YIELD_VECTOR) : "memory");
}

void task_sleep_ms(uint32_t ms)
{
    uint32_t hz = pit_hz();
    uint32_t ticks = (ms * hz + 999) / 1000;
    uint32_t irq = irq_save();
    current->wake_tick = pit_ticks() + (ticks ? ticks : 1);
    current->state = TASK_SLEEPING;
    task_yield();
    irq_restore(irq);
}

void task_block(void)
{
    current->state = TASK_BLOCKED;
    task_yield();
}

void task_wake(struct task *t)
{
    uint32_t irq = irq_save();
    if (t->state == TASK_BLOCKED || t->state == TASK_SLEEPING) {
        t->state = TASK_READY;
        resched = true; /* run the woken task (e.g. the shell after a key press) soon */
    }
    irq_restore(irq);
}

static void mark_dead(struct task *t, int code)
{
    t->state = TASK_DEAD;
    t->exit_code = code;
}

void task_exit(int code)
{
    __asm__ volatile("cli");
    if (current->user)
        kprintf("[process %u (%s) exited with code %d]\n", (unsigned)current->pid, current->name, code);
    mark_dead(current, code);
    task_yield();
    panic("dead task %u was scheduled again", (unsigned)current->pid);
}

void task_kill_current(const char *fmt, ...)
{
    __asm__ volatile("cli");
    kprintf("[process %u (%s) killed: ", (unsigned)current->pid, current->name);
    va_list ap;
    va_start(ap, fmt);
    kvprintf(fmt, ap);
    va_end(ap);
    kputs("]\n");
    mark_dead(current, -1);
    task_yield();
    panic("killed task %u was scheduled again", (unsigned)current->pid);
}

bool task_kill(uint32_t pid)
{
    uint32_t irq = irq_save();
    struct task *t = task_find(pid);
    bool ok = t && t != current && t != idle_task && t != task_list && t->state != TASK_DEAD;
    if (ok) {
        mark_dead(t, -1);
        kprintf("[process %u (%s) killed]\n", (unsigned)t->pid, t->name);
    }
    irq_restore(irq);
    return ok;
}

void task_for_each(void (*fn)(const struct task *t, void *ctx), void *ctx)
{
    uint32_t irq = irq_save();
    for (struct task *t = task_list; t; t = t->next) {
        if (t->state != TASK_DEAD)
            fn(t, ctx);
    }
    irq_restore(irq);
}

const char *task_state_name(enum task_state state)
{
    switch (state) {
    case TASK_READY:
        return "ready";
    case TASK_RUNNING:
        return "running";
    case TASK_SLEEPING:
        return "sleeping";
    case TASK_BLOCKED:
        return "blocked";
    case TASK_DEAD:
        return "dead";
    }
    return "?";
}

void task_tick(void)
{
    if (!current)
        return;
    current->cpu_ticks++;
    uint32_t now = pit_ticks();
    for (struct task *t = task_list; t; t = t->next) {
        if (t->state == TASK_SLEEPING && (int32_t)(now - t->wake_tick) >= 0) {
            t->state = TASK_READY;
            if (current == idle_task)
                resched = true;
        }
    }
    if (--slice_left <= 0)
        resched = true;
}

bool task_need_resched(void)
{
    return resched;
}

/* Frees tasks that died, except the one whose stack we are still on. */
static void reap(void)
{
    struct task **p = &task_list;
    while (*p) {
        struct task *t = *p;
        if (t->state == TASK_DEAD && t != current) {
            *p = t->next;
            if (t->user)
                paging_destroy_address_space(t->page_dir);
            kfree(t->kstack);
            kfree(t);
        } else {
            p = &t->next;
        }
    }
}

static struct task *pick_next(void)
{
    struct task *t = current;
    do {
        t = t->next ? t->next : task_list;
        if (t != idle_task && t->state == TASK_READY)
            return t;
    } while (t != current);
    return idle_task;
}

struct regs *task_switch(struct regs *r)
{
    resched = false;
    if (!current)
        return r;
    current->regs = r;
    if (current->state == TASK_RUNNING)
        current->state = TASK_READY;

    struct task *next = pick_next();
    if (next == idle_task && current->state == TASK_READY && current != idle_task)
        next = current; /* nothing else to run: keep going */

    reap();
    next->state = TASK_RUNNING;
    slice_left = TIME_SLICE_TICKS;
    if (next != current) {
        paging_switch_directory(next->page_dir);
        if (next->kstack)
            tss_set_kernel_stack(next->kstack_top);
        current = next;
    }
    return next->regs;
}
