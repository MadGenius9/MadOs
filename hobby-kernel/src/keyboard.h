#ifndef KEYBOARD_H
#define KEYBOARD_H

/* PS/2 keyboard on IRQ 1, scancode set 1, US layout. Key presses are turned
   into ASCII and queued; Ctrl+letter yields the control code (Ctrl+C = 3). */
void keyboard_init(void);

/* Blocks (halting the CPU between interrupts) until a character arrives. */
char keyboard_getchar(void);

#endif
