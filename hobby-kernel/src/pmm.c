#include "pmm.h"

#include <stdbool.h>
#include <stddef.h>

#include "kernel.h"
#include "kprintf.h"
#include "kstring.h"
#include "sync.h"

#define MAX_FRAMES (1u << 20) /* 4 GiB / 4 KiB */
#define BITMAP_WORDS (MAX_FRAMES / 32)
#define LOW_MEMORY_END 0x100000u /* BIOS data, VGA memory, option ROMs */

static uint32_t bitmap[BITMAP_WORDS];
static uint32_t usable_frames;
static uint32_t free_frames;
static uint32_t frame_limit; /* one past the highest usable frame */
static uint32_t search_hint; /* bitmap word to start searching from */

static inline bool frame_used(uint32_t frame)
{
    return bitmap[frame / 32] & (1u << (frame % 32));
}

static inline void set_used(uint32_t frame)
{
    bitmap[frame / 32] |= 1u << (frame % 32);
}

static inline void set_free(uint32_t frame)
{
    bitmap[frame / 32] &= ~(1u << (frame % 32));
}

static void free_range(uint64_t base, uint64_t length)
{
    /* Only whole frames inside the region are usable. */
    uint64_t start = (base + PAGE_SIZE - 1) / PAGE_SIZE;
    uint64_t end = (base + length) / PAGE_SIZE;
    if (end > MAX_FRAMES)
        end = MAX_FRAMES;
    for (uint64_t f = start; f < end; f++) {
        if (frame_used((uint32_t)f)) {
            set_free((uint32_t)f);
            usable_frames++;
            free_frames++;
        }
    }
    if (end > frame_limit && start < end)
        frame_limit = (uint32_t)end;
}

void pmm_reserve_range(uint64_t base, uint64_t length)
{
    /* Any frame the range touches is reserved. */
    uint64_t start = base / PAGE_SIZE;
    uint64_t end = (base + length + PAGE_SIZE - 1) / PAGE_SIZE;
    if (start >= MAX_FRAMES)
        return;
    if (end > MAX_FRAMES)
        end = MAX_FRAMES;
    for (uint64_t f = start; f < end; f++) {
        if (!frame_used((uint32_t)f)) {
            set_used((uint32_t)f);
            free_frames--;
        }
    }
}

void pmm_init(const struct mb2_info *info)
{
    memset(bitmap, 0xFF, sizeof(bitmap));
    usable_frames = 0;
    free_frames = 0;
    frame_limit = 0;
    search_hint = 0;

    const struct mb2_tag_mmap *mmap = (const struct mb2_tag_mmap *)mb2_find_tag(info, MB2_TAG_MMAP);
    if (!mmap)
        panic("bootloader provided no memory map");

    const uint8_t *entry = (const uint8_t *)mmap + sizeof(*mmap);
    const uint8_t *end = (const uint8_t *)mmap + mmap->size;
    for (; entry + sizeof(struct mb2_mmap_entry) <= end; entry += mmap->entry_size) {
        const struct mb2_mmap_entry *e = (const struct mb2_mmap_entry *)entry;
        if (e->type == MB2_MEMORY_AVAILABLE)
            free_range(e->base_addr, e->length);
    }

    pmm_reserve_range(0, LOW_MEMORY_END);
    pmm_reserve_range((uintptr_t)_kernel_start, (uintptr_t)_kernel_end - (uintptr_t)_kernel_start);
    pmm_reserve_range((uintptr_t)info, info->total_size);
}

static uint32_t alloc_locked(void)
{
    uint32_t words = (frame_limit + 31) / 32;
    for (uint32_t n = 0; n < words; n++) {
        uint32_t w = (search_hint + n) % words;
        if (bitmap[w] == 0xFFFFFFFFu)
            continue;
        uint32_t frame = w * 32 + (uint32_t)__builtin_ctz(~bitmap[w]);
        if (frame >= frame_limit)
            continue;
        set_used(frame);
        free_frames--;
        search_hint = w;
        return frame * PAGE_SIZE;
    }
    return 0;
}

uint32_t pmm_alloc_frame(void)
{
    uint32_t irq = irq_save();
    uint32_t frame = alloc_locked();
    irq_restore(irq);
    return frame;
}

void pmm_free_frame(uint32_t phys)
{
    uint32_t frame = phys / PAGE_SIZE;
    uint32_t irq = irq_save();
    if (phys % PAGE_SIZE || frame >= frame_limit || !frame_used(frame)) {
        irq_restore(irq);
        kprintf("pmm: ignoring bad free of frame 0x%08x\n", (unsigned)phys);
        return;
    }
    set_free(frame);
    free_frames++;
    irq_restore(irq);
}

uint32_t pmm_usable_frames(void)
{
    return usable_frames;
}

uint32_t pmm_free_frames(void)
{
    return free_frames;
}
