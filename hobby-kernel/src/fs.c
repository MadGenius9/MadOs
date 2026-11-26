#include "fs.h"

#include "heap.h"
#include "kstring.h"

#define TAR_BLOCK 512
#define MAX_DEPTH 8

struct tar_header {
    char name[100];
    char mode[8];
    char uid[8];
    char gid[8];
    char size[12];
    char mtime[12];
    char checksum[8];
    char typeflag;
    char linkname[100];
    char magic[6]; /* "ustar\0" */
    char version[2];
    char uname[32];
    char gname[32];
    char devmajor[8];
    char devminor[8];
    char prefix[155];
    char pad[12];
};

_Static_assert(sizeof(struct tar_header) == TAR_BLOCK, "tar header is one block");

static struct fs_node *nodes;
static size_t node_count;
static size_t node_capacity;

static uint32_t parse_octal(const char *s, size_t len, bool *ok)
{
    uint32_t v = 0;
    size_t i = 0;
    while (i < len && s[i] == ' ')
        i++;
    for (; i < len && s[i] >= '0' && s[i] <= '7'; i++) {
        if (v > 0x1FFFFFFF) {
            *ok = false;
            return 0;
        }
        v = v * 8 + (uint32_t)(s[i] - '0');
    }
    return v;
}

static bool checksum_ok(const struct tar_header *h)
{
    bool ok = true;
    uint32_t expected = parse_octal(h->checksum, sizeof(h->checksum), &ok);
    const uint8_t *b = (const uint8_t *)h;
    uint32_t sum = 0;
    for (size_t i = 0; i < TAR_BLOCK; i++) {
        bool in_field = i >= offsetof(struct tar_header, checksum) &&
                        i < offsetof(struct tar_header, checksum) + sizeof(h->checksum);
        sum += in_field ? ' ' : b[i];
    }
    return ok && sum == expected;
}

static bool all_zero(const uint8_t *b)
{
    for (size_t i = 0; i < TAR_BLOCK; i++) {
        if (b[i])
            return false;
    }
    return true;
}

/* Copies up to max bytes of a possibly unterminated field. */
static size_t field_len(const char *s, size_t max)
{
    size_t n = 0;
    while (n < max && s[n])
        n++;
    return n;
}

/* Builds "/prefix/name" without leading "./" or "/" and trailing "/".
   Returns false if it does not fit or is empty (the archive root). */
static bool make_path(char *out, const struct tar_header *h)
{
    char raw[FS_PATH_MAX * 2];
    size_t n = 0;
    size_t plen = field_len(h->prefix, sizeof(h->prefix));
    size_t nlen = field_len(h->name, sizeof(h->name));
    if (plen + nlen + 2 > sizeof(raw))
        return false;
    if (plen) {
        memcpy(raw, h->prefix, plen);
        n = plen;
        raw[n++] = '/';
    }
    memcpy(raw + n, h->name, nlen);
    n += nlen;
    raw[n] = '\0';

    const char *p = raw;
    for (;;) {
        if (p[0] == '.' && p[1] == '/')
            p += 2;
        else if (p[0] == '/')
            p++;
        else
            break;
    }
    size_t len = strlen(p);
    while (len && p[len - 1] == '/')
        len--;
    if (len == 0 || (len == 1 && p[0] == '.') || len + 2 > FS_PATH_MAX)
        return false;
    out[0] = '/';
    memcpy(out + 1, p, len);
    out[len + 1] = '\0';
    return true;
}

static struct fs_node *find(const char *path)
{
    for (size_t i = 0; i < node_count; i++) {
        if (strcmp(nodes[i].path, path) == 0)
            return &nodes[i];
    }
    return NULL;
}

static struct fs_node *add(const char *path, bool is_dir, const uint8_t *data, uint32_t size)
{
    if (node_count == node_capacity)
        return NULL;
    struct fs_node *n = &nodes[node_count++];
    memcpy(n->path, path, strlen(path) + 1);
    n->is_dir = is_dir;
    n->data = data;
    n->size = size;
    return n;
}

