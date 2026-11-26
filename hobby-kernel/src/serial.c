#include "serial.h"

#include "io.h"

#define COM1 0x3F8

static bool present;

bool serial_init(void)
{
    outb(COM1 + 1, 0x00); /* disable UART interrupts */
    outb(COM1 + 3, 0x80); /* DLAB on: next two writes set the divisor */
    outb(COM1 + 0, 0x03); /* divisor 3 = 38400 baud */
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03); /* 8 data bits, no parity, 1 stop bit */
    outb(COM1 + 2, 0xC7); /* enable and clear FIFOs, 14-byte threshold */
    outb(COM1 + 4, 0x1E); /* loopback mode for a self-test */
    outb(COM1 + 0, 0xAE);
    if (inb(COM1 + 0) != 0xAE) {
        present = false;
        return false;
    }
    outb(COM1 + 4, 0x0F); /* normal operation */
    present = true;
    return true;
}

static void raw_put(char c)
{
    while (!(inb(COM1 + 5) & 0x20)) {
        /* wait for the transmit holding register to empty */
    }
    outb(COM1, (uint8_t)c);
}

void serial_putchar(char c)
{
    if (!present)
        return;
    if (c == '\n') {
        raw_put('\r');
    } else if (c == '\b') {
        /* erase the character on a serial terminal too */
        raw_put('\b');
        raw_put(' ');
    }
    raw_put(c);
}
