/* SPDX-License-Identifier: GPL-3.0-only */
/* piece_dump FILE...: piece_check() each file; on PE_OK the canonical text (as fm1c's piece.dump()), else "ERR <code>".
 * Files separated by a line "--". Exit 0 when all were read (whatever their verdict), 2 on an I/O error. */
#include <stdio.h>
#include <stdlib.h>
#include "piece.h"

int main(int argc, char **argv)
{
    static uint8_t buf[PIECE_MAX_BYTES + 2];
    int a;
    for (a = 1; a < argc; a++) {
        FILE *f = fopen(argv[a], "rb");
        size_t n;
        int e;
        piece_view_t v;
        uint32_t i;
        if (!f) { perror(argv[a]); return 2; }
        n = fread(buf, 1, sizeof buf, f);
        fclose(f);
        if (a > 1) printf("--\n");
        e = piece_check(buf, (uint32_t)n);
        if (e) { printf("ERR %d\n", e); continue; }
        piece_view(buf, &v);
        printf("piece %s|%u|%u|%u\n", v.name, v.tempo_x10, v.len_ticks, v.nch);
        for (i = 0; i < v.nch; i++) {
            piece_cur_t c;
            piece_note_t no;
            printf("ch %u %u %u %u %u %u\n", i, v.ch[i].role, v.ch[i].sound, v.ch[i].vol, v.ch[i].pan, v.ch[i].src);
            piece_cursor(&v.ch[i], &c);
            while (piece_next(&c, &no)) printf("n %u %u %u %u\n", no.tick, no.dur, no.pitch, no.vel);
        }
    }
    return 0;
}
