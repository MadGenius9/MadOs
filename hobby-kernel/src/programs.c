#include "programs.h"

#include "kstring.h"

#define DECLARE(name) extern const uint8_t program_##name##_start[], program_##name##_end[]
#define ENTRY(name, description) {#name, description, program_##name##_start, program_##name##_end}

DECLARE(hello);
DECLARE(counter);
DECLARE(spin);
DECLARE(badptr);
DECLARE(priv);

const struct user_program user_programs[] = {
    ENTRY(hello, "print a greeting and the process id"),
    ENTRY(counter, "count to 5, sleeping 300 ms between lines"),
    ENTRY(spin, "loop forever (preemption demo; stop with kill or Ctrl+C)"),
    ENTRY(badptr, "touch kernel memory from ring 3 (gets killed)"),
    ENTRY(priv, "run a privileged instruction in ring 3 (gets killed)"),
};

const size_t user_program_count = sizeof(user_programs) / sizeof(user_programs[0]);

const struct user_program *program_find(const char *name)
{
    for (size_t i = 0; i < user_program_count; i++) {
        if (strcmp(user_programs[i].name, name) == 0)
            return &user_programs[i];
    }
    return NULL;
}
