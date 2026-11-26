#ifndef SERIAL_H
#define SERIAL_H

#include <stdbool.h>

/* COM1 output, mirrored from the console. In QEMU, `-serial stdio` shows it
   in your terminal, which is handy for logs and automated tests. */
bool serial_init(void);
void serial_putchar(char c);

#endif
