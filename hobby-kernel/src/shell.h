#ifndef SHELL_H
#define SHELL_H

/* Tiny interactive shell on top of the keyboard driver. Never returns. */
__attribute__((noreturn)) void shell_run(void);

#endif
