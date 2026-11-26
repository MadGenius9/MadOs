/* Host unit test for src/heap.c. Compiles the real allocator against a
   static arena with stub paging/PMM functions: `make test`. */

#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define ARENA_SIZE (64u << 20)
static uint8_t heap_test_arena[ARENA_SIZE] __attribute__((aligned(4096)));
#define HEAP_START ((uintptr_t)heap_test_arena)
#define HEAP_MAX ((uintptr_t)heap_test_arena + ARENA_SIZE)

/* ---- stubs for the kernel functions heap.c uses ---- */

static unsigned mapped_pages;
static unsigned frames_in_use;
static unsigned fail_frames_after = ~0u; /* simulate running out of RAM */

uint32_t pmm_alloc_frame(void)
{
    if (frames_in_use >= fail_frames_after)
        return 0;
    frames_in_use++;
    return 0x1000;
}

void pmm_free_frame(uint32_t phys)
{
    (void)phys;
    frames_in_use--;
}

_Bool paging_map_page(uintptr_t virt, uint32_t phys, uint32_t flags)
{
    (void)phys;
    (void)flags;
    if (virt % 4096 || virt < HEAP_START || virt >= HEAP_MAX) {
        fprintf(stderr, "map of bad address %#lx\n", (unsigned long)virt);
        abort();
    }
    mapped_pages++;
    return 1;
}

uint32_t paging_unmap_page(uintptr_t virt)
{
    (void)virt;
    mapped_pages--;
    return 0x1000;
}

static jmp_buf panic_jump;
static int expect_panic;
static char panic_message[256];

void panic(const char *fmt, ...)
{
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(panic_message, sizeof(panic_message), fmt, ap);
    va_end(ap);
    if (expect_panic)
        longjmp(panic_jump, 1);
    fprintf(stderr, "unexpected panic: %s\n", panic_message);
    abort();
}

#include "../src/heap.c"

/* ---- helpers ---- */

