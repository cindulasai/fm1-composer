/* SPDX-License-Identifier: GPL-3.0-only */
/* prefs_test: each piece's saved choices: found by id, updated in place, 200 kept, the oldest saved replaced */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include "piece.h"
#include "prefs.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; printf("FAIL %s:%d ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

static void put(uint32_t id, uint32_t tempo)
{
    uint8_t s[PIECE_MAX_CH];
    uint32_t ch;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) s[ch] = (uint8_t)(id + ch);
    pref_put(id, tempo, s, id & 0xFFu);
}

static int has(uint32_t id, uint32_t tempo)
{
    const pref_t *p = pref_find(id);
    uint32_t ch;
    if (!p || p->tempo_x10 != tempo || p->mute != (id & 0xFFu)) return 0;
    for (ch = 0; ch < PIECE_MAX_CH; ch++)
        if (p->sound[ch] != (uint8_t)(id + ch)) return 0;
    return 1;
}

static uint32_t used(void)
{
    uint32_t i, n = 0;
    for (i = 0; i < NPREF; i++) n += prefs.p[i].used;
    return n;
}

int main(void)
{
    uint32_t i;
    prefs_reset();
    CHECK(prefs.magic == PREF_MAGIC && !pref_find(0) && !pref_find(7), "empty");
    put(0, 1200); put(0xFFFFFFFFu, 900); put(7, 1000);
    CHECK(has(0, 1200) && has(0xFFFFFFFFu, 900) && has(7, 1000) && used() == 3, "three, by id (0 and ~0 too)");
    put(7, 1500);
    CHECK(has(7, 1500) && used() == 3, "saved again: updated in place");
    for (i = 100; used() < NPREF; i++) put(i, 1200);
    CHECK(has(0, 1200) && has(i - 1u, 1200), "200 kept");
    put(0, 1300);                                       /* 0 is now saved most recently; ~0 longest ago */
    put(5000, 600);
    CHECK(has(5000, 600) && !pref_find(0xFFFFFFFFu) && has(0, 1300) && has(7, 1500) && used() == NPREF,
          "the 201st replaces the one saved longest ago");
    put(5001, 600);
    CHECK(has(5001, 600) && !pref_find(7) && has(0, 1300), "then the next oldest");
    printf(fails ? "prefs_test: %d FAILED\n" : "prefs_test: ok\n", fails);
    return fails != 0;
}
