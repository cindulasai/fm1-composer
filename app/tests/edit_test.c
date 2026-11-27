/* SPDX-License-Identifier: GPL-3.0-only */
/* US-31: edit.c's part tools (shift, x2 / 1/2, transpose) against a model in this file: after each turn the piece
 * is byte for byte what encode() makes of the model's notes (a builder here, rests and all), valid; a refused turn
 * changes no byte; a hold is one undo, back to the bytes it began from. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "piece.h"
#include "edit.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

/* ---- the model: channels of notes; the builder writes them as encode() does ---- */
typedef struct { uint32_t tick, dur; uint8_t pitch, vel; } tn_t;
#define MAXN 6000
typedef struct { uint8_t role, steps, mode; uint32_t n; tn_t note[MAXN]; } mch_t;   /* mode: US-30's nibble */
static mch_t mc[PIECE_MAX_CH];
static uint32_t m_nch, m_len, m_hold, m_hl;
static uint8_t pb[PIECE_MAX_BYTES], want[PIECE_MAX_BYTES], before[PIECE_MAX_BYTES];

static void put16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void put32(uint8_t *p, uint32_t v) { put16(p, v & 0xFFFFu); put16(p + 2, v >> 16); }
static int cmp_tn(const void *a, const void *b)
{
    const tn_t *x = a, *y = b;
    return x->tick != y->tick ? (x->tick < y->tick ? -1 : 1) : (int)x->pitch - (int)y->pitch;
}
static uint32_t plen(const uint8_t *b) { return pc_u32(b + 40); }
static int eqp(const uint8_t *a, const uint8_t *b) { return plen(a) == plen(b) && memcmp(a, b, plen(a)) == 0; }   /* (the room past the end is scratch) */

/* the model as bytes; 0 when it does not fit */
static uint32_t build(uint8_t *b)
{
    uint32_t off = PIECE_HDR + m_nch * PIECE_CHREC, ch, i, modes = 0;
    memset(b, 0, PIECE_MAX_BYTES);
    memcpy(b, "F1P1", 4); b[4] = 1; b[5] = (uint8_t)m_nch; put16(b + 8, 1200); put16(b + 10, 96); put32(b + 12, m_len);
    memcpy(b + 16, "tools", 5);
    for (ch = 0; ch < m_nch; ch++) {
        uint8_t *c = b + PIECE_HDR + ch * PIECE_CHREC;
        uint32_t t = 0, k = 0;
        qsort(mc[ch].note, mc[ch].n, sizeof(tn_t), cmp_tn);
        c[0] = mc[ch].role; c[1] = 255; c[2] = 100; c[3] = 64; c[5] = mc[ch].steps;
        put32(c + 12, off);
        for (i = 0; i < mc[ch].n; i++) {
            const tn_t *x = &mc[ch].note[i];
            uint32_t gap = x->tick - t;
            for (; gap > 65535u; gap -= 65535u, k++, off += 6) {
                if (off + 10 > PIECE_MAX_BYTES) return 0;
                put16(b + off, 65535); put16(b + off + 2, 1); b[off + 4] = 255; b[off + 5] = 1;
            }
            if (off + 10 > PIECE_MAX_BYTES) return 0;
            put16(b + off, gap); put16(b + off + 2, x->dur); b[off + 4] = x->pitch; b[off + 5] = x->vel;
            t = x->tick; off += 6; k++;
        }
        put32(c + 8, k);
        modes |= (uint32_t)mc[ch].mode << 4u * ch;
    }
    put32(b + 40, off + 4);
    put32(b + 44, modes);
    put32(b + off, piece_crc32(b, off));
    return off + 4;
}

static void load(void)                          /* the model into pb, a new piece for edit.c */
{
    uint32_t n = build(pb);
    CHECK(n && piece_check(pb, n) == 0, "model piece valid (%u B)", n);
    edit_forget();
}

static void same(const char *what)               /* pb == the model, and valid */
{
    uint32_t n = build(want);
    CHECK(n && plen(pb) == n && memcmp(pb, want, n) == 0, "%s: bytes as encode() makes them (%u vs %u)", what, plen(pb), n);
    CHECK(piece_check(pb, plen(pb)) == 0, "%s: piece_check", what);
}

static void mreset(uint32_t nch, uint32_t len)
{
    memset(mc, 0, sizeof mc);
    m_nch = nch; m_len = len; m_hold = 0;
    for (uint32_t c = 0; c < nch; c++) mc[c].role = ROLE_LEAD;
}
static void add(uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel)
{
    mc[ch].note[mc[ch].n++] = (tn_t){tick, dur, (uint8_t)pitch, (uint8_t)vel};
}

