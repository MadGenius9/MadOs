#ifndef KEYBOARD_H
#define KEYBOARD_H

#include <stdbool.h>

/* PS/2 keyboard on IRQ 1, scancode set 1, US layout. Key presses are turned
   into ASCII and queued; Ctrl+letter yields the control code (Ctrl+C = 3). */
void keyboard_init(void);

/* Blocks the calling task until a character arrives. */
char keyboard_getchar(void);

/* Non-blocking: returns true and stores a character if one is queued. */
bool keyboard_trygetchar(char *c);

#endif
