/* ls - list directories of the ramdisk using the readdir system call. */
#include "ulib.h"

static int list(const char *path)
{
    char name[128];
    int n;
    unsigned i = 0;
    while ((n = readdir(path, i, name, sizeof(name))) > 0) {
        puts(name);
        i++;
    }
    if (n < 0) {
        printf("ls: %s: %s\n", path, strerror(n));
        return 1;
    }
    return 0;
}

int main(int argc, char **argv)
{
    if (argc < 2)
        return list("/");
    int status = 0;
    for (int i = 1; i < argc; i++) {
        if (argc > 2)
            printf("%s:\n", argv[i]);
        status |= list(argv[i]);
    }
    return status;
}