/* ---- the tools on the model (the part's length L as edit.c keeps it for the hold) ---- */
static uint32_t m_part(uint32_t ch)
{
    uint32_t L, last = 0, i;
    if (mc[ch].steps) L = mc[ch].steps * 24u;
    else if (m_hold) L = m_hl;
    else { for (i = 0; i < mc[ch].n; i++) if (mc[ch].note[i].tick > last) last = mc[ch].note[i].tick; L = (last / 384u + 1u) * 384u; }
    return L > m_len ? m_len : L;
}
static void m_shift(uint32_t ch, int32_t d, uint32_t g)
{
    uint32_t L = m_part(ch), i, s = (uint32_t)(((int64_t)d * g % L + L) % L);
    for (i = 0; i < mc[ch].n; i++) if (mc[ch].note[i].tick < L) mc[ch].note[i].tick = (mc[ch].note[i].tick + s) % L;
    m_hl = L; m_hold = 1;
}
static void m_x2(uint32_t ch)
{
    uint32_t L = m_part(ch), i, n = mc[ch].n, k = 0;
    static tn_t keep[MAXN];
    for (i = 0; i < n; i++) {
        tn_t x = mc[ch].note[i];
        if (x.tick < L) { keep[k++] = x; if (x.tick + L < m_len) { x.tick += L; keep[k++] = x; } }
        else if (x.tick >= 2u * L) keep[k++] = x;
    }
    memcpy(mc[ch].note, keep, k * sizeof(tn_t)); mc[ch].n = k;
    mc[ch].steps = (uint8_t)(mc[ch].steps * 2u);
    m_hl = 2u * L < m_len ? 2u * L : m_len; m_hold = 1;
}
static void m_half(uint32_t ch)
{
    uint32_t L = m_part(ch), half = mc[ch].steps ? mc[ch].steps / 2u * 24u : L / 2u, i, k = 0;
    for (i = 0; i < mc[ch].n; i++) if (mc[ch].note[i].tick < half || mc[ch].note[i].tick >= L) mc[ch].note[k++] = mc[ch].note[i];
    mc[ch].n = k;
    mc[ch].steps = (uint8_t)(mc[ch].steps / 2u);
    m_hl = half; m_hold = 1;
}
static void m_trans(uint32_t ch, int32_t d)
{
    for (uint32_t i = 0; i < mc[ch].n; i++) mc[ch].note[i].pitch = (uint8_t)(mc[ch].note[i].pitch + d);
    if (!m_hold) m_hl = m_part(ch);
    m_hold = 1;
}

static int undo1(void) { uint32_t c, t; return edit_undo(pb, &c, &t); }

static void refused(int r, int want_r, const char *what)
{
    CHECK(r == want_r, "%s: %d (want %d)", what, r, want_r);
    CHECK(memcmp(pb, before, PIECE_MAX_BYTES) == 0, "%s: nothing changed", what);
}

/* ---- 31.1 ---- */
static void test_31_1_shift_wraps_in_the_part(void)
{
    mreset(2, 4u * 384u);
    mc[0].steps = 4;                            /* a 4-step loop: 96 ticks */
    add(0, 0, 60, 24, 100); add(0, 48, 62, 24, 90); add(0, 72, 64, 30, 80); add(0, 200, 70, 10, 70);   /* (200: past the loop) */
    add(1, 0, 40, 96, 100); add(1, 900, 43, 96, 100);
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_SHIFT, 1, 24) == ED_OK, "shift +1");
    m_shift(0, 1, 24); same("31.1 shift +1 in a loop (72 wraps to 0, 200 stays)");
    CHECK(edit_tool(pb, 0, ET_SHIFT, -3, 24) == ED_OK, "shift -3");
    m_shift(0, -3, 24); same("31.1 shift -3");
    CHECK(undo1() == ED_OK, "undo"); m_hold = 0;
    CHECK(eqp(pb, before), "31.1 the hold's two turns: one undo");
    CHECK(undo1() == ED_NONE, "31.1 nothing more to undo");
    edit_tools_end();
    /* no loop: the bars up to the last note (bar 3 here: 1152 ticks) */
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 1, ET_SHIFT, -1, 96) == ED_OK, "shift -1 1/4"); m_hold = 0;
    m_shift(1, -1, 96); same("31.1 shift -1 without a loop: 0 wraps to 1056");
    CHECK(mc[1].note[0].tick == 804 && mc[1].note[1].tick == 1056, "model: %u %u", mc[1].note[0].tick, mc[1].note[1].tick);
}

