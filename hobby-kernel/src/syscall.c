#include "syscall.h"

#include <stdint.h>

#include "idt.h"
#include "kprintf.h"
#include "paging.h"
#include "task.h"

static uint32_t sys_write(uint32_t buf, uint32_t len)
{
    if (len > SYS_WRITE_MAX)
        return (uint32_t)-EINVAL;
    /* Never trust a user pointer: it must be mapped, user-accessible memory
       inside the user range, or a process could make the kernel read
       (and print) kernel memory. */
    if (!paging_user_range_ok(buf, len))
        return (uint32_t)-EFAULT;
    const char *p = (const char *)(uintptr_t)buf;
    for (uint32_t i = 0; i < len; i++)
        kputc(p[i]);
    return len;
}

static void syscall_handler(struct regs *r)
{
    switch (r->eax) {
    case SYS_EXIT:
        task_exit((int)r->ebx);
    case SYS_WRITE:
        r->eax = sys_write(r->ebx, r->ecx);
        break;
    case SYS_GETPID:
        r->eax = task_current()->pid;
        break;
    case SYS_SLEEP:
        task_sleep_ms(r->ebx);
        r->eax = 0;
        break;
    case SYS_YIELD:
        task_yield();
        r->eax = 0;
        break;
    default:
        r->eax = (uint32_t)-ENOSYS;
        break;
    }
}

void syscall_init(void)
{
    interrupt_register_handler(SYSCALL_VECTOR, syscall_handler);
}
