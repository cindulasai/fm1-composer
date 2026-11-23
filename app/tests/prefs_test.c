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

static void fxput(uint32_t id, uint8_t base)
{
    uint8_t v[PIECE_MAX_CH][PREF_FX];
    memset(v, base, sizeof v);
    fxpref_put(id, (const uint8_t (*)[PREF_FX])v);
}

static void test_18_6_effects_kept_by_id(void)
{
    uint32_t i;
    const fxpref_t *p;
    fxprefs_reset();
    CHECK(fxprefs.magic == FXP_MAGIC && !fxpref_find(1), "empty");
    for (i = 1; i <= NFXP; i++) fxput(i, (uint8_t)i);
    for (i = 1; i <= NFXP; i++) CHECK((p = fxpref_find(i)) && p->v[7][5] == (uint8_t)i, "kept %u", i);
    fxput(3, 99);                                   /* again: in place */
    CHECK(fxpref_find(3)->v[0][0] == 99, "updated in place");
    fxput(500, 7);                                  /* full: the one saved longest ago (1) gives way */
    CHECK(!fxpref_find(1) && fxpref_find(500) && fxpref_find(2) && fxpref_find(3), "oldest replaced");
    fxput(501, 7);
    CHECK(!fxpref_find(2) && fxpref_find(3), "then the next oldest (3 was saved again)");
    CHECK(sizeof fxprefs <= 4084u && NFXP >= 73u, "a blob holds more than the library's pieces");
}

static void test_18_6_stamps_renumbered_before_they_wrap(void)
{
    uint32_t i;
    fxprefs_reset();
    for (i = 1; i <= 3u; i++) fxput(i, 1);
    fxprefs.stamp = 0xFFFEu;                        /* (as after 65534 saves) */
    fxprefs.p[0].stamp = 0xFFF0u, fxprefs.p[1].stamp = 0xFFF5u, fxprefs.p[2].stamp = 0xFFFEu;
    fxput(4, 1);
    CHECK(fxprefs.p[0].stamp == 1u && fxprefs.p[1].stamp == 6u && fxprefs.p[2].stamp == 15u, "renumbered: %u %u %u",
          fxprefs.p[0].stamp, fxprefs.p[1].stamp, fxprefs.p[2].stamp);
    CHECK(fxpref_find(4)->stamp == 16u && fxprefs.stamp == 16u, "the new one newest: %u", fxpref_find(4)->stamp);
    for (i = 5; i < 5u + NFXP - 4u; i++) fxput(i, 1);
    fxput(900, 1);
    CHECK(!fxpref_find(1) && fxpref_find(2), "order kept: the oldest still goes first");
}

int main(void)
{
    test_18_6_effects_kept_by_id();
    test_18_6_stamps_renumbered_before_they_wrap();
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
