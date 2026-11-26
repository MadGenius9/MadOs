/* echo - print the command-line arguments (shows argc/argv passing). */
#include "ulib.h"

int main(int argc, char **argv)
{
    for (int i = 1; i < argc; i++)
        printf("%s%s", argv[i], i + 1 < argc ? " " : "");
    printf("\n");
    return argc - 1;
}
