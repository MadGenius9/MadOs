#ifndef PROGRAMS_H
#define PROGRAMS_H

#include <stddef.h>
#include <stdint.h>

/* User programs built into the kernel image (see user/ and programs.asm). */
struct user_program {
    const char *name;
    const char *description;
    const uint8_t *start;
    const uint8_t *end;
};

extern const struct user_program user_programs[];
extern const size_t user_program_count;

const struct user_program *program_find(const char *name);

#endif
