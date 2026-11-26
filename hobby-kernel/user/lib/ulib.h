#ifndef ULIB_H
#define ULIB_H

/* A tiny C library for hobby-kernel user programs: system call wrappers,
   a few string functions and printf. System call numbers and error codes
   match the kernel's src/syscall.h. */

#include <stdarg.h>
#include <stddef.h>

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

/* System calls. Negative results are -errno. */
__attribute__((noreturn)) void exit(int code);
int write(const void *buf, size_t len); /* to the console */
int getpid(void);
int sleep_ms(unsigned ms);
int yield(void);
int open(const char *path);
int read(int fd, void *buf, size_t len);
int close(int fd);
/* Name of the index-th entry of a directory (directories end in '/');
   returns the name length, 0 after the last entry. */
int readdir(const char *path, unsigned index, char *buf, size_t len);

/* Strings and memory. */
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);
void *memset(void *dst, int value, size_t n);
void *memcpy(void *dst, const void *src, size_t n);
const char *strerror(int err); /* accepts err or -err */

/* Output. printf supports %c %s %d %u %x %% with '-', '0' and a width. */
int puts(const char *s); /* adds a newline */
int printf(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap);

#endif
