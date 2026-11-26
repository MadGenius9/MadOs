#ifndef SYSCALL_H
#define SYSCALL_H

/* System calls: `int 0x80` with the number in EAX and arguments in EBX, ECX,
   EDX, ESI. The result comes back in EAX; negative values are errors. Keep in
   sync with user/syscalls.inc. */
#define SYS_EXIT 0   /* exit(code)                 never returns          */
#define SYS_WRITE 1  /* write(buf, len)            bytes written, -EFAULT */
#define SYS_GETPID 2 /* getpid()                   pid                    */
#define SYS_SLEEP 3  /* sleep(milliseconds)        0                      */
#define SYS_YIELD 4  /* yield()                    0                      */
#define SYS_OPEN 5   /* open(path)                 fd                     */
#define SYS_READ 6   /* read(fd, buf, len)         bytes read, 0 = end    */
#define SYS_CLOSE 7  /* close(fd)                  0                      */
#define SYS_READDIR 8 /* readdir(path, index, buf, len): name length,
                         0 = no more entries; directories end in '/'     */

#define ENOENT 2
#define EBADF 9
#define EFAULT 14
#define ENOTDIR 20
#define EISDIR 21
#define EINVAL 22
#define EMFILE 24
#define ERANGE 34
#define ENAMETOOLONG 36
#define ENOSYS 38

#define SYS_WRITE_MAX 4096

void syscall_init(void);

#endif