static void test_31_1_x2_and_half(void)
{
    mreset(3, 8u * 384u);
    mc[1].steps = 4;
    add(0, 10, 50, 5, 100);
    add(1, 0, 60, 24, 100); add(1, 24, 61, 24, 100); add(1, 120, 99, 24, 100); add(1, 400, 70, 24, 100);   /* 120: replaced */
    add(2, 5, 30, 5, 100);
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 1, ET_LEN, 1, 0) == ED_OK, "x2"); m_hold = 0;
    m_x2(1); same("31.1 x2 a loop: copied after itself, 8 steps");
    CHECK(pb[PIECE_HDR + PIECE_CHREC + PIECE_CH_STEPS] == 8, "loop 8 steps");
    CHECK(edit_tool(pb, 1, ET_LEN, 1, 0) == ED_OK && edit_tool(pb, 1, ET_LEN, 1, 0) == ED_OK && edit_tool(pb, 1, ET_LEN, 1, 0) == ED_OK, "x2 to 64");
    m_x2(1); m_x2(1); m_x2(1); same("31.1 x2 to 64 steps");
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 1, ET_LEN, 1, 0), ED_LONG, "31.1 x2 past 64 steps");
    CHECK(edit_tool(pb, 1, ET_LEN, -1, 0) == ED_OK, "1/2"); m_half(1); same("31.1 1/2: the first half kept, 32 steps");
    while (mc[1].steps > 1) { CHECK(edit_tool(pb, 1, ET_LEN, -1, 0) == ED_OK, "1/2"); m_half(1); }
    same("31.1 1/2 down to 1 step");
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 1, ET_LEN, -1, 0), ED_SHORT, "31.1 1/2 of 1 step");
    CHECK(undo1() == ED_OK && !eqp(pb, before), "undo: to before the refusals' hold (x2 x4 up to 64)");
    /* no loop: the part = bar 1 (the last note at 400 -> 2 bars); x2 up to the piece's end */
    mreset(1, 3u * 384u);
    add(0, 0, 60, 24, 100); add(0, 384, 62, 24, 100); add(0, 400, 64, 24, 100);   /* 384: its copy at 1152 = the end */
    load();
    CHECK(edit_tool(pb, 0, ET_LEN, 1, 0) == ED_OK, "x2 no loop"); m_hold = 0;
    m_x2(0); same("31.1 x2 without a loop: bar 1-2 copied to 3, cut at the end");
    CHECK(mc[0].n == 4, "31.1 the copies at and past the end dropped (%u)", mc[0].n);
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 0, ET_LEN, 1, 0), ED_LONG, "31.1 x2 when the part fills the piece");
    CHECK(edit_tool(pb, 0, ET_LEN, -1, 0) == ED_OK, "1/2 no loop"); m_half(0); same("31.1 1/2 without a loop");
}

static void test_31_1_transpose_and_refusals(void)
{
    mreset(2, 4u * 384u);
    add(0, 0, 60, 24, 100); add(0, 0, 64, 24, 100); add(0, 96, 126, 24, 100); add(0, 200, 1, 24, 100);
    mc[1].role = ROLE_DRUMS; add(1, 0, 36, 24, 100);
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_TRANS, 1, 0) == ED_OK, "+1"); m_hold = 0;
    m_trans(0, 1); same("31.1 transpose +1");
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 0, ET_TRANS, 1, 0), ED_RANGE, "31.1 transpose past 127");
    CHECK(edit_tool(pb, 0, ET_TRANS, -2, 0) == ED_OK, "-2"); m_trans(0, -2); same("31.1 transpose -2");
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 0, ET_TRANS, -1, 0), ED_RANGE, "31.1 transpose below 0");
    refused(edit_tool(pb, 1, ET_TRANS, 1, 0), ED_DRUMS, "31.1 drums do not transpose");
    mc[1].role = ROLE_PERC; load(); memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 1, ET_TRANS, -1, 0), ED_DRUMS, "31.1 percussion does not transpose");
    CHECK(edit_tool(pb, 1, ET_SHIFT, 1, 24) == ED_OK, "31.1 drums shift");
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 5, ET_SHIFT, 1, 24), ED_ARG, "31.1 no such channel");
}

