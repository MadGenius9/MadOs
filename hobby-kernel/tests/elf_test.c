/* Host unit test for src/elf.c. Loads a real user program and many
   corrupted variants of it. The user range the loader writes to is mapped
   on the host at the same address (0x40000000), and paging/PMM calls are
   stubbed. A rejected image must be refused before anything is mapped.
   `make test` runs it as: build/elf_test build/initrd/bin/cat */

#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>

#define ARENA_SIZE (16u << 20)

static unsigned maps;
static uint32_t mapped_pages[4096];

uint32_t pmm_alloc_frame(void)
{
    return 0x1000;
}

void pmm_free_frame(uint32_t phys)
{
    (void)phys;
}

uint32_t paging_virt_to_phys(uintptr_t virt)
{
    for (unsigned i = 0; i < maps; i++) {
        if (mapped_pages[i] == (virt & ~0xFFFu))
            return 0x1000;
    }
    return 0;
}

_Bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags)
{
    (void)phys;
    if (virt < 0x40000000u || virt >= 0x40000000u + ARENA_SIZE || !(flags & 4) || maps == 4096) {
        fprintf(stderr, "bad map of %#lx flags %#x\n", (unsigned long)virt, flags);
        abort();
    }
    mapped_pages[maps++] = (uint32_t)virt;
    return 1;
}

#include "../src/elf.c"

static int failures;
#define CHECK(cond)                                                                                                    \
    do {                                                                                                               \
        if (!(cond)) {                                                                                                 \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);                                   \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

#define LIMIT (0x40000000u + ARENA_SIZE)

static uint8_t *image;
static size_t image_size;

/* Loads a modified copy of the image; returns the error (NULL = loaded). */
static const char *load_variant(void (*mutate)(uint8_t *img, size_t *size))
{
    uint8_t *copy = malloc(image_size);
    memcpy(copy, image, image_size);
    size_t size = image_size;
    if (mutate)
        mutate(copy, &size);
    maps = 0;
    uint32_t entry = 0;
    const char *err = elf_load(copy, size, LIMIT, &entry);
    free(copy);
    return err;
}

static struct elf_header *eh(uint8_t *img)
{
    return (struct elf_header *)img;
}

static struct elf_phdr *ph0(uint8_t *img)
{
    return (struct elf_phdr *)(img + eh(img)->phoff);
}

static void bad_magic(uint8_t *img, size_t *size) { (void)size; img[1] = 'X'; }
static void bad_class(uint8_t *img, size_t *size) { (void)size; img[4] = 2; }
static void bad_machine(uint8_t *img, size_t *size) { (void)size; eh(img)->machine = 62; }
static void truncated(uint8_t *img, size_t *size) { (void)img; *size = 20; }
static void phoff_past_end(uint8_t *img, size_t *size) { eh(img)->phoff = (uint32_t)*size; }
static void too_many_phdrs(uint8_t *img, size_t *size) { (void)size; eh(img)->phnum = 1000; }
static void kernel_vaddr(uint8_t *img, size_t *size) { (void)size; ph0(img)->vaddr = 0x00100000; }
/* These two keep vaddr and the entry point valid, so only the size is wrong. */
static void wrapping_segment(uint8_t *img, size_t *size) { (void)size; ph0(img)->memsz = 0xFFFFF000u; }
static void past_limit(uint8_t *img, size_t *size)
{
    (void)size;
    ph0(img)->memsz = LIMIT - ph0(img)->vaddr + 0x1000;
}
static void filesz_gt_memsz(uint8_t *img, size_t *size) { (void)size; ph0(img)->filesz = ph0(img)->memsz + 1; }
static void data_past_end(uint8_t *img, size_t *size) { ph0(img)->offset = (uint32_t)*size - 4; }
static void entry_outside(uint8_t *img, size_t *size) { (void)size; eh(img)->entry = 0x30000000; }

int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: elf_test PROGRAM\n");
        return 2;
    }
    void *arena = mmap((void *)0x40000000, ARENA_SIZE, PROT_READ | PROT_WRITE,
                       MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE, -1, 0);
    if (arena != (void *)0x40000000) {
        printf("elf_test: SKIPPED (cannot map 0x40000000 on this host)\n");
        return 0;
    }
    FILE *f = fopen(argv[1], "rb");
    if (!f) {
        perror(argv[1]);
        return 2;
    }
    fseek(f, 0, SEEK_END);
    image_size = (size_t)ftell(f);
    fseek(f, 0, SEEK_SET);
    image = malloc(image_size);
    if (fread(image, 1, image_size, f) != image_size)
        return 2;
    fclose(f);

    /* The real program loads: its bytes appear at their virtual address. */
    memset(arena, 0xCC, ARENA_SIZE);
    const char *err = load_variant(NULL);
    CHECK(err == NULL);
    struct elf_phdr *p = ph0(image);
    CHECK(maps > 0);
    CHECK(memcmp((void *)(uintptr_t)p->vaddr, image + p->offset, p->filesz) == 0);
    /* Bytes past the file contents (.bss and the rest of the page) are zero. */
    uint32_t end = (p->vaddr + p->memsz + 0xFFF) & ~0xFFFu;
    for (uint32_t a = p->vaddr + p->filesz; a < end; a++) {
        if (*(uint8_t *)(uintptr_t)a != 0) {
            CHECK(!"memory after the segment contents is not zeroed");
            break;
        }
    }

    /* Each variant must be rejected for its own reason, before mapping. */
    struct {
        const char *name;
        void (*mutate)(uint8_t *, size_t *);
        const char *expect;
    } bad[] = {
        {"bad magic", bad_magic, "not an ELF file"},
        {"64-bit class", bad_class, "not a 32-bit little-endian ELF"},
        {"wrong machine", bad_machine, "not an i386 executable"},
        {"truncated header", truncated, "file too small"},
        {"program headers past the end", phoff_past_end, "program headers outside the file"},
        {"too many program headers", too_many_phdrs, "bad program header table"},
        {"segment in kernel memory", kernel_vaddr, "segment outside user memory"},
        {"segment wrapping around 4 GiB", wrapping_segment, "segment outside user memory"},
        {"segment past the user limit", past_limit, "segment outside user memory"},
        {"file size larger than memory size", filesz_gt_memsz, "segment file size exceeds memory size"},
        {"segment data past the end", data_past_end, "segment data outside the file"},
        {"entry point outside segments", entry_outside, "entry point not in a loadable segment"},
    };
    for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
        err = load_variant(bad[i].mutate);
        if (!err || strncmp(err, bad[i].expect, strlen(bad[i].expect)) != 0 || maps != 0) {
            fprintf(stderr, "  variant '%s': got '%s' (expected '%s'), %u pages mapped\n", bad[i].name,
                    err ? err : "accepted", bad[i].expect, maps);
            failures++;
        }
    }

    if (failures) {
        printf("elf_test: %d failure(s)\n", failures);
        return 1;
    }
    printf("elf_test: all tests passed (1 valid image, %zu rejected variants)\n", sizeof(bad) / sizeof(bad[0]));
    return 0;
}
