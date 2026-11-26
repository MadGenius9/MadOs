#ifndef HEAP_H
#define HEAP_H

#include <stddef.h>
#include <stdint.h>

/* Kernel heap at virtual 0xD0000000 (up to 256 MiB). It grows on demand by
   taking frames from the PMM and mapping them with the paging code.
   Allocations are 16-byte aligned. Freed blocks merge with free neighbours;
   mapped pages are kept for reuse rather than returned to the PMM. */

struct heap_stats {
    size_t mapped_bytes;
    size_t used_bytes;
    size_t free_bytes;
    size_t used_blocks;
    size_t free_blocks;
};

void heap_init(void);
void *kmalloc(size_t size);
void *kcalloc(size_t count, size_t size);
void kfree(void *ptr);
void heap_get_stats(struct heap_stats *stats);

#endif
