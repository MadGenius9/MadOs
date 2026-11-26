/* Host unit test for src/fs.c: indexes the real initrd built by mkinitrd,
   plus hand-made archives (missing directories, corruption, truncation).
   `make test` runs it as: build/fs_test build/initrd.tar */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void *kcalloc(size_t count, size_t size)
{
    return calloc(count, size);
}

void kfree(void *p)
{
    free(p);
}

#include "../src/fs.c"

static int failures;
#define CHECK(cond)                                                                                                    \
    do {                                                                                                               \
        if (!(cond)) {                                                                                                 \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #cond);                                   \
            failures++;                                                                                                \
        }                                                                                                              \
    } while (0)

/* Writes one ustar header block for a file or directory. */
static void make_header(uint8_t *block, const char *name, uint32_t size, char type)
{
    struct tar_header *h = (struct tar_header *)block;
    memset(block, 0, TAR_BLOCK);
    snprintf(h->name, sizeof(h->name), "%s", name);
    snprintf(h->mode, sizeof(h->mode), "%07o", 0644);
    snprintf(h->uid, sizeof(h->uid), "%07o", 0);
    snprintf(h->gid, sizeof(h->gid), "%07o", 0);
    snprintf(h->size, sizeof(h->size), "%011o", size);
    snprintf(h->mtime, sizeof(h->mtime), "%011o", 0);
    h->typeflag = type;
    memcpy(h->magic, "ustar", 6);
    memcpy(h->version, "00", 2);
    memset(h->checksum, ' ', sizeof(h->checksum));
    unsigned sum = 0;
    for (int i = 0; i < TAR_BLOCK; i++)
        sum += block[i];
    snprintf(h->checksum, sizeof(h->checksum), "%06o", sum);
    h->checksum[7] = ' ';
}

static uint8_t *read_file(const char *path, size_t *size)
{
    FILE *f = fopen(path, "rb");
    if (!f)
        return NULL;
    fseek(f, 0, SEEK_END);
    *size = (size_t)ftell(f);
    fseek(f, 0, SEEK_SET);
    uint8_t *data = malloc(*size);
    if (fread(data, 1, *size, f) != *size) {
        free(data);
        data = NULL;
    }
    fclose(f);
    return data;
}

static void test_real_initrd(const char *path)
{
    size_t size;
    uint8_t *data = read_file(path, &size);
    CHECK(data != NULL);
    if (!data)
        return;
    CHECK(fs_init_tar(data, size) > 3);

    const struct fs_node *root = fs_lookup("/");
    CHECK(root && root->is_dir);
    const struct fs_node *bin = fs_lookup("/bin");
    CHECK(bin && bin->is_dir);
    CHECK(fs_lookup("/bin/") == bin);
    const struct fs_node *cat = fs_lookup("/bin/cat");
    CHECK(cat && !cat->is_dir && cat->size > 4 && memcmp(cat->data, "\x7f" "ELF", 4) == 0);
    CHECK(strcmp(fs_basename(cat), "cat") == 0);
    const struct fs_node *motd = fs_lookup("/etc/motd");
    CHECK(motd && motd->size > 0 && memcmp(motd->data, "Welcome", 7) == 0);
    CHECK(fs_lookup("/nope") == NULL);
    CHECK(fs_lookup("bin/cat") == NULL); /* must be absolute */

    /* The root's children are exactly bin and etc; /bin holds no directories. */
    size_t n = 0;
    const struct fs_node *c;
    while ((c = fs_child(root, n)) != NULL) {
        CHECK(c->is_dir);
        n++;
    }
    CHECK(n == 2);
    n = 0;
    while ((c = fs_child(bin, n)) != NULL) {
        CHECK(!c->is_dir && strncmp(c->path, "/bin/", 5) == 0);
        n++;
    }
    CHECK(n >= 8);
    CHECK(fs_child(cat, 0) == NULL); /* a file has no children */
    free(data);
}

static void test_missing_dirs_and_errors(void)
{
    /* One file three levels deep, no directory entries. */
    static uint8_t tar[TAR_BLOCK * 4];
    memset(tar, 0, sizeof(tar));
    make_header(tar, "./a/b/c.txt", 5, '0');
    memcpy(tar + TAR_BLOCK, "hello", 5);
    CHECK(fs_init_tar(tar, sizeof(tar)) == 4); /* /, /a, /a/b, /a/b/c.txt */
    const struct fs_node *a = fs_lookup("/a");
    const struct fs_node *b = fs_lookup("/a/b");
    const struct fs_node *f = fs_lookup("/a/b/c.txt");
    CHECK(a && a->is_dir && b && b->is_dir && f && f->size == 5);
    CHECK(fs_child(a, 0) == b && fs_child(a, 1) == NULL);
    CHECK(fs_child(b, 0) == f);
    CHECK(f && memcmp(f->data, "hello", 5) == 0);

    /* A corrupt checksum rejects the archive. */
    tar[148] ^= 1;
    CHECK(fs_init_tar(tar, sizeof(tar)) == 0);
    CHECK(fs_lookup("/") == NULL);
    tar[148] ^= 1;

    /* A size field pointing past the end rejects the archive. */
    make_header(tar, "big", 100000, '0');
    CHECK(fs_init_tar(tar, sizeof(tar)) == 0);

    /* Not a tar archive at all. */
    static uint8_t junk[TAR_BLOCK * 2];
    memset(junk, 'x', sizeof(junk));
    CHECK(fs_init_tar(junk, sizeof(junk)) == 0);
}

int main(int argc, char **argv)
{
    if (argc != 2) {
        fprintf(stderr, "usage: fs_test INITRD.tar\n");
        return 2;
    }
    test_real_initrd(argv[1]);
    test_missing_dirs_and_errors();
    if (failures) {
        printf("fs_test: %d failure(s)\n", failures);
        return 1;
    }
    printf("fs_test: all tests passed\n");
    return 0;
}
