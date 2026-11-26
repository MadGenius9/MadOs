#ifndef KPRINTF_H
#define KPRINTF_H

#include <stdarg.h>

/* Console output goes to both the VGA screen and COM1.
   kprintf supports %c %s %d %i %u %x %X %p %% with an optional '-' flag,
   '0' flag and field width (for example %08x). Arguments are 32-bit. */
void kputc(char c);
void kputs(const char *s);
void kprintf(const char *fmt, ...) __attribute__((format(printf, 1, 2)));
void kvprintf(const char *fmt, va_list ap);

/* Prints the message in red, disables interrupts and halts forever. */
__attribute__((noreturn, format(printf, 1, 2))) void panic(const char *fmt, ...);

#endif
