/* SPDX-License-Identifier: GPL-3.0-only */
/* US-6: lib.c through its interface on a simulated NOR flash (erase sets 0xFF, a write can only clear bits) that
 * can lose power at any erase or write: a cut applies part of that operation, then nothing more until reboot. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "piece.h"
#include "store.h"
#define REGION (LIB_SECT * 4096u)

static uint8_t fl[REGION];
static uint32_t ops, cut_at, outside, erases;   /* cut_at 0 = never */
static int powered = 1, cut_hit;
static uint32_t rnd_state = 1;
static uint32_t rnd(void) { rnd_state = rnd_state * 1103515245u + 12345u; return rnd_state >> 8; }

static int in_region(uint32_t off, uint32_t n) { return off >= LIB_AT && n <= REGION && off - LIB_AT <= REGION - n; }
static int sim_read(uint32_t off, void *p, uint32_t n)
{
    if (!powered || !in_region(off, n)) return -1;
    memcpy(p, fl + (off - LIB_AT), n);
    return 0;
}
static int sim_cut(void) { return cut_at && ++ops == cut_at; }
static int sim_erase(uint32_t off)
{
    if (!powered) return -1;
    if (!in_region(off, 4096) || (off & 4095u)) { outside++; return -1; }
    if (sim_cut()) { memset(fl + (off - LIB_AT), 0xFF, rnd() % 4096u); powered = 0; cut_hit = 1; return -1; }
    memset(fl + (off - LIB_AT), 0xFF, 4096);
    erases++;
    return 0;
}
static int sim_write(uint32_t off, const void *p, uint32_t n)
{
    const uint8_t *s = p;
    uint32_t i, m = n;
    if (!powered) return -1;
    if (!in_region(off, n)) { outside++; return -1; }
    if (sim_cut()) { m = n ? rnd() % n : 0; powered = 0; cut_hit = 1; }
    for (i = 0; i < m; i++) fl[off - LIB_AT + i] &= s[i];
    return m == n ? 0 : -1;
}
#define LIB_READ(off, p, n) sim_read(off, p, n)
#define LIB_ERASE(off) sim_erase(off)
#define LIB_WRITE(off, p, n) sim_write(off, p, n)
#include "lib.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

static uint8_t buf[LIB_MAX + 16], got[LIB_MAX + 16];
static void fill(uint32_t seed, uint32_t n) { uint32_t i, s = seed * 2654435761u + 1u; for (i = 0; i < n; i++) { s = s * 1664525u + 1013904223u; buf[i] = (uint8_t)(s >> 24); } }
static void erase_all(void) { memset(fl, 0xFF, sizeof fl); powered = 1; cut_at = 0; ops = 0; cut_hit = 0; }
static uint32_t span_of(uint32_t len) { return (LIB_HDR + len + 4095u) / 4096u; }

static void test_6_1_put_remount_newest_first(void)
{
    int32_t a, b, c;
    erase_all();
    lib_mount();
    CHECK(lib_count() == 0 && lib_free() == REGION && lib_room() == LIB_MAX, "empty: %u %u", lib_count(), lib_free());
    fill(1, 1000); a = lib_put("first", buf, 1000);
    fill(2, 32768); b = lib_put("second", buf, 32768);
    fill(3, 4032); c = lib_put("third", buf, 4032);         /* exactly one sector */
    CHECK(a >= 0 && b > a && c > b, "ids %d %d %d", a, b, c);
    lib_mount();
    CHECK(lib_count() == 3, "3 after remount: %u", lib_count());
    CHECK(lib_entry(0)->id == (uint32_t)c && !strcmp(lib_entry(0)->name, "third") && lib_entry(2)->id == (uint32_t)a, "newest first");
    CHECK(lib_entry(1)->len == 32768 && lib_entry(1)->span == 9 && lib_entry(0)->span == 1, "spans");
    fill(2, 32768);
    CHECK(lib_read(1, got, sizeof got) == 32768 && !memcmp(got, buf, 32768), "data intact");
    CHECK(lib_free() == REGION - 11u * 4096u, "free %u", lib_free());
    CHECK(lib_find((uint32_t)b) == 1 && lib_find(9999) == LIB_NONE, "find");
}

