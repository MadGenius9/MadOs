#ifndef MULTIBOOT1_H
#define MULTIBOOT1_H

#include <stdint.h>

#include "multiboot2.h"

/* Value of EAX when a Multiboot (version 1) loader jumps to the kernel.
   GRUB uses Multiboot2; Multiboot1 is supported so that QEMU's built-in
   loader (`qemu-system-i386 -kernel kernel.elf`) can boot the kernel
   without an ISO. */
#define MB1_BOOTLOADER_MAGIC 0x2BADB002

/* Translates the Multiboot1 information at mbi_phys into an equivalent
   Multiboot2 structure (bootloader name + memory map) in a static buffer,
   so the rest of the kernel only has to understand Multiboot2. */
const struct mb2_info *mb1_convert(uint32_t mbi_phys);

#endif
