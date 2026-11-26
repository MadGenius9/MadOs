#include "multiboot1.h"

#include <stddef.h>

#include "kstring.h"

struct mb1_info {
    uint32_t flags;
    uint32_t mem_lower; /* KiB below 1 MiB (flag bit 0) */
    uint32_t mem_upper; /* KiB above 1 MiB (flag bit 0) */
    uint32_t boot_device;
    uint32_t cmdline;
    uint32_t mods_count;
    uint32_t mods_addr;
    uint32_t syms[4];
    uint32_t mmap_length; /* flag bit 6 */
    uint32_t mmap_addr;
    uint32_t drives_length;
    uint32_t drives_addr;
    uint32_t config_table;
    uint32_t boot_loader_name; /* flag bit 9 */
} __attribute__((packed));

struct mb1_mmap_entry {
    uint32_t size; /* size of the rest of the entry, not counting this field */
    uint64_t base_addr;
    uint64_t length;
    uint32_t type;
} __attribute__((packed));

#define MB1_FLAG_MEMINFO (1u << 0)
#define MB1_FLAG_MMAP (1u << 6)
#define MB1_FLAG_LOADER_NAME (1u << 9)

#define MAX_NAME 64
#define MAX_ENTRIES 64

static uint8_t buffer[8 + 8 + MAX_NAME + sizeof(struct mb2_tag_mmap) + MAX_ENTRIES * sizeof(struct mb2_mmap_entry) + 8]
    __attribute__((aligned(8)));

static uint32_t align8(uint32_t n)
{
    return (n + 7) & ~7u;
}

static void add_entry(struct mb2_mmap_entry *e, uint64_t base, uint64_t length, uint32_t type)
{
    e->base_addr = base;
    e->length = length;
    e->type = type;
    e->reserved = 0;
}

const struct mb2_info *mb1_convert(uint32_t mbi_phys)
{
    const struct mb1_info *mb1 = (const struct mb1_info *)(uintptr_t)mbi_phys;
    uint8_t *p = buffer + 8;

    const char *name = "unknown (Multiboot1)";
    if (mb1->flags & MB1_FLAG_LOADER_NAME)
        name = (const char *)(uintptr_t)mb1->boot_loader_name;
    size_t len = strlen(name);
    if (len > MAX_NAME - 1)
        len = MAX_NAME - 1;
    struct mb2_tag_string *loader = (struct mb2_tag_string *)p;
    loader->type = MB2_TAG_BOOTLOADER_NAME;
    loader->size = (uint32_t)(8 + len + 1);
    memcpy(loader->string, name, len);
    loader->string[len] = '\0';
    p += align8(loader->size);

    struct mb2_tag_mmap *mmap = (struct mb2_tag_mmap *)p;
    struct mb2_mmap_entry *out = (struct mb2_mmap_entry *)(p + sizeof(*mmap));
    uint32_t n = 0;
    if (mb1->flags & MB1_FLAG_MMAP) {
        uint32_t addr = mb1->mmap_addr;
        uint32_t end = mb1->mmap_addr + mb1->mmap_length;
        while (addr < end && n < MAX_ENTRIES) {
            const struct mb1_mmap_entry *e = (const struct mb1_mmap_entry *)(uintptr_t)addr;
            add_entry(&out[n++], e->base_addr, e->length, e->type);
            addr += e->size + sizeof(e->size);
        }
    } else if (mb1->flags & MB1_FLAG_MEMINFO) {
        add_entry(&out[n++], 0, (uint64_t)mb1->mem_lower * 1024, MB2_MEMORY_AVAILABLE);
        add_entry(&out[n++], 0x100000, (uint64_t)mb1->mem_upper * 1024, MB2_MEMORY_AVAILABLE);
    }
    if (n) {
        mmap->type = MB2_TAG_MMAP;
        mmap->entry_size = sizeof(struct mb2_mmap_entry);
        mmap->entry_version = 0;
        mmap->size = (uint32_t)(sizeof(*mmap) + n * sizeof(struct mb2_mmap_entry));
        p += align8(mmap->size);
    }

    struct mb2_tag *end_tag = (struct mb2_tag *)p;
    end_tag->type = MB2_TAG_END;
    end_tag->size = 8;
    p += 8;

    struct mb2_info *info = (struct mb2_info *)buffer;
    info->total_size = (uint32_t)(p - buffer);
    info->reserved = 0;
    return info;
}