static void test_6_3_full_then_delete_reclaims(void)
{
    uint32_t i;
    int32_t ids[9], r;
    erase_all();
    lib_mount();
    erases = 0;
    for (i = 0; i < 8; i++) { fill(10 + i, 32768); ids[i] = lib_put("big", buf, 32768); CHECK(ids[i] >= 0, "put %u", i); }
    fill(99, 32768);
    r = lib_put("one too many", buf, 32768);
    CHECK(r == LIB_FULL && lib_count() == 8, "full: %d", r);
    CHECK(lib_free() == 4096 && lib_room() == 4032, "free %u room %u", lib_free(), lib_room());
    fill(98, 4032);
    CHECK(lib_put("small", buf, 4032) >= 0 && lib_free() == 0 && lib_room() == 0, "the last sector used");
    CHECK(erases == 0, "a fresh flash needs no erase: %u", erases);
    fill(97, 1); CHECK(lib_put("tiny", buf, 1) == LIB_FULL, "nothing left");
    for (i = 0; i < 8; i++) { fill(10 + i, 32768); CHECK(lib_read((uint32_t)lib_find((uint32_t)ids[i]), got, sizeof got) == 32768 && !memcmp(got, buf, 32768), "intact %u", i); }
    CHECK(lib_del((uint32_t)ids[3]) == LIB_OK && lib_count() == 8 && lib_free() == 9u * 4096u, "deleted");
    CHECK(lib_del((uint32_t)ids[3]) == LIB_NONE, "delete twice");
    fill(99, 32768);
    r = lib_put("reclaimed", buf, 32768);                  /* erases the dead sectors */
    CHECK(r >= 0 && lib_count() == 9, "reclaimed: %d", r);
    lib_mount();
    CHECK(lib_count() == 9 && lib_entry(0)->id == (uint32_t)r && lib_read(0, got, sizeof got) == 32768 && !memcmp(got, buf, 32768), "after remount");
    {                                           /* the entry at the very end deleted, then remounted */
        uint32_t k;
        for (k = 0; k < lib_count(); k++) if (lib_entry(k)->first + lib_entry(k)->span == LIB_SECT) break;
        CHECK(k < lib_count() && lib_del(lib_entry(k)->id) == LIB_OK, "delete the last entry");
        lib_mount();
        CHECK(lib_count() == 8, "after deleting the end: %u", lib_count());
    }
    CHECK(outside == 0, "nothing outside the library touched");
}

static void test_6_3_prefers_free_sectors_and_73_entries(void)
{
    uint32_t i;
    int32_t a;
    erase_all(); lib_mount(); erases = 0;
    fill(1, 100); a = lib_put("a", buf, 100);
    CHECK(lib_del((uint32_t)a) == LIB_OK, "del");
    fill(2, 100); CHECK(lib_put("b", buf, 100) >= 0 && erases == 0, "a free sector before a dirty one: %u erases", erases);
    erase_all(); lib_mount();
    for (i = 0; i < LIB_SECT; i++) { fill(i, 50); CHECK(lib_put("t", buf, 50) >= 0, "tiny %u", i); }
    CHECK(lib_count() == 73 && lib_put("t", buf, 50) == LIB_FULL, "73 entries, then full");
    CHECK(lib_del(lib_entry(40)->id) == LIB_OK && lib_count() == 72, "delete from a full table");
    fill(7, 50); CHECK(lib_read(0, got, 50) == 50, "a read into exactly its length");
}

static void test_6_4_foreign_and_corrupt_flash(void)
{
    uint32_t i;
    int32_t a, b;
    memset(fl, 0x00, sizeof fl); powered = 1; cut_at = 0;   /* all zero */
    lib_mount();
    CHECK(lib_count() == 0 && lib_free() == REGION, "zeros: empty, all reclaimable");
    for (i = 0; i < REGION; i++) fl[i] = (uint8_t)(i * 7u + (i >> 9));   /* someone else's store */
    memcpy(fl, "COMP", 4); memcpy(fl + 4096, "SLOT", 4); memcpy(fl + 8192, "LIB1", 4);
    lib_mount();
    CHECK(lib_count() == 0, "foreign: empty");
    fill(5, 5000); a = lib_put("mine", buf, 5000);
    CHECK(a >= 0 && lib_count() == 1, "put on foreign data");
    fill(6, 300); b = lib_put("mine too", buf, 300);
    fl[(uint32_t)lib_entry(1)->first * 4096u + 20u] ^= 0x40;  /* a header byte of the first entry flipped */
    lib_mount();
    CHECK(lib_count() == 1 && lib_entry(0)->id == (uint32_t)b, "corrupt record skipped: %u", lib_count());
    fl[(uint32_t)lib_entry(0)->first * 4096u + LIB_HDR + 10u] ^= 1;   /* a data byte flipped */
    lib_mount();
    CHECK(lib_count() == 0, "corrupt data skipped");
    erase_all(); lib_mount();
    CHECK(lib_count() == 0 && lib_free() == REGION, "erased: empty");
    CHECK(outside == 0, "never outside 0x97000-0xDFFFF");
}

static void test_6_4_hostile_calls(void)
{
    char name[40];
    erase_all(); lib_mount();
    fill(1, 10);
    CHECK(lib_put("x", buf, 0) == LIB_BAD && lib_put("x", buf, LIB_MAX + 1) == LIB_BAD, "lengths");
    CHECK(lib_put(0, buf, 10) >= 0 && !strcmp(lib_entry(0)->name, ""), "no name");
    CHECK(lib_put("with space", buf, 10) >= 0 && !strcmp(lib_entry(0)->name, "with space"), "spaces kept");
    lib_mount();
    CHECK(!strcmp(lib_entry(0)->name, "with space"), "spaces kept after remount");
    memset(name, 'n', sizeof name); name[39] = 0; name[1] = '\n'; name[2] = (char)0xC3;
    CHECK(lib_put(name, buf, 10) >= 0 && strlen(lib_entry(0)->name) == 24 && lib_entry(0)->name[1] == '?' && lib_entry(0)->name[2] == '?', "name cleaned: %s", lib_entry(0)->name);
    CHECK(lib_entry(3) == 0 && lib_read(3, got, 100) == LIB_NONE && lib_read(0, got, 9) == LIB_BAD, "bad reads");
    CHECK(lib_del(12345) == LIB_NONE, "unknown id");
    powered = 0;
    CHECK(lib_put("x", buf, 10) == LIB_EIO && lib_del(lib_entry(0)->id) == LIB_EIO && lib_read(0, got, 100) == LIB_EIO, "dead flash");
    powered = 1;
}

