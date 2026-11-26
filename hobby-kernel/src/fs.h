#ifndef FS_H
#define FS_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

/* A read-only filesystem built from the initial ramdisk: a tar archive
   (POSIX ustar) loaded by the bootloader as a boot module. Files point
   straight into the archive; nothing is copied. Paths are absolute,
   e.g. "/bin/cat"; the root is "/". */

#define FS_PATH_MAX 128

struct fs_node {
    char path[FS_PATH_MAX];
    bool is_dir;
    const uint8_t *data; /* file contents (NULL for directories) */
    uint32_t size;
};

/* Indexes the archive. Returns the number of nodes (files and directories,
   including the root), or 0 if the data is not a valid archive. */
size_t fs_init_tar(const uint8_t *data, size_t size);

/* NULL if not found. Accepts "/a/b", "/a/b/" and "/" ("." and ".." are not
   supported). */
const struct fs_node *fs_lookup(const char *path);

/* The index-th direct child of a directory, or NULL past the end. */
const struct fs_node *fs_child(const struct fs_node *dir, size_t index);

/* The last path component ("cat" for "/bin/cat", "/" for the root). */
const char *fs_basename(const struct fs_node *node);

#endif
