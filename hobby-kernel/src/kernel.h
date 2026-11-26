#ifndef KERNEL_H
#define KERNEL_H

#define KERNEL_NAME "hobby-kernel"
#define KERNEL_VERSION "0.1"

/* Defined by linker.ld. Use their addresses, never their values. */
extern char _kernel_start[];
extern char _kernel_end[];

#endif
