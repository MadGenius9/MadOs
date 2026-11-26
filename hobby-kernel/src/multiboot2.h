#ifndef MULTIBOOT2_H
#define MULTIBOOT2_H

#include <stddef.h>
#include <stdint.h>

/* Value of EAX when a Multiboot2 loader jumps to the kernel. */
#define MB2_BOOTLOADER_MAGIC 0x36D76289

#define MB2_TAG_END 0
#define MB2_TAG_CMDLINE 1
#define MB2_TAG_BOOTLOADER_NAME 2
#define MB2_TAG_BASIC_MEMINFO 4
#define MB2_TAG_MMAP 6

#define MB2_MEMORY_AVAILABLE 1

struct mb2_info {
    uint32_t total_size;
    uint32_t reserved;
    /* followed by 8-byte aligned tags */
};

struct mb2_tag {
    uint32_t type;
    uint32_t size;
};

struct mb2_tag_string {
    uint32_t type;
    uint32_t size;
    char string[];
};

struct mb2_tag_basic_meminfo {
    uint32_t type;
    uint32_t size;
    uint32_t mem_lower; /* KiB below 1 MiB */
    uint32_t mem_upper; /* KiB above 1 MiB */
};

struct mb2_mmap_entry {
    uint64_t base_addr;
    uint64_t length;
    uint32_t type;
    uint32_t reserved;
};

struct mb2_tag_mmap {
    uint32_t type;
    uint32_t size;
    uint32_t entry_size;
    uint32_t entry_version;
    /* followed by entries of entry_size bytes */
};

/* Returns the first tag of the given type, or NULL. */
static inline const struct mb2_tag *mb2_find_tag(const struct mb2_info *info, uint32_t type)
{
    const uint8_t *p = (const uint8_t *)info + 8;
    const uint8_t *end = (const uint8_t *)info + info->total_size;
    while (p + sizeof(struct mb2_tag) <= end) {
        const struct mb2_tag *tag = (const struct mb2_tag *)p;
        if (tag->type == MB2_TAG_END)
            return NULL;
        if (tag->type == type)
            return tag;
        p += (tag->size + 7) & ~7u;
    }
    return NULL;
}

#endif
