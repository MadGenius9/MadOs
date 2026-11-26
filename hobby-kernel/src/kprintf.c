#include "kprintf.h"

#include <stdbool.h>
#include <stdint.h>

#include "kstring.h"
#include "serial.h"
#include "sync.h"
#include "vga.h"

void kputc(char c)
{
    vga_putchar(c);
    serial_putchar(c);
}

void kputs(const char *s)
{
    uint32_t irq = irq_save();
    while (*s)
        kputc(*s++);
    irq_restore(irq);
}

static int format_uint(char *out, uint32_t value, unsigned base, bool upper)
{
    const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char tmp[32];
    int n = 0;
    do {
        tmp[n++] = digits[value % base];
        value /= base;
    } while (value);
    for (int i = 0; i < n; i++)
        out[i] = tmp[n - 1 - i];
    return n;
}

static void emit(const char *s, int len, int width, bool left, char pad)
{
    int fill = width > len ? width - len : 0;
    if (!left) {
        for (int i = 0; i < fill; i++)
            kputc(pad);
    }
    for (int i = 0; i < len; i++)
        kputc(s[i]);
    if (left) {
        for (int i = 0; i < fill; i++)
            kputc(' ');
    }
}

void kvprintf(const char *fmt, va_list ap)
{
    for (; *fmt; fmt++) {
        if (*fmt != '%') {
            kputc(*fmt);
            continue;
        }
        fmt++;

        bool left = false;
        char pad = ' ';
        int width = 0;
        if (*fmt == '-') {
            left = true;
            fmt++;
        }
        if (*fmt == '0') {
            pad = '0';
            fmt++;
        }
        while (*fmt >= '0' && *fmt <= '9')
            width = width * 10 + (*fmt++ - '0');

        char buf[16];
        int n;
        switch (*fmt) {
        case '\0':
            return;
        case 'c':
            buf[0] = (char)va_arg(ap, int);
            emit(buf, 1, width, left, ' ');
            break;
        case 's': {
            const char *s = va_arg(ap, const char *);
            if (!s)
                s = "(null)";
            emit(s, (int)strlen(s), width, left, ' ');
            break;
        }
        case 'd':
        case 'i': {
            int v = va_arg(ap, int);
            uint32_t magnitude = v < 0 ? 0u - (uint32_t)v : (uint32_t)v;
            n = format_uint(buf + 1, magnitude, 10, false);
            if (v >= 0) {
                emit(buf + 1, n, width, left, pad);
            } else if (pad == '0' && !left) {
                kputc('-');
                emit(buf + 1, n, width - 1, false, '0');
            } else {
                buf[0] = '-';
                emit(buf, n + 1, width, left, ' ');
            }
            break;
        }
        case 'u':
            n = format_uint(buf, va_arg(ap, unsigned), 10, false);
            emit(buf, n, width, left, pad);
            break;
        case 'x':
        case 'X':
            n = format_uint(buf, va_arg(ap, unsigned), 16, *fmt == 'X');
            emit(buf, n, width, left, pad);
            break;
        case 'p':
            kputs("0x");
            n = format_uint(buf, (uint32_t)(uintptr_t)va_arg(ap, void *), 16, false);
            emit(buf, n, 8, false, '0');
            break;
        case '%':
            kputc('%');
            break;
        default:
            kputc('%');
            kputc(*fmt);
            break;
        }
    }
}

void kprintf(const char *fmt, ...)
{
    uint32_t irq = irq_save();
    va_list ap;
    va_start(ap, fmt);
    kvprintf(fmt, ap);
    va_end(ap);
    irq_restore(irq);
}

void panic(const char *fmt, ...)
{
    __asm__ volatile("cli");
    vga_set_color(VGA_WHITE, VGA_RED);
    kputs("\n*** KERNEL PANIC: ");
    va_list ap;
    va_start(ap, fmt);
    kvprintf(fmt, ap);
    va_end(ap);
    kputs(" ***\nSystem halted.");
    for (;;)
        __asm__ volatile("cli; hlt");
}
