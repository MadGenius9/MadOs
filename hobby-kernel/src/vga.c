/* VGA text mode driver: 80x25 cells at physical 0xB8000. Each cell is a
   16-bit value: low byte = code page 437 character, high byte = colour
   (low nibble foreground, high nibble background). */

#include "vga.h"

#include <stddef.h>
#include <stdint.h>

#include "io.h"

#define VGA_WIDTH 80
#define VGA_HEIGHT 25
#define VGA_CRTC_INDEX 0x3D4
#define VGA_CRTC_DATA 0x3D5

static volatile uint16_t *const vga_buffer = (volatile uint16_t *)0xB8000;
static size_t row;
static size_t col;
static uint8_t color = VGA_LIGHT_GREY | VGA_BLACK << 4;

static inline uint16_t cell(char c, uint8_t attr)
{
    return (uint16_t)(uint8_t)c | (uint16_t)attr << 8;
}

static void update_cursor(void)
{
    uint16_t pos = (uint16_t)(row * VGA_WIDTH + col);
    outb(VGA_CRTC_INDEX, 0x0F);
    outb(VGA_CRTC_DATA, (uint8_t)(pos & 0xFF));
    outb(VGA_CRTC_INDEX, 0x0E);
    outb(VGA_CRTC_DATA, (uint8_t)(pos >> 8));
}

static void enable_cursor(uint8_t start_line, uint8_t end_line)
{
    outb(VGA_CRTC_INDEX, 0x0A);
    outb(VGA_CRTC_DATA, (inb(VGA_CRTC_DATA) & 0xC0) | start_line);
    outb(VGA_CRTC_INDEX, 0x0B);
    outb(VGA_CRTC_DATA, (inb(VGA_CRTC_DATA) & 0xE0) | end_line);
}

static void scroll(void)
{
    for (size_t i = 0; i < (VGA_HEIGHT - 1) * VGA_WIDTH; i++)
        vga_buffer[i] = vga_buffer[i + VGA_WIDTH];
    for (size_t x = 0; x < VGA_WIDTH; x++)
        vga_buffer[(VGA_HEIGHT - 1) * VGA_WIDTH + x] = cell(' ', color);
    row = VGA_HEIGHT - 1;
}

static void newline(void)
{
    col = 0;
    if (++row == VGA_HEIGHT)
        scroll();
}

static void put_visible(char c)
{
    vga_buffer[row * VGA_WIDTH + col] = cell(c, color);
    if (++col == VGA_WIDTH)
        newline();
}

void vga_init(void)
{
    enable_cursor(14, 15);
    vga_clear();
}

void vga_clear(void)
{
    for (size_t i = 0; i < VGA_WIDTH * VGA_HEIGHT; i++)
        vga_buffer[i] = cell(' ', color);
    row = 0;
    col = 0;
    update_cursor();
}

void vga_set_color(enum vga_color fg, enum vga_color bg)
{
    color = (uint8_t)(fg | bg << 4);
}

void vga_putchar(char c)
{
    switch (c) {
    case '\n':
        newline();
        break;
    case '\r':
        col = 0;
        break;
    case '\t':
        do
            put_visible(' ');
        while (col % 4);
        break;
    case '\b':
        if (col > 0) {
            col--;
        } else if (row > 0) {
            row--;
            col = VGA_WIDTH - 1;
        }
        vga_buffer[row * VGA_WIDTH + col] = cell(' ', color);
        break;
    default:
        put_visible(c);
        break;
    }
    update_cursor();
}

void vga_write(const char *s)
{
    while (*s)
        vga_putchar(*s++);
}