/* ---- 6.2: random operations, a power cut at a random erase or write each run ---- */
typedef struct { uint32_t id, len, seed; } m_t;
static m_t model[80];
static uint32_t nmodel;

static int model_matches(int put_in_flight, uint32_t maybe_seed, uint32_t maybe_len, int maybe_gone_id)
{
    uint32_t i, extra = 0;
    for (i = 0; i < nmodel; i++) {
        int k = lib_find(model[i].id);
        if (k < 0) { if ((int)model[i].id == maybe_gone_id) continue; return 0; }
        fill(model[i].seed, model[i].len);
        if (lib_read((uint32_t)k, got, sizeof got) != (int)model[i].len || memcmp(got, buf, model[i].len)) return 0;
    }
    for (i = 0; i < lib_count(); i++) {
        uint32_t j, known = 0;
        for (j = 0; j < nmodel; j++) known |= lib_entry(i)->id == model[j].id;
        if (known) continue;
        if (!put_in_flight || extra++) return 0;  /* only the put in flight may appear, and complete */
        fill(maybe_seed, maybe_len);
        if (lib_read(i, got, sizeof got) != (int)maybe_len || memcmp(got, buf, maybe_len)) return 0;
    }
    return 1;
}

static void test_6_2_power_cut_anywhere(void)
{
    uint32_t run, bad = 0, cuts = 0;
    for (run = 0; run < 1000; run++) {
        uint32_t op, nops = 8 + rnd() % 20;
        rnd_state = run * 7919u + 3u;
        erase_all();
        if (run % 5 == 4) memset(fl, 0x00, sizeof fl);
        lib_mount();
        nmodel = 0;
        cut_at = 1 + rnd() % 30;
        ops = 0;
        for (op = 0; op < nops; op++) {
            uint32_t kind = rnd() % 10;
            int32_t r;
            int inflight_put = 0, gone = -1;
            uint32_t seed = rnd(), len = rnd() % 5 == 0 ? 20000 + rnd() % 12769 : 1 + rnd() % 6000;
            if (kind < 6) {
                fill(seed, len);
                r = lib_put("p", buf, len);
                if (r >= 0) { model[nmodel].id = (uint32_t)r; model[nmodel].len = len; model[nmodel].seed = seed; nmodel++; }
                inflight_put = r < 0 && r != LIB_FULL;
            } else if (kind < 9 && nmodel) {
                uint32_t k = rnd() % nmodel;
                gone = (int)model[k].id;
                r = lib_del(model[k].id);
                if (r == LIB_OK) { model[k] = model[--nmodel]; gone = -1; }
            } else {
                lib_mount();
            }
            if (cut_hit) {                      /* reboot */
                cuts++;
                powered = 1; cut_hit = 0; cut_at = 0;
                lib_mount();
                if (!model_matches(inflight_put, seed, len, gone)) { bad++; if (bad < 4) printf("run %u op %u: library differs after the cut\n", run, op); }
                {                               /* and the next put works */
                    uint32_t i;
                    int32_t nr;
                    for (i = 0; i < lib_count(); i++) {   /* (what survived is now committed) */
                        uint32_t j, known = 0;
                        for (j = 0; j < nmodel; j++) known |= lib_entry(i)->id == model[j].id;
                        if (!known) { model[nmodel].id = lib_entry(i)->id; model[nmodel].len = len; model[nmodel].seed = seed; nmodel++; }
                    }
                    for (i = 0; i < nmodel;) { if (lib_find(model[i].id) < 0) model[i] = model[--nmodel]; else i++; }
                    fill(seed + 1, 777);
                    nr = lib_put("after", buf, 777);
                    if (nr >= 0) { model[nmodel].id = (uint32_t)nr; model[nmodel].len = 777; model[nmodel].seed = seed + 1; nmodel++; }
                    else if (nr != LIB_FULL) { bad++; printf("run %u: put after the cut failed %d\n", run, nr); }
                }
            }
        }
        lib_mount();
        if (!model_matches(0, 0, 0, -1)) { bad++; if (bad < 4) printf("run %u: library differs at the end\n", run); }
    }
    CHECK(bad == 0 && cuts > 500, "%u bad of 1000 runs (%u cuts)", bad, cuts);
    CHECK(outside == 0, "nothing outside the library touched");
}

int main(void)
{
    test_6_1_put_remount_newest_first();
    test_6_3_full_then_delete_reclaims();
    test_6_3_prefers_free_sectors_and_73_entries();
    test_6_4_foreign_and_corrupt_flash();
    test_6_4_hostile_calls();
    test_6_2_power_cut_anywhere();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
