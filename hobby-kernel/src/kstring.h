#ifndef KSTRING_H
#define KSTRING_H

#include <stddef.h>

/* GCC may emit calls to memset/memcpy/memmove/memcmp even in freestanding
   code, so these keep their standard names. */
void *memset(void *dst, int value, size_t n);
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
int memcmp(const void *a, const void *b, size_t n);
size_t strlen(const char *s);
int strcmp(const char *a, const char *b);

#endif
