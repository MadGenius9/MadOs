#include "heap.h"

#include <stdbool.h>

#include "kprintf.h"
#include "kstring.h"
#include "paging.h"
#include "pmm.h"

/* Overridable so tests/heap_test.c can run this file on the host. */
#ifndef HEAP_START
#define HEAP_START ((uintptr_t)0xD0000000u)
#endif
#ifndef HEAP_MAX
#define HEAP_MAX ((uintptr_t)0xE0000000u)
#endif

#define HEAP_ALIGN 16
#define MAGIC_FREE 0xF4EEB10Cu
#define MAGIC_USED 0xA110CA7Eu

/* Every block, used or free, starts with this header. Blocks are kept in a
   doubly linked list in address order and are contiguous: a block's payload
   ends exactly where the next header starts, and the last block ends at
   heap_end. */
struct block {
    size_t size; /* payload bytes after the header */
    uint32_t magic;
    struct block *prev;
    struct block *next;
};

_Static_assert(sizeof(struct block) % HEAP_ALIGN == 0, "header must keep payloads aligned");

static struct block *head;
static struct block *tail;
static uintptr_t heap_end;

static size_t align_up(size_t value, size_t align)
{
    return (value + align - 1) & ~(align - 1);
}

static void unmap_pages(uintptr_t start, size_t bytes)
{
    for (size_t off = 0; off < bytes; off += PAGE_SIZE) {
        uint32_t phys = paging_unmap_page(start + off);
        if (phys)
            pmm_free_frame(phys);
    }
}

static bool map_pages(uintptr_t start, size_t bytes)
{
    for (size_t off = 0; off < bytes; off += PAGE_SIZE) {
        uint32_t frame = pmm_alloc_frame();
        if (!frame || !paging_map_page(start + off, frame, PG_RW)) {
            if (frame)
                pmm_free_frame(frame);
            unmap_pages(start, off);
            return false;
        }
    }
    return true;
}

/* Maps more pages so that a free block of at least `size` bytes exists at
   the end of the heap, and returns that block. */
static struct block *grow(size_t size)
{
    bool extend_tail = tail && tail->magic == MAGIC_FREE;
    size_t need = extend_tail ? size - tail->size : size + sizeof(struct block);
    size_t bytes = align_up(need, PAGE_SIZE);
    if (bytes > HEAP_MAX - heap_end || !map_pages(heap_end, bytes))
        return NULL;

    uintptr_t start = heap_end;
    heap_end += bytes;
    if (extend_tail) {
        tail->size += bytes;
        return tail;
    }
    struct block *b = (struct block *)start;
    b->size = bytes - sizeof(struct block);
    b->magic = MAGIC_FREE;
    b->prev = tail;
    b->next = NULL;
    if (tail)
        tail->next = b;
    else
        head = b;
    tail = b;
    return b;
}

/* Cuts b down to `size` bytes if the rest is big enough to be useful. */
static void split(struct block *b, size_t size)
{
    if (b->size < size + sizeof(struct block) + HEAP_ALIGN)
        return;
    struct block *rest = (struct block *)((uint8_t *)(b + 1) + size);
    rest->size = b->size - size - sizeof(struct block);
    rest->magic = MAGIC_FREE;
    rest->prev = b;
    rest->next = b->next;
    if (b->next)
        b->next->prev = rest;
    else
        tail = rest;
    b->next = rest;
    b->size = size;
}

/* Absorbs b->next into b. */
static void merge_with_next(struct block *b)
{
    struct block *n = b->next;
    b->size += sizeof(struct block) + n->size;
    b->next = n->next;
    if (n->next)
        n->next->prev = b;
    else
        tail = b;
    n->magic = 0;
}

void heap_init(void)
{
    head = NULL;
    tail = NULL;
    heap_end = HEAP_START;
}

void *kmalloc(size_t size)
{
    if (size == 0 || size > HEAP_MAX - HEAP_START)
        return NULL;
    size = align_up(size, HEAP_ALIGN);

    struct block *b = head;
    while (b && !(b->magic == MAGIC_FREE && b->size >= size))
        b = b->next;
    if (!b && !(b = grow(size)))
        return NULL;

    split(b, size);
    b->magic = MAGIC_USED;
    return b + 1;
}

void *kcalloc(size_t count, size_t size)
{
    if (size && count > (size_t)-1 / size)
        return NULL;
    void *p = kmalloc(count * size);
    if (p)
        memset(p, 0, count * size);
    return p;
}

void kfree(void *ptr)
{
    if (!ptr)
        return;
    struct block *b = (struct block *)ptr - 1;
    if ((uintptr_t)b < HEAP_START || (uintptr_t)ptr >= heap_end || (uintptr_t)ptr % HEAP_ALIGN)
        panic("kfree: %p is not a heap pointer", ptr);
    if (b->magic == MAGIC_FREE)
        panic("kfree: double free of %p", ptr);
    if (b->magic != MAGIC_USED)
        panic("kfree: heap corruption at %p", ptr);

    b->magic = MAGIC_FREE;
    if (b->next && b->next->magic == MAGIC_FREE)
        merge_with_next(b);
    if (b->prev && b->prev->magic == MAGIC_FREE)
        merge_with_next(b->prev);
}

void heap_get_stats(struct heap_stats *stats)
{
    memset(stats, 0, sizeof(*stats));
    stats->mapped_bytes = heap_end - HEAP_START;
    for (struct block *b = head; b; b = b->next) {
        if (b->magic == MAGIC_USED) {
            stats->used_blocks++;
            stats->used_bytes += b->size;
        } else {
            stats->free_blocks++;
            stats->free_bytes += b->size;
        }
    }
}
