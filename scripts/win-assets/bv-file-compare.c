/* Exit 0 only after both complete binary streams compare equal without errors. */
#include <stdio.h>
#include <string.h>

int main(int argc, char **argv) {
    unsigned char left[65536], right[65536];
    FILE *a, *b;
    int status = 0;
    if (argc != 3) {
        fputs("usage: bv-file-compare LEFT RIGHT\n", stderr);
        return 2;
    }
    a = fopen(argv[1], "rb");
    if (!a) return 2;
    b = fopen(argv[2], "rb");
    if (!b) { fclose(a); return 2; }
    for (;;) {
        size_t na = fread(left, 1, sizeof(left), a);
        size_t nb = fread(right, 1, sizeof(right), b);
        if (ferror(a) || ferror(b)) { status = 2; break; }
        if (na != nb || memcmp(left, right, na) != 0) {
            status = 1;
            break;
        }
        if (feof(a) && feof(b)) break;
        if (na == 0 || nb == 0) { status = 2; break; }
    }
    if (fclose(a) != 0) status = 2;
    if (fclose(b) != 0) status = 2;
    if (status != 0) fputs("binary comparison failed\n", stderr);
    return status;
}