/* ---- 31.3 ---- */
static void test_31_3_empty_full_big(void)
{
    uint32_t i, t;
    mreset(2, 4u * 384u);
    add(1, 0, 60, 24, 100);
    load();
    memcpy(before, pb, sizeof pb);
    for (t = 0; t < 3u; t++) refused(edit_tool(pb, 0, t, t == ET_LEN ? -1 : 1, 24), ED_EMPTY, "31.3 an empty channel");
    refused(edit_tool(pb, 0, ET_LEN, 1, 0), ED_EMPTY, "31.3 x2 of an empty channel");
    CHECK(undo1() == ED_NONE, "31.3 nothing logged");
    /* a full piece: channel 1 small, channel 2 fills the rest; x2 does not fit: nothing half-done */
    mreset(2, 999u * 384u);
    for (i = 0; i < 40; i++) add(0, i * 24u, 60, 12, 100);
    for (i = 0; i < (PIECE_MAX_BYTES - PIECE_HDR - 2u * PIECE_CHREC - 40u * 6u - 4u) / 6u; i++) add(1, i * 10u, 50 + i % 20, 5, 90);
    load();
    CHECK(PIECE_MAX_BYTES - plen(pb) < 12u, "the piece is full (%u B)", plen(pb));
    memcpy(before, pb, sizeof pb);
    refused(edit_tool(pb, 0, ET_LEN, 1, 0), ED_FULL, "31.3 x2 past the note limit");
    CHECK(undo1() == ED_NONE, "31.3 a refused turn logs nothing");
    CHECK(edit_tool(pb, 0, ET_SHIFT, 1, 24) == ED_OK, "31.3 a full piece still shifts"); m_shift(0, 1, 24); same("shift full");
    CHECK(edit_tool(pb, 0, ET_LEN, -1, 0) == ED_OK, "31.3 and halves"); m_half(0); same("1/2 full");
    CHECK(edit_tool(pb, 0, ET_TRANS, 3, 0) == ED_OK, "31.3 and transposes"); m_trans(0, 3); same("transpose full");
    CHECK(undo1() == ED_OK && eqp(pb, before), "31.3 one undo back");
    memcpy(before, pb, sizeof pb);              /* channel 2: more events than a hold can keep */
    refused(edit_tool(pb, 1, ET_TRANS, 1, 0), ED_BIG, "31.3 a part too big to undo");
}

static void test_31_3_fifty_turns_one_undo_and_history(void)
{
    uint32_t i, c, t;
    mreset(2, 16u * 384u);
    for (i = 0; i < 30; i++) add(0, i * 48u + 6u, 48 + i % 24, 30, 100);
    add(1, 0, 40, 24, 100);
    load();
    edit_mark(); CHECK(edit_insert(pb, 1, 96, 41, 24, 100) == ED_OK, "an edit before");
    add(1, 96, 41, 24, 100);
    memcpy(before, pb, sizeof pb);
    m_hold = 0;
    for (i = 0; i < 50u; i++) {
        uint32_t k = i % 5u;
        int r = k == 0 ? edit_tool(pb, 0, ET_SHIFT, i % 2 ? -1 : 1, 24) : k == 1 ? edit_tool(pb, 0, ET_TRANS, 1, 0)
              : k == 2 ? edit_tool(pb, 0, ET_LEN, 1, 0) : k == 3 ? edit_tool(pb, 0, ET_LEN, -1, 0) : edit_tool(pb, 0, ET_TRANS, -1, 0);
        CHECK(r == ED_OK, "turn %u: %d", i, r);
        if (k == 0) m_shift(0, i % 2 ? -1 : 1, 24); else if (k == 1) m_trans(0, 1); else if (k == 2) m_x2(0); else if (k == 3) m_half(0); else m_trans(0, -1);
        same("31.3 a turn of 50");
    }
    CHECK(edit_undo(pb, &c, &t) == ED_OK && c == 0, "undo the hold");
    CHECK(eqp(pb, before), "31.3 50 turns in one hold: one undo");
    CHECK(undo1() == ED_OK, "the edit before is still there to undo");
    CHECK(undo1() == ED_NONE, "then nothing");
    /* two holds: the second drops the first from the history (one kept, as passes) */
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_TRANS, 1, 0) == ED_OK, "hold 1");
    edit_tools_end();
    memcpy(want, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_TRANS, 1, 0) == ED_OK, "hold 2");
    CHECK(undo1() == ED_OK && eqp(pb, want), "31.3 undo: back to after hold 1");
    CHECK(undo1() == ED_NONE, "31.3 only the last hold is kept");
    /* another change inside a hold ends it: the next turn is a hold of its own */
    load();
    memcpy(before, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_TRANS, 1, 0) == ED_OK, "turn");
    edit_mark(); CHECK(edit_insert(pb, 1, 192, 50, 24, 100) == ED_OK, "a key");
    memcpy(want, pb, sizeof pb);
    CHECK(edit_tool(pb, 0, ET_TRANS, 1, 0) == ED_OK, "turn after the key");
    CHECK(undo1() == ED_OK && eqp(pb, want), "31.3 the turn after the key: a hold of its own, undone alone");
    CHECK(undo1() == ED_OK && undo1() == ED_NONE, "31.3 then the key; the first hold left the history (one hold kept)");
}

