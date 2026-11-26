#include "syscall.h"

#include <stdint.h>

#include "fs.h"
#include "idt.h"
#include "kprintf.h"
#include "kstring.h"
#include "paging.h"
#include "pmm.h"
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

#define ERR(e) ((uint32_t)-(e))

/* Copies a NUL-terminated string from user memory, checking each page
   before touching it. Returns its length, or a negative errno. */
static int copy_user_string(uint32_t uaddr, char *out, size_t max)
{
    for (size_t i = 0; i < max; i++) {
        uint32_t a = uaddr + (uint32_t)i;
        if ((i == 0 || (a & (PAGE_SIZE - 1)) == 0) && !paging_user_range_ok(a, 1))
            return -EFAULT;
        out[i] = *(const char *)(uintptr_t)a;
        if (out[i] == '\0')
            return (int)i;
    }
    return -ENAMETOOLONG;
}

static struct open_file *get_file(uint32_t fd)
{
    struct task *t = task_current();
    if (fd >= TASK_MAX_FILES || !t->files[fd].node)
        return NULL;
    return &t->files[fd];
}

static uint32_t sys_open(uint32_t upath)
{
    char path[FS_PATH_MAX];
    int len = copy_user_string(upath, path, sizeof(path));
    if (len < 0)
        return (uint32_t)len;
    const struct fs_node *node = fs_lookup(path);
    if (!node)
        return ERR(ENOENT);
    if (node->is_dir)
        return ERR(EISDIR);
    struct task *t = task_current();
    for (uint32_t fd = 0; fd < TASK_MAX_FILES; fd++) {
        if (!t->files[fd].node) {
            t->files[fd].node = node;
            t->files[fd].offset = 0;
            return fd;
        }
    }
    return ERR(EMFILE);
}

static uint32_t sys_read(uint32_t fd, uint32_t buf, uint32_t len)
{
    struct open_file *f = get_file(fd);
    if (!f)
        return ERR(EBADF);
    if (!paging_user_range_ok(buf, len))
        return ERR(EFAULT);
    uint32_t left = f->node->size - f->offset;
    uint32_t n = len < left ? len : left;
    memcpy((void *)(uintptr_t)buf, f->node->data + f->offset, n);
    f->offset += n;
    return n;
}

static uint32_t sys_close(uint32_t fd)
{
    struct open_file *f = get_file(fd);
    if (!f)
        return ERR(EBADF);
    f->node = NULL;
    return 0;
}

static uint32_t sys_readdir(uint32_t upath, uint32_t index, uint32_t buf, uint32_t len)
{
    char path[FS_PATH_MAX];
    int plen = copy_user_string(upath, path, sizeof(path));
    if (plen < 0)
        return (uint32_t)plen;
    const struct fs_node *dir = fs_lookup(path);
    if (!dir)
        return ERR(ENOENT);
    if (!dir->is_dir)
        return ERR(ENOTDIR);
    const struct fs_node *child = fs_child(dir, index);
    if (!child)
        return 0;
    const char *name = fs_basename(child);
    size_t nlen = strlen(name);
    size_t need = nlen + (child->is_dir ? 1 : 0) + 1;
    if (len < need)
        return ERR(ERANGE);
    if (!paging_user_range_ok(buf, need))
        return ERR(EFAULT);
    char *out = (char *)(uintptr_t)buf;
    memcpy(out, name, nlen);
    if (child->is_dir)
        out[nlen++] = '/';
    out[nlen] = '\0';
    return (uint32_t)nlen;
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
    case SYS_OPEN:
        r->eax = sys_open(r->ebx);
        break;
    case SYS_READ:
        r->eax = sys_read(r->ebx, r->ecx, r->edx);
        break;
    case SYS_CLOSE:
        r->eax = sys_close(r->ebx);
        break;
    case SYS_READDIR:
        r->eax = sys_readdir(r->ebx, r->ecx, r->edx, r->esi);
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
