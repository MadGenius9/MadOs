#ifndef SYSCALL_H
#define SYSCALL_H

/* System calls: `int 0x80` with the number in EAX and arguments in EBX, ECX,
   EDX. The result comes back in EAX; negative values are errors. Keep in
   sync with user/syscalls.inc. */
#define SYS_EXIT 0   /* exit(code)                 never returns          */
#define SYS_WRITE 1  /* write(buf, len)            bytes written, -EFAULT */
#define SYS_GETPID 2 /* getpid()                   pid                    */
#define SYS_SLEEP 3  /* sleep(milliseconds)        0                      */
#define SYS_YIELD 4  /* yield()                    0                      */

#define EFAULT 14
#define EINVAL 22
#define ENOSYS 38

#define SYS_WRITE_MAX 4096

void syscall_init(void);

#endif