/* ---- many pieces: random notes (long gaps: rests), random holds, against the model ---- */
static uint32_t rs = 12345u;
static uint32_t rnd(uint32_t n) { rs = rs * 1103515245u + 12345u; return (rs >> 8) % n; }

static void test_31_1_random_holds_match_the_model(void)
{
    uint32_t it, ch, i, turns = 0;
    int f0 = fails;
    for (it = 0; it < 400u; it++) {
        uint32_t nch = 1u + rnd(4), sel;
        mreset(nch, it % 4u == 0 ? 999u * 384u : (1u + rnd(32)) * 384u - rnd(2) * rnd(200));
        for (ch = 0; ch < nch; ch++) {
            uint32_t n = rnd(it % 4u == 0 ? 12 : 60);
            mc[ch].steps = rnd(3) ? 0 : (uint8_t)(1 + rnd(64));
            mc[ch].role = rnd(5) ? ROLE_LEAD : ROLE_DRUMS;
            for (i = 0; i < n; i++) {
                uint32_t t = rnd(m_len), p = 20 + rnd(90), k;
                for (k = 0; k < mc[ch].n && !(mc[ch].note[k].tick == t && mc[ch].note[k].pitch == p); k++) {}
                if (k == mc[ch].n) add(ch, t, p, 1 + rnd(400), 1 + rnd(127));
            }
        }
        if (!build(want)) continue;
        load();
        memcpy(before, pb, sizeof pb);
        sel = rnd(nch);
        m_hold = 0;
        for (i = 0; i < 12u; i++) {
            uint32_t tool = rnd(3), g = 12u << rnd(4);
            int32_t d = tool == ET_LEN ? (rnd(2) ? 1 : -1) : (int32_t)rnd(7) - 3;
            uint8_t prev[PIECE_MAX_BYTES];
            int r;
            if (!d) d = 1;
            memcpy(prev, pb, sizeof pb);
            r = edit_tool(pb, sel, tool, d, g);
            if (r != ED_OK) { CHECK(memcmp(prev, pb, sizeof pb) == 0, "refused %d changed bytes", r); continue; }
            if (tool == ET_SHIFT) m_shift(sel, d, g); else if (tool == ET_TRANS) m_trans(sel, d); else if (d > 0) m_x2(sel); else m_half(sel);
            same("31.1 random turn");
            turns++;
            if (fails > f0) { printf("piece %u, turn %u: tool %u d %d g %u ch %u steps %u\n", it, i, tool, d, g, sel, mc[sel].steps); return; }
        }
        if (!eqp(pb, before)) {
            CHECK(undo1() == ED_OK, "undo");
            CHECK(eqp(pb, before), "31.1 the random hold: one undo back (piece %u)", it);
        }
        edit_tools_end();
    }
    CHECK(turns > 1500u, "enough turns went through (%u)", turns);
}

