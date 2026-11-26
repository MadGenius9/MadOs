#ifndef ELF_H
#define ELF_H

#include <stddef.h>
#include <stdint.h>

/* Loads a 32-bit x86 ELF executable into the *current* address space: maps
   zeroed user pages for every PT_LOAD segment, copies the file contents and
   leaves the rest (.bss) zero. Every header field is checked against the
   image size and the user address range first, so a corrupt or hostile
   file cannot make the kernel read outside the image or map kernel space.
   Returns NULL on success (entry point in *entry) or an error message. */
const char *elf_load(const uint8_t *image, size_t size, uint32_t limit, uint32_t *entry);

#endif
