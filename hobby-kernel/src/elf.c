#include "elf.h"

#include <stdbool.h>

#include "kstring.h"
#include "paging.h"
#include "pmm.h"

#define EI_NIDENT 16
#define ET_EXEC 2
#define EM_386 3
#define PT_LOAD 1
#define MAX_PHDRS 16

struct elf_header {
    uint8_t ident[EI_NIDENT];
    uint16_t type;
    uint16_t machine;
    uint32_t version;
    uint32_t entry;
    uint32_t phoff;
    uint32_t shoff;
    uint32_t flags;
    uint16_t ehsize;
    uint16_t phentsize;
    uint16_t phnum;
    uint16_t shentsize;
    uint16_t shnum;
    uint16_t shstrndx;
};

struct elf_phdr {
    uint32_t type;
    uint32_t offset;
    uint32_t vaddr;
    uint32_t paddr;
    uint32_t filesz;
    uint32_t memsz;
    uint32_t flags;
    uint32_t align;
};

static bool in_user_range(uint32_t start, uint32_t len, uint32_t limit)
{
    return start >= USER_BASE && start < limit && len <= limit - start;
}

/* Maps zeroed, writable user pages over [start, end) that are not mapped yet
   (two segments may share a page). */
static bool map_segment(uint32_t start, uint32_t end)
{
    for (uint32_t page = start & ~(PAGE_SIZE - 1); page < end; page += PAGE_SIZE) {
        if (paging_virt_to_phys(page))
            continue;
        uint32_t frame = pmm_alloc_frame();
        if (!frame)
            return false;
        if (!paging_map_page(page, frame, PG_RW | PG_USER)) {
            pmm_free_frame(frame);
            return false;
        }
        memset((void *)(uintptr_t)page, 0, PAGE_SIZE);
    }
    return true;
}

const char *elf_load(const uint8_t *image, size_t size, uint32_t limit, uint32_t *entry)
{
    if (size < sizeof(struct elf_header))
        return "file too small to be an ELF executable";
    const struct elf_header *eh = (const struct elf_header *)image;
    if (memcmp(eh->ident, "\x7f" "ELF", 4) != 0)
        return "not an ELF file";
    if (eh->ident[4] != 1 || eh->ident[5] != 1)
        return "not a 32-bit little-endian ELF";
    if (eh->type != ET_EXEC || eh->machine != EM_386)
        return "not an i386 executable";
    if (eh->phentsize != sizeof(struct elf_phdr) || eh->phnum == 0 || eh->phnum > MAX_PHDRS)
        return "bad program header table";
    if (eh->phoff > size || (size_t)eh->phnum * sizeof(struct elf_phdr) > size - eh->phoff)
        return "program headers outside the file";

    const struct elf_phdr *ph = (const struct elf_phdr *)(image + eh->phoff);
    bool entry_ok = false;
    for (uint16_t i = 0; i < eh->phnum; i++) {
        if (ph[i].type != PT_LOAD)
            continue;
        if (ph[i].filesz > ph[i].memsz)
            return "segment file size exceeds memory size";
        if (ph[i].offset > size || ph[i].filesz > size - ph[i].offset)
            return "segment data outside the file";
        if (!in_user_range(ph[i].vaddr, ph[i].memsz, limit))
            return "segment outside user memory";
        if (eh->entry >= ph[i].vaddr && eh->entry - ph[i].vaddr < ph[i].memsz)
            entry_ok = true;
    }
    if (!entry_ok)
        return "entry point not in a loadable segment";

    for (uint16_t i = 0; i < eh->phnum; i++) {
        if (ph[i].type != PT_LOAD || ph[i].memsz == 0)
            continue;
        if (!map_segment(ph[i].vaddr, ph[i].vaddr + ph[i].memsz))
            return "out of memory";
        memcpy((void *)(uintptr_t)ph[i].vaddr, image + ph[i].offset, ph[i].filesz);
    }
    *entry = eh->entry;
    return NULL;
}
