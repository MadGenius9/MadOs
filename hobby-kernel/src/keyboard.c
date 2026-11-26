#include "keyboard.h"

#include <stdbool.h>
#include <stdint.h>

#include "idt.h"
#include "io.h"
#include "pic.h"
#include "sync.h"
#include "task.h"

#define PS2_DATA 0x60
#define PS2_STATUS 0x64
#define PS2_OUTPUT_FULL 0x01

#define SC_EXTENDED 0xE0
#define SC_RELEASE 0x80
#define SC_LCTRL 0x1D
#define SC_LSHIFT 0x2A
#define SC_RSHIFT 0x36
#define SC_LALT 0x38
#define SC_CAPSLOCK 0x3A

/* Scancode set 1 to ASCII, US layout. 0 = no character. */
static const char keymap_normal[0x3A] = {
    0,    27,  '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', '-', '=', '\b', /* 0x00-0x0E */
    '\t', 'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', '[', ']', '\n',      /* 0x0F-0x1C */
    0,    'a', 's', 'd', 'f', 'g', 'h', 'j', 'k', 'l', ';', '\'', '`',           /* 0x1D-0x29 */
    0,    '\\', 'z', 'x', 'c', 'v', 'b', 'n', 'm', ',', '.', '/', 0,             /* 0x2A-0x36 */
    '*',  0,   ' ',                                                              /* 0x37-0x39 */
};

static const char keymap_shift[0x3A] = {
    0,    27,  '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '\b',
    '\t', 'Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P', '{', '}', '\n',
    0,    'A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', ':', '"', '~',
    0,    '|', 'Z', 'X', 'C', 'V', 'B', 'N', 'M', '<', '>', '?', 0,
    '*',  0,   ' ',
};

static bool shift_left, shift_right, ctrl, alt, caps_lock, extended;

/* Single-producer (IRQ) / single-consumer ring buffer; uint8_t indices wrap
   at 256 on their own. */
static volatile char buffer[256];
static volatile uint8_t head, tail;
static struct task *waiter; /* task blocked in keyboard_getchar */

static void enqueue(char c)
{
    uint8_t next = (uint8_t)(head + 1);
    if (next == tail)
        return; /* full: drop the key */
    buffer[head] = c;
    head = next;
    if (waiter) {
        task_wake(waiter);
        waiter = NULL;
    }
}

static char translate(uint8_t code)
{
    if (code >= sizeof(keymap_normal))
        return 0;
    bool shift = shift_left || shift_right;
    char c = shift ? keymap_shift[code] : keymap_normal[code];
    if (caps_lock) {
        if (c >= 'a' && c <= 'z')
            c = (char)(c - 'a' + 'A');
        else if (c >= 'A' && c <= 'Z')
            c = (char)(c - 'A' + 'a');
    }
    if (ctrl) {
        if (c >= 'a' && c <= 'z')
            c = (char)(c - 'a' + 1);
        else if (c >= 'A' && c <= 'Z')
            c = (char)(c - 'A' + 1);
    }
    return c;
}

static void on_keyboard_irq(struct regs *r)
{
    (void)r;
    uint8_t scancode = inb(PS2_DATA);

    if (scancode == SC_EXTENDED) {
        extended = true;
        return;
    }
    bool released = scancode & SC_RELEASE;
    uint8_t code = scancode & (uint8_t)~SC_RELEASE;
    bool was_extended = extended;
    extended = false;

    switch (code) {
    case SC_LSHIFT:
        if (!was_extended) /* E0 2A is a fake shift sent around some keys */
            shift_left = !released;
        return;
    case SC_RSHIFT:
        if (!was_extended)
            shift_right = !released;
        return;
    case SC_LCTRL: /* E0 1D = right Ctrl */
        ctrl = !released;
        return;
    case SC_LALT: /* E0 38 = right Alt */
        alt = !released;
        return;
    case SC_CAPSLOCK:
        if (!released)
            caps_lock = !caps_lock;
        return;
    default:
        break;
    }

    if (released || alt)
        return;
    if (was_extended) {
        /* Keypad Enter and keypad '/' share codes with the main keys;
           arrows, Home, etc. are ignored for now. */
        if (code == 0x1C)
            enqueue('\n');
        else if (code == 0x35)
            enqueue('/');
        return;
    }
    char c = translate(code);
    if (c)
        enqueue(c);
}

void keyboard_init(void)
{
    /* Discard anything the BIOS left in the controller's output buffer. */
    while (inb(PS2_STATUS) & PS2_OUTPUT_FULL)
        (void)inb(PS2_DATA);
    interrupt_register_handler(IRQ_VECTOR(1), on_keyboard_irq);
    pic_unmask(1);
}

bool keyboard_trygetchar(char *c)
{
    uint32_t irq = irq_save();
    bool got = head != tail;
    if (got) {
        *c = buffer[tail];
        tail = (uint8_t)(tail + 1);
    }
    irq_restore(irq);
    return got;
}

char keyboard_getchar(void)
{
    char c;
    uint32_t irq = irq_save();
    /* Interrupts stay off between the empty check and blocking, so the key
       press that wakes us cannot slip in between. */
    while (!keyboard_trygetchar(&c)) {
        waiter = task_current();
        task_block();
    }
    irq_restore(irq);
    return c;
}
