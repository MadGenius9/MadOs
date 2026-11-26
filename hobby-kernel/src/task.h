#ifndef TASK_H
#define TASK_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#include "fs.h"
#include "idt.h"

#define TASK_MAX_FILES 8
#define TASK_MAX_ARGS 16
#define TASK_MAX_ARG_BYTES 1024

struct open_file {
    const struct fs_node *node; /* NULL = free slot */
    uint32_t offset;
};

/* Preemptive round-robin multitasking on one CPU.

   Every task has its own kernel stack. A task is switched out only inside
   an interrupt: the timer (its time slice ran out) or the yield vector
   (it slept, blocked or exited). Its registers are then the interrupt frame
   on its kernel stack, and resuming it means returning from that frame.
   User tasks also have their own address space and run in ring 3. */

enum task_state {
    TASK_READY,
    TASK_RUNNING,
    TASK_SLEEPING,
    TASK_BLOCKED,
    TASK_DEAD,
};

struct task {
    uint32_t pid;
    char name[16];
    enum task_state state;
    bool user;
    struct regs *regs;   /* saved frame while not running */
    void *kstack;        /* kmalloc'd kernel stack (NULL for the boot task) */
    uint32_t kstack_top;
    uint32_t page_dir;   /* physical address of the page directory */
    uint32_t wake_tick;  /* for TASK_SLEEPING */
    uint32_t cpu_ticks;  /* timer ticks spent running */
    int exit_code;
    void (*entry)(void *);
    void *arg;
    struct open_file files[TASK_MAX_FILES]; /* user processes' file descriptors */
    struct task *next;
};

/* Turns the code that is running (kernel_main, on the boot stack) into
   task 0 named `name`, and creates the idle task. */
void tasking_init(const char *name);

struct task *task_create_kernel(const char *name, void (*entry)(void *), void *arg);

/* Creates a ring-3 process from an ELF executable, with a 16 KiB stack
   ending at USER_TOP that holds argv. Returns NULL and sets *error (a static
   message) when the image is invalid or memory runs out. */
struct task *task_create_user(const char *name, const uint8_t *image, size_t size, int argc,
                              const char *const *argv, const char **error);

struct task *task_current(void);
struct task *task_find(uint32_t pid);

void task_yield(void);
void task_sleep_ms(uint32_t ms);

/* Blocks the current task until task_wake. Call with interrupts disabled,
   after checking the condition, so a wake-up cannot be missed. */
void task_block(void);
void task_wake(struct task *t);

__attribute__((noreturn)) void task_exit(int code);

/* Kills the current task (a user process that faulted) with a message. */
__attribute__((noreturn, format(printf, 1, 2))) void task_kill_current(const char *fmt, ...);

/* Kills another task. Fails for the boot task, the idle task and itself. */
bool task_kill(uint32_t pid);

/* Calls fn for every task (interrupts disabled meanwhile). */
void task_for_each(void (*fn)(const struct task *t, void *ctx), void *ctx);

const char *task_state_name(enum task_state state);

/* Hooks used by the timer and the interrupt dispatcher. */
void task_tick(void);
bool task_need_resched(void);
struct regs *task_switch(struct regs *r);

#endif
