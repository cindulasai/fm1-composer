/* SPDX-License-Identifier: GPL-3.0-only */
/* fm6_ref: 155-byte FM6 voices on stdin -> per voice one line "hash peak nonzero" of what fm6_core.c renders for a
 * fixed phrase (three notes, a retrigger, note-offs, the patch's LFO). Built twice by tools/tests/test_fm6.py
 * (19.2): with SLOOP's own fm6_core.c and tables (-DFM6_REF_SLOOP -I<sloop-src>/firmware/src -I<SLOOP's generated
 * headers>) and with COMPOSER's (-Iengines); the two must print the same lines. Only fm6_core.c's interface is
 * used, so the reference needs nothing else of SLOOP. */
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#ifdef FM6_REF_SLOOP
#include "felucca_tables.h"                     /* SLOOP's tools/gen_tables.py */
#else
#include "fe_tables.h"                          /* COMPOSER's core tables + SLOOP's FM6 part (gen_fm6_tables.py) */
#include "fm6_tables.h"
#endif
#include "fm6_core.c"                           /* (not in tests/: found through -I, SLOOP's or COMPOSER's) */

#define NV 3
static fm6_note_t note[NV];
static int32_t bus[2][FM6_N];

int main(void)
{
    uint8_t p[FP_SIZE];
    static const int32_t KEYS[NV] = {48, 60, 67}, VEL[NV] = {127, 80, 30};
    while (fread(p, 1, FP_SIZE, stdin) == FP_SIZE) {
        fm6_lfo_t lfo;
        uint32_t hash = 2166136261u, nz = 0, b, i, k, alg = p[FP_ALG] & 31u;
        int32_t peak = 0, fb = p[FP_FB], dt[6] = {0, 0, 0, 0, 0, 0};
        memset(note, 0, sizeof note);
        memset(&lfo, 0, sizeof lfo);
        fm6_lfo_reset(&lfo, p);
        for (b = 0; b < 4000u; b++) {               /* ~2.9 s */
            int32_t lv, ld, out[FM6_N];
            for (k = 0; k < NV; k++) {
                if (b == k * 100u) { fm6_note_init(&note[k], p, KEYS[k], VEL[k], 1); fm6_lfo_key(&lfo); }
                if (b == 1500u + k * 200u) fm6_note_key(&note[k], p, 0);
            }
            if (b == 1000u) fm6_note_init(&note[1], p, KEYS[1] + 2, 100, 0);   /* a retrigger: phases kept */
            lv = fm6_lfo_sample(&lfo);
            ld = fm6_lfo_delay(&lfo);
            for (k = 0; k < NV; k++) {
                if (!note[k].live) continue;
                memset(out, 0, sizeof out);
                if (fm6_note_compute(&note[k], p, out, bus, lv, ld, fm6_note_logfreq((KEYS[k] + p[FP_TRNSP] - 24) * 16, k),
                                     alg, fb, dt, (int32_t)k << 20))
                    for (i = 0; i < FM6_N; i++) {
                        int32_t a = out[i] < 0 ? -out[i] : out[i];
                        hash = (hash ^ (uint32_t)out[i]) * 16777619u;
                        nz += out[i] != 0;
                        if (a > peak) peak = a;
                    }
                if (b > 1500u && fm6_note_done(&note[k], p, alg)) note[k].live = 0;
            }
        }
        printf("%08x %d %u\n", hash, peak, nz);
    }
    return 0;
}
