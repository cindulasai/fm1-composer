/* SPDX-License-Identifier: GPL-3.0-only */
/* edit_run: edit.c on this computer, driven by lines on stdin:
 *   load PATH | i CH TICK PITCH DUR VEL | d CH TICK PITCH | s CH TICK PITCH DUR VEL | u | save PATH
 * Prints "r CODE" for each edit; after each one checks the piece (CHECK <err>) and that an error changed no
 * byte (CHANGED). Exit 2 on an I/O error. */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "piece.h"
#include "edit.c"

int main(void)
{
    static uint8_t buf[PIECE_MAX_BYTES], before[PIECE_MAX_BYTES];
    char line[600], path[512];
    unsigned c, t, p, d, v;
    while (fgets(line, sizeof line, stdin)) {
        int r = -1;
        uint32_t uc, ut;
        FILE *f;
        memcpy(before, buf, sizeof buf);
        if (sscanf(line, "load %511s", path) == 1) {
            size_t n;
            if (!(f = fopen(path, "rb"))) { perror(path); return 2; }
            memset(buf, 0, sizeof buf);
            n = fread(buf, 1, sizeof buf, f);
            fclose(f);
            if (piece_check(buf, (uint32_t)n)) { printf("BADLOAD\n"); return 2; }
            edit_forget();
            continue;
        }
        if (sscanf(line, "save %511s", path) == 1) {
            if (!(f = fopen(path, "wb")) || fwrite(buf, 1, pc_u32(buf + 40), f) != pc_u32(buf + 40)) { perror(path); return 2; }
            fclose(f);
            continue;
        }
        if (sscanf(line, "i %u %u %u %u %u", &c, &t, &p, &d, &v) == 5) r = edit_insert(buf, c, t, p, d, v);
        else if (sscanf(line, "s %u %u %u %u %u", &c, &t, &p, &d, &v) == 5) r = edit_set(buf, c, t, p, d, v);
        else if (sscanf(line, "d %u %u %u", &c, &t, &p) == 3) r = edit_delete(buf, c, t, p);
        else if (line[0] == 'u') r = edit_undo(buf, &uc, &ut);
        else continue;
        printf("r %d\n", r);
        if (piece_check(buf, pc_u32(buf + 40))) printf("CHECK %d\n", piece_check(buf, pc_u32(buf + 40)));
        if (r && memcmp(before, buf, sizeof buf)) printf("CHANGED\n");
    }
    return 0;
}
