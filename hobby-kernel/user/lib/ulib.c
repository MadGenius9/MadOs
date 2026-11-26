#include "ulib.h"

#define SYS_EXIT 0
#define SYS_WRITE 1
#define SYS_GETPID 2
#define SYS_SLEEP 3
#define SYS_YIELD 4
#define SYS_OPEN 5
#define SYS_READ 6
#define SYS_CLOSE 7
#define SYS_READDIR 8

static inline int syscall4(int n, int a, int b, int c, int d)
{
    int ret;
    __asm__ volatile("int $0x80" : "=a"(ret) : "a"(n), "b"(a), "c"(b), "d"(c), "S"(d) : "memory");
    return ret;
}

void exit(int code)
{
    syscall4(SYS_EXIT, code, 0, 0, 0);
    for (;;) {
    }
}

int write(const void *buf, size_t len)
{
    return syscall4(SYS_WRITE, (int)buf, (int)len, 0, 0);
}

int getpid(void)
{
    return syscall4(SYS_GETPID, 0, 0, 0, 0);
}

int sleep_ms(unsigned ms)
{
    return syscall4(SYS_SLEEP, (int)ms, 0, 0, 0);
}

int yield(void)
{
    return syscall4(SYS_YIELD, 0, 0, 0, 0);
}

int open(const char *path)
{
    return syscall4(SYS_OPEN, (int)path, 0, 0, 0);
}

int read(int fd, void *buf, size_t len)
{
    return syscall4(SYS_READ, fd, (int)buf, (int)len, 0);
}

int close(int fd)
{
    return syscall4(SYS_CLOSE, fd, 0, 0, 0);
}

int readdir(const char *path, unsigned index, char *buf, size_t len)
{
    return syscall4(SYS_READDIR, (int)path, (int)index, (int)buf, (int)len);
}

size_t strlen(const char *s)
{
    size_t n = 0;
    while (s[n])
        n++;
    return n;
}

int strcmp(const char *a, const char *b)
{
    while (*a && *a == *b) {
        a++;
        b++;
    }
    return (unsigned char)*a - (unsigned char)*b;
}

void *memset(void *dst, int value, size_t n)
{
    unsigned char *d = dst;
    while (n--)
        *d++ = (unsigned char)value;
    return dst;
}

void *memcpy(void *dst, const void *src, size_t n)
{
    unsigned char *d = dst;
    const unsigned char *s = src;
    while (n--)
        *d++ = *s++;
    return dst;
}

const char *strerror(int err)
{
    switch (err < 0 ? -err : err) {
    case ENOENT:
        return "no such file or directory";
    case EBADF:
        return "bad file descriptor";
    case EFAULT:
        return "bad address";
    case ENOTDIR:
        return "not a directory";
    case EISDIR:
        return "is a directory";
    case EINVAL:
        return "invalid argument";
    case EMFILE:
        return "too many open files";
    case ERANGE:
        return "result too large";
    case ENAMETOOLONG:
        return "name too long";
    case ENOSYS:
        return "function not implemented";
    default:
        return "unknown error";
    }
}

struct out {
    char *buf;
    size_t size;
    size_t len; /* characters produced, even past size */
};

static void put(struct out *o, char c)
{
    if (o->len + 1 < o->size)
        o->buf[o->len] = c;
    o->len++;
}

static void put_field(struct out *o, const char *s, size_t len, int width, int left, char pad)
{
    int fill = width > (int)len ? width - (int)len : 0;
    if (!left)
        while (fill-- > 0)
            put(o, pad);
    for (size_t i = 0; i < len; i++)
        put(o, s[i]);
    if (left)
        while (fill-- > 0)
            put(o, ' ');
}

int vsnprintf(char *buf, size_t size, const char *fmt, va_list ap)
{
    struct out o = {buf, size, 0};
    for (; *fmt; fmt++) {
        if (*fmt != '%') {
            put(&o, *fmt);
            continue;
        }
        fmt++;
        int left = 0, width = 0;
        char pad = ' ';
        if (*fmt == '-') {
            left = 1;
            fmt++;
        }
        if (*fmt == '0') {
            pad = '0';
            fmt++;
        }
        while (*fmt >= '0' && *fmt <= '9')
            width = width * 10 + (*fmt++ - '0');

        char num[12];
        size_t n = 0;
        unsigned v = 0, base = 10;
        int negative = 0;
        switch (*fmt) {
        case '\0':
            fmt--;
            continue;
        case 'c': {
            char c = (char)va_arg(ap, int);
            put_field(&o, &c, 1, width, left, ' ');
            continue;
        }
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s)
                s = "(null)";
            put_field(&o, s, strlen(s), width, left, ' ');
            continue;
        }
        case 'd': {
            int d = va_arg(ap, int);
            negative = d < 0;
            v = negative ? 0u - (unsigned)d : (unsigned)d;
            break;
        }
        case 'u':
            v = va_arg(ap, unsigned);
            break;
        case 'x':
            v = va_arg(ap, unsigned);
            base = 16;
            break;
        default:
            put(&o, '%');
            put(&o, *fmt);
            continue;
        }
        char digits[12];
        do {
            digits[n++] = "0123456789abcdef"[v % base];
            v /= base;
        } while (v);
        size_t k = 0;
        if (negative)
            num[k++] = '-';
        while (n)
            num[k++] = digits[--n];
        put_field(&o, num, k, width, left, negative && pad == '0' ? ' ' : pad);
    }
    if (size)
        buf[o.len < size ? o.len : size - 1] = '\0';
    return (int)o.len;
}

int printf(const char *fmt, ...)
{
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    size_t len = (size_t)n < sizeof(buf) ? (size_t)n : sizeof(buf) - 1;
    write(buf, len);
    return n;
}

int puts(const char *s)
{
    write(s, strlen(s));
    write("\n", 1);
    return 0;
}
