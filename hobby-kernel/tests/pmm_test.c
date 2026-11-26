/* Host unit test for src/pmm.c: feeds it a hand-built Multiboot2 memory map
   and checks which frames it hands out. `make test`. */

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* The linker symbols; on the host they sit above 4 GiB, outside the range
   the PMM tracks, so they reserve nothing. */
char _kernel_start[1];
char _kernel_end[1];

static int warnings;

void kprintf(const char *fmt, ...)
{
    (void)fmt;
    warnings++;
}

void panic(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    vfprintf(stderr, fmt, ap);
    va_end(ap);
    abort();
}

#include "../src/pmm.c"

static int failures;
#define CHECK(cond)                                                                                                    \
    do {                                                                                                               \
        if (!(cond)) {                                                                                                 \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);                                   \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

static uint8_t info_buf[512] __attribute__((aligned(8)));

/* Typical QEMU -m 128M map plus a region above 4 GiB and an unaligned one. */
static const struct mb2_mmap_entry entries[] = {
    {0x00000000, 0x0009FC00, MB2_MEMORY_AVAILABLE, 0},
    {0x0009FC00, 0x00000400, 2, 0},
    {0x000F0000, 0x00010000, 2, 0},
    {0x00100000, 0x07EE0000, MB2_MEMORY_AVAILABLE, 0}, /* 1 MiB .. 0x07FE0000 */
    {0x07FE0000, 0x00020000, 2, 0},
    {0x08000800, 0x00003000, MB2_MEMORY_AVAILABLE, 0}, /* only 2 whole frames */
    {0xFFFC0000, 0x00040000, 2, 0},
    {0x100000000ull, 0x10000000, MB2_MEMORY_AVAILABLE, 0}, /* above 4 GiB: ignored */
};

static const struct mb2_info *build_info(void)
{
    uint8_t *p = info_buf + 8;
    struct mb2_tag_mmap *mmap = (struct mb2_tag_mmap *)p;
    mmap->type = MB2_TAG_MMAP;
    mmap->entry_size = sizeof(struct mb2_mmap_entry);
    mmap->entry_version = 0;
    memcpy(p + sizeof(*mmap), entries, sizeof(entries));
    mmap->size = sizeof(*mmap) + sizeof(entries);
    p += (mmap->size + 7) & ~7u;
    struct mb2_tag *end = (struct mb2_tag *)p;
    end->type = MB2_TAG_END;
    end->size = 8;
    p += 8;
    struct mb2_info *info = (struct mb2_info *)info_buf;
    info->total_size = (uint32_t)(p - info_buf);
    info->reserved = 0;
    return info;
}

static int frame_ok(uint32_t phys)
{
    return (phys >= 0x00100000 && phys < 0x07FE0000) || phys == 0x08001000 || phys == 0x08002000;
}

int main(void)
{
    pmm_init(build_info());

    /* Available frames: 0x9F below 1 MiB (then reserved again), 0x7EE0 from
       the main region, 2 from the unaligned one. */
    uint32_t main_frames = 0x07EE0000 / PAGE_SIZE;
    CHECK(pmm_usable_frames() == 0x9F + main_frames + 2);
    CHECK(pmm_free_frames() == main_frames + 2);

    /* Reserving a range inside free memory removes exactly those frames. */
    pmm_reserve_range(0x00200800, 0x1000); /* touches 2 frames */
    CHECK(pmm_free_frames() == main_frames);

    /* Hand out everything: every frame must be valid and unique. */
    static uint8_t seen[1u << 20];
    uint32_t count = 0, first = 0;
    for (;;) {
        uint32_t f = pmm_alloc_frame();
        if (!f)
            break;
        if (!first)
            first = f;
        CHECK(f % PAGE_SIZE == 0);
        CHECK(frame_ok(f));
        CHECK(f != 0x00200000 && f != 0x00201000);
        CHECK(!seen[f / PAGE_SIZE]);
        seen[f / PAGE_SIZE] = 1;
        count++;
    }
    CHECK(count == main_frames);
    CHECK(pmm_free_frames() == 0);
    CHECK(first == 0x00100000);

    /* Freeing makes a frame available again; bad frees are refused. */
    pmm_free_frame(0x00300000);
    CHECK(pmm_free_frames() == 1);
    CHECK(pmm_alloc_frame() == 0x00300000);
    pmm_free_frame(0x00300000);
    int w = warnings;
    pmm_free_frame(0x00300000); /* double free */
    pmm_free_frame(0x00300004); /* unaligned */
    pmm_free_frame(0xF0000000); /* outside RAM */
    CHECK(warnings == w + 3);
    CHECK(pmm_free_frames() == 1);

    if (failures) {
        printf("pmm_test: %d failure(s)\n", failures);
        return 1;
    }
    printf("pmm_test: all tests passed (%u frames allocated)\n", count);
    return 0;
}