/* ---- US-39: a channel added / removed ---- */
static uint32_t plen0(void)                     /* the model's size as built */
{
    uint32_t b = PIECE_HDR + m_nch * PIECE_CHREC + 4u, ch;
    for (ch = 0; ch < m_nch; ch++) b += mc[ch].n * 6u;
    return b;
}
static void m_remove(uint32_t k)
{
    for (uint32_t j = k; j + 1u < m_nch; j++) mc[j] = mc[j + 1u];
    memset(&mc[m_nch - 1u], 0, sizeof mc[0]);
    m_nch--;
}
static void test_39_1_add_and_remove_match_encode(void)
{
    uint32_t run, i;
    srand(39);
    for (run = 0; run < 300; run++) {
        uint32_t n = 1u + (uint32_t)rand() % PIECE_MAX_CH, ch, k;
        mreset(n, 16u * 384u);
        for (ch = 0; ch < n; ch++) {
            mc[ch].role = (uint8_t)(rand() % ROLE_COUNT);
            mc[ch].mode = (uint8_t)(rand() % 4);
            mc[ch].steps = (uint8_t)(rand() % 3 * 16);
            for (i = (uint32_t)rand() % 40; i > 0; i--) add(ch, (uint32_t)rand() % (16u * 384u), 30u + (uint32_t)rand() % 60u, 1u + (uint32_t)rand() % 200u, 1u + (uint32_t)rand() % 127u);
            for (i = 0; i + 1u < mc[ch].n; i++) mc[ch].note[i].pitch = (uint8_t)(mc[ch].note[i].pitch + i % 2);   /* (pairs may clash: dedupe) */
        }
        for (ch = 0; ch < n; ch++) {            /* no two notes at one tick and pitch, as a piece has */
            uint32_t a, b, w = 0;
            for (a = 0; a < mc[ch].n; a++) {
                for (b = 0; b < w && (mc[ch].note[b].tick != mc[ch].note[a].tick || mc[ch].note[b].pitch != mc[ch].note[a].pitch); b++) {}
                if (b == w) mc[ch].note[w++] = mc[ch].note[a];
            }
            mc[ch].n = w;
        }
        load();
        if (n < PIECE_MAX_CH) {
            uint32_t r = (uint32_t)rand() % ROLE_COUNT;
            CHECK(edit_chan_add(pb, r, 255u) == ED_OK, "run %u: add to %u", run, n);
            memset(&mc[n], 0, sizeof mc[0]);
            mc[n].role = (uint8_t)r; mc[n].mode = 1u;   /* AUTO */
            m_nch++;
            same("add");
            CHECK(edit_undoable() == 0, "add: the history forgotten");
        }
        if (m_nch > 1u) {
            k = (uint32_t)rand() % m_nch;
            CHECK(edit_chan_remove(pb, k) == ED_OK, "run %u: remove %u of %u", run, k, m_nch);
            m_remove(k);
            same("remove");
        }
    }
}
static void test_39_3_refusals_change_no_byte(void)
{
    uint32_t n, k;
    mreset(8, 4u * 384u);
    load();
    memcpy(want, pb, PIECE_MAX_BYTES);
    CHECK(edit_chan_add(pb, ROLE_BASS, 255u) == ED_ARG && memcmp(pb, want, PIECE_MAX_BYTES) == 0, "9th channel refused");
    CHECK(edit_chan_remove(pb, 8) == ED_ARG && memcmp(pb, want, PIECE_MAX_BYTES) == 0, "no channel 9");
    mreset(1, 4u * 384u);
    add(0, 0, 60, 96, 100);
    load();
    memcpy(want, pb, PIECE_MAX_BYTES);
    CHECK(edit_chan_remove(pb, 0) == ED_ARG && memcmp(pb, want, PIECE_MAX_BYTES) == 0, "the only channel stays");
    CHECK(edit_chan_add(pb, ROLE_COUNT, 255u) == ED_ARG && memcmp(pb, want, PIECE_MAX_BYTES) == 0, "no such role");
    mreset(2, 999u * 384u);                     /* full to the last few bytes: no room for 16 more */
    for (k = 0; plen0() + 6u <= PIECE_MAX_BYTES - 4u; k++) add(0, k * 24u, 60u + k % 12u, 10, 90);
    n = build(pb);
    CHECK(n + PIECE_CHREC > PIECE_MAX_BYTES && piece_check(pb, n) == 0, "near full (%u B)", n);
    edit_forget();
    memcpy(want, pb, PIECE_MAX_BYTES);
    CHECK(edit_chan_add(pb, ROLE_BASS, 255u) == ED_FULL && memcmp(pb, want, PIECE_MAX_BYTES) == 0, "no room: ED_FULL");
    CHECK(edit_chan_remove(pb, 1) == ED_OK, "a removal still fits");
    m_remove(1);
    same("remove from a full piece");
}

int main(void)
{
    test_39_1_add_and_remove_match_encode();
    test_39_3_refusals_change_no_byte();
    test_31_1_shift_wraps_in_the_part();
    test_31_1_x2_and_half();
    test_31_1_transpose_and_refusals();
    test_31_3_empty_full_big();
    test_31_3_fifty_turns_one_undo_and_history();
    test_31_1_random_holds_match_the_model();
    printf(fails ? "edit_test: %d FAILED\n" : "edit_test: ok\n", fails);
    return fails != 0;
}