static int failures;
#define CHECK(cond)                                                                                                    \
    do {                                                                                                               \
        if (!(cond)) {                                                                                                 \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);                                   \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

static void reset(void)
{
    heap_init();
    mapped_pages = 0;
    frames_in_use = 0;
}

/* Walks the block list and checks every structural invariant. */
static void check_invariants(void)
{
    uintptr_t expect = HEAP_START;
    struct block *prev = NULL;
    for (struct block *b = head; b; b = b->next) {
        CHECK((uintptr_t)b == expect);
        CHECK(b->prev == prev);
        CHECK(b->magic == MAGIC_FREE || b->magic == MAGIC_USED);
        CHECK(b->size % HEAP_ALIGN == 0);
        if (prev)
            CHECK(!(prev->magic == MAGIC_FREE && b->magic == MAGIC_FREE)); /* no unmerged neighbours */
        expect = (uintptr_t)(b + 1) + b->size;
        prev = b;
    }
    CHECK(tail == prev);
    CHECK(expect == heap_end);
    CHECK(mapped_pages * 4096 == heap_end - HEAP_START);
}

static void test_basic(void)
{
    reset();
    CHECK(kmalloc(0) == NULL);
    uint8_t *a = kmalloc(1);
    uint8_t *b = kmalloc(100);
    CHECK(a && b);
    CHECK((uintptr_t)a % 16 == 0 && (uintptr_t)b % 16 == 0);
    CHECK(b >= a + 16);
    memset(a, 1, 1);
    memset(b, 2, 100);
    check_invariants();
    kfree(a);
    kfree(b);
    check_invariants();
    struct heap_stats s;
    heap_get_stats(&s);
    CHECK(s.used_blocks == 0 && s.free_blocks == 1);
    kfree(NULL); /* allowed, no-op */
}

static void test_reuse_and_coalesce(void)
{
    reset();
    void *a = kmalloc(64), *b = kmalloc(64), *c = kmalloc(64);
    kfree(b);
    void *d = kmalloc(48);
    CHECK(d == b); /* first fit reuses the hole */
    kfree(a);
    kfree(d);
    check_invariants();
    void *e = kmalloc(64 + 64 + sizeof(struct block)); /* fits only in a+b merged */
    CHECK(e == a);
    kfree(e);
    kfree(c);
    check_invariants();
    CHECK(head == tail && head->magic == MAGIC_FREE);
}

static void test_growth(void)
{
    reset();
    void *small = kmalloc(16);
    void *big = kmalloc(5u << 20);
    CHECK(small && big);
    memset(big, 0xAB, 5u << 20);
    check_invariants();
    CHECK(heap_end - HEAP_START >= (5u << 20));
    kfree(small);
    kfree(big);
    check_invariants();
    /* Freed memory is reused rather than mapping more. */
    size_t before = heap_end;
    void *again = kmalloc(4u << 20);
    CHECK(again && heap_end == before);
    kfree(again);
}

static void test_limits(void)
{
    reset();
    CHECK(kmalloc(ARENA_SIZE) == NULL); /* header does not fit */
    CHECK(kmalloc((size_t)-1) == NULL);
    CHECK(kcalloc((size_t)-1 / 2, 4) == NULL); /* overflow */
    uint32_t *z = kcalloc(100, sizeof(uint32_t));
    CHECK(z);
    for (int i = 0; i < 100; i++)
        CHECK(z[i] == 0);
    kfree(z);

    /* Out of physical memory: grow fails cleanly and rolls back. */
    reset();
    fail_frames_after = 3;
    CHECK(kmalloc(10 * 4096) == NULL);
    CHECK(mapped_pages == 0 && frames_in_use == 0);
    CHECK(kmalloc(100) != NULL);
    fail_frames_after = ~0u;
    check_invariants();
}

static void test_bad_free(void)
{
    reset();
    char *p = kmalloc(32);
    kfree(p);
    expect_panic = 1;
    if (!setjmp(panic_jump)) {
        kfree(p);
        CHECK(!"double free not detected");
    } else {
        CHECK(strstr(panic_message, "double free"));
    }
    static char not_heap[32] __attribute__((aligned(16)));
    if (!setjmp(panic_jump)) {
        kfree(not_heap + 16);
        CHECK(!"foreign pointer not detected");
    } else {
        CHECK(strstr(panic_message, "not a heap pointer"));
    }
    expect_panic = 0;
}

static void test_random(void)
{
    enum { SLOTS = 512, ROUNDS = 200000 };
    static uint8_t *ptr[SLOTS];
    static size_t len[SLOTS];
    reset();
    uint32_t seed = 12345;
    for (int round = 0; round < ROUNDS; round++) {
        seed = seed * 1103515245u + 12345u;
        int i = (seed >> 8) % SLOTS;
        if (ptr[i]) {
            for (size_t j = 0; j < len[i]; j++) {
                if (ptr[i][j] != (uint8_t)i) {
                    CHECK(!"data corrupted");
                    return;
                }
            }
            kfree(ptr[i]);
            ptr[i] = NULL;
        } else {
            seed = seed * 1103515245u + 12345u;
            len[i] = 1 + (seed >> 4) % ((seed & 0x100) ? 20000 : 200);
            ptr[i] = kmalloc(len[i]);
            CHECK(ptr[i] != NULL);
            memset(ptr[i], (uint8_t)i, len[i]);
        }
        if (round % 10000 == 0)
            check_invariants();
    }
    for (int i = 0; i < SLOTS; i++)
        kfree(ptr[i]);
    check_invariants();
    CHECK(head == tail && head->magic == MAGIC_FREE);
}

int main(void)
{
    test_basic();
    test_reuse_and_coalesce();
    test_growth();
    test_limits();
    test_bad_free();
    test_random();
    if (failures) {
        printf("heap_test: %d failure(s)\n", failures);
        return 1;
    }
    printf("heap_test: all tests passed\n");
    return 0;
}