/* Adds missing parent directories of path (archives may omit them). */
static void add_parents(const char *path)
{
    char parent[FS_PATH_MAX];
    size_t len = strlen(path);
    memcpy(parent, path, len + 1);
    for (int depth = 0; depth < MAX_DEPTH; depth++) {
        while (len > 1 && parent[len - 1] != '/')
            len--;
        if (len <= 1)
            return; /* reached the root */
        parent[--len] = '\0';
        if (!find(parent))
            add(parent, true, NULL, 0);
    }
}

size_t fs_init_tar(const uint8_t *data, size_t size)
{
    kfree(nodes);
    nodes = NULL;
    node_count = 0;

    /* First pass: validate and count entries. */
    size_t entries = 0;
    size_t off = 0;
    while (off + TAR_BLOCK <= size && !all_zero(data + off)) {
        const struct tar_header *h = (const struct tar_header *)(data + off);
        bool ok = true;
        uint32_t fsize = parse_octal(h->size, sizeof(h->size), &ok);
        if (memcmp(h->magic, "ustar", 5) != 0 || !checksum_ok(h) || !ok)
            return 0;
        size_t blocks = 1 + (fsize + TAR_BLOCK - 1) / TAR_BLOCK;
        if (fsize > size || blocks * TAR_BLOCK > size - off)
            return 0;
        entries++;
        off += blocks * TAR_BLOCK;
    }

    node_capacity = 1 + entries * (MAX_DEPTH + 1);
    nodes = kcalloc(node_capacity, sizeof(*nodes));
    if (!nodes)
        return 0;
    add("/", true, NULL, 0);

    /* Second pass: add files and directories. */
    off = 0;
    while (off + TAR_BLOCK <= size && !all_zero(data + off)) {
        const struct tar_header *h = (const struct tar_header *)(data + off);
        bool ok = true;
        uint32_t fsize = parse_octal(h->size, sizeof(h->size), &ok);
        char path[FS_PATH_MAX];
        bool is_file = h->typeflag == '0' || h->typeflag == '\0';
        bool is_dir = h->typeflag == '5';
        if ((is_file || is_dir) && make_path(path, h) && !find(path)) {
            add_parents(path);
            add(path, is_dir, is_dir ? NULL : data + off + TAR_BLOCK, is_dir ? 0 : fsize);
        }
        off += (1 + (fsize + TAR_BLOCK - 1) / TAR_BLOCK) * TAR_BLOCK;
    }
    return node_count;
}

const struct fs_node *fs_lookup(const char *path)
{
    if (!nodes || !path || path[0] != '/')
        return NULL;
    char clean[FS_PATH_MAX];
    size_t len = strlen(path);
    while (len > 1 && path[len - 1] == '/')
        len--;
    if (len >= FS_PATH_MAX)
        return NULL;
    memcpy(clean, path, len);
    clean[len] = '\0';
    return find(clean);
}

static bool is_child(const struct fs_node *dir, const struct fs_node *n)
{
    size_t dlen = strlen(dir->path);
    const char *rest;
    if (dlen == 1) { /* root */
        rest = n->path + 1;
    } else {
        if (memcmp(n->path, dir->path, dlen) != 0 || n->path[dlen] != '/')
            return false;
        rest = n->path + dlen + 1;
    }
    if (*rest == '\0')
        return false;
    for (; *rest; rest++) {
        if (*rest == '/')
            return false;
    }
    return true;
}

const struct fs_node *fs_child(const struct fs_node *dir, size_t index)
{
    if (!dir || !dir->is_dir)
        return NULL;
    for (size_t i = 0; i < node_count; i++) {
        if (is_child(dir, &nodes[i]) && index-- == 0)
            return &nodes[i];
    }
    return NULL;
}

const char *fs_basename(const struct fs_node *node)
{
    const char *base = node->path;
    for (const char *p = node->path; *p; p++) {
        if (*p == '/' && p[1])
            base = p + 1;
    }
    return base;
}
