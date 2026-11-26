/* cat - print files from the ramdisk using open/read/write system calls. */
#include "ulib.h"

int main(int argc, char **argv)
{
    if (argc < 2) {
        puts("usage: cat FILE...");
        return 2;
    }
    int status = 0;
    for (int i = 1; i < argc; i++) {
        int fd = open(argv[i]);
        if (fd < 0) {
            printf("cat: %s: %s\n", argv[i], strerror(fd));
            status = 1;
            continue;
        }
        char buf[256];
        int n;
        while ((n = read(fd, buf, sizeof(buf))) > 0)
            write(buf, (size_t)n);
        close(fd);
    }
    return status;
}
