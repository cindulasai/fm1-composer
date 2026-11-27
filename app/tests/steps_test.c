/* SPDX-License-Identifier: GPL-3.0-only */
/* US-25: the step sequencer. steps.c over edit.c (the grid <-> the piece's notes, undo), player.c's loop lengths
 * (polymeter) and swing, edits while playing (player_hold / player_edited), hostile values. The note calls are
 * recorded on their way to the real synth, as in player_test.c. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "synth.c"

static uint32_t now;
typedef struct { uint32_t frame; uint8_t on, ch, pitch, vel; } rec_t;
static rec_t rec[40000];
static uint32_t nrec;
static void t_on(uint32_t ch, uint32_t p, uint32_t v) { if (nrec < 40000u) rec[nrec++] = (rec_t){now, 1, (uint8_t)ch, (uint8_t)p, (uint8_t)v}; synth_on(ch, p, v); }
static void t_off(uint32_t ch, uint32_t p) { if (nrec < 40000u) rec[nrec++] = (rec_t){now, 0, (uint8_t)ch, (uint8_t)p}; synth_off(ch, p); }
#define PL_NOTE_ON t_on
#define PL_NOTE_OFF t_off
#define PL_CH_OFF synth_ch_off
#define PL_HUSH synth_hush
static uint32_t steps;
#define PL_STEP() (steps++)
#include "player.c"
#include "edit.c"
#include "steps.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

/* ---- a piece builder (piece.h's format, as fm1c's encode() writes it) ---- */
typedef struct { uint32_t tick, dur; uint8_t pitch, vel; } tn_t;
static uint8_t pb[PIECE_MAX_BYTES], want[PIECE_MAX_BYTES];
static void put16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void put32(uint8_t *p, uint32_t v) { put16(p, v & 0xFFFFu); put16(p + 2, v >> 16); }
static uint32_t bs_n, bs_t[8];                  /* sections (v2 when bs_n) */
static uint8_t bs_steps[8], bs_swing[8];        /* each channel's loop length, swing */
static uint32_t build(uint8_t *b, uint32_t tempo, uint32_t len, uint32_t nch, const tn_t *const *notes, const uint32_t *counts)
{
    uint32_t off = PIECE_HDR + nch * PIECE_CHREC, ch, i, n;
    memset(b, 0, PIECE_MAX_BYTES);
    if (bs_n) {
        b[off] = (uint8_t)bs_n;
        for (i = 0; i < bs_n; i++) { put32(b + off + 4 + i * 12, bs_t[i]); b[off + 8 + i * 12] = (uint8_t)('A' + i); }
        off += 4 + bs_n * 12;
    }
    memcpy(b, "F1P1", 4); b[4] = bs_n ? 2 : 1; b[5] = (uint8_t)nch; put16(b + 8, tempo); put16(b + 10, 96); put32(b + 12, len);
    memcpy(b + 16, "steps", 5);
    for (ch = 0; ch < nch; ch++) {
        uint8_t *c = b + PIECE_HDR + ch * PIECE_CHREC;
        uint32_t t = 0;
        c[0] = ROLE_PAD; c[1] = 255; c[2] = 100; c[3] = 64; c[5] = bs_steps[ch]; c[6] = bs_swing[ch];
        put32(c + 8, counts[ch]); put32(c + 12, off);
        for (i = 0; i < counts[ch]; i++) {
            const tn_t *x = &notes[ch][i];
            put16(b + off, x->tick - t); put16(b + off + 2, x->dur); b[off + 4] = x->pitch; b[off + 5] = x->vel;
            t = x->tick; off += 6;
        }
    }
    n = off + 4;
    put32(b + 40, n);
    put32(b + off, piece_crc32(b, off));
    return n;
}
static uint32_t plen(const uint8_t *b) { return pc_u32(b + 40); }

static void play_frames(uint32_t frames)
{
    int32_t out[2 * CTL];
    uint32_t f;
    for (f = 0; f < frames; f += CTL) {
        player_tick(CTL);
        memset(out, 0, sizeof out);
        synth_apply();
        synth_render(out, CTL);
        now += CTL;
    }
}
static uint32_t frames_of(uint32_t ticks, uint32_t tempo) { return (uint32_t)((uint64_t)ticks * PL_TICK_UNITS / (tempo * 96u)); }

static void reset(void)
{
    synth_init(); player_macros(0, 0); nrec = 0; now = 0; pl_want_run = 0; pl_state = 0; pl_running = 0; pl_loop = 1; pl_sloop = 0; steps = 0;
    bs_n = 0; memset(bs_steps, 0, sizeof bs_steps); memset(bs_swing, 0, sizeof bs_swing); edit_forget();
}

/* the pitches channel ch started, in order (at most max) */
static uint32_t ons_of(uint32_t ch, uint8_t *out, uint32_t max)
{
    uint32_t i, k = 0;
    for (i = 0; i < nrec; i++) if (rec[i].on && rec[i].ch == ch && k < max) out[k++] = rec[i].pitch;
    return k;
}
static int balanced(void)                       /* every note-on has its note-off */
{
    static int32_t held[8][128];
    uint32_t i, c, p;
    memset(held, 0, sizeof held);
    for (i = 0; i < nrec; i++) held[rec[i].ch][rec[i].pitch] += rec[i].on ? 1 : -1;
    for (c = 0; c < 8u; c++) for (p = 0; p < 128u; p++) if (held[c][p]) { printf("held %u %u: %d\n", c, p, held[c][p]); return 0; }
    return 1;
}

/* ---- 25.1: the grid <-> the notes ---- */
static void test_25_1_grid_and_notes(void)
{
    static const tn_t none[1] = {{0, 0, 0, 0}};
    const tn_t *nl[2] = {none, none};
    uint32_t cnt[2] = {0, 0}, n, orig, t3 = 3u * ST_TICKS, uc, ut;
    piece_note_t cn[ST_CHORD];
    piece_view_t v;
    uint8_t cell[ST_PAGE], first[PIECE_MAX_BYTES];
    reset();
    orig = build(pb, 1200, 8 * 384, 2, nl, cnt);
    memcpy(first, pb, orig);
    edit_mark();
    CHECK(steps_key(pb, 0, 0, 60, 1, 100) == ED_OK, "C4 at step 1");
    {   /* byte for byte what encode() writes for that one note */
        static const tn_t one[1] = {{0, 24, 60, 100}};
        const tn_t *wl[2] = {one, none};
        uint32_t wc[2] = {1, 0}, wn = build(want, 1200, 8 * 384, 2, wl, wc);
        CHECK(plen(pb) == wn && !memcmp(pb, want, wn), "the bytes are encode()'s");
    }
    edit_mark();                                /* a 4-note chord at step 4, 3 steps long (a tie), velocity 90 */
    CHECK(steps_key(pb, 0, t3, 64, 3, 90) == ED_OK && steps_key(pb, 0, t3, 67, 3, 90) == ED_OK, "chord");
    CHECK(steps_key(pb, 0, t3, 71, 3, 90) == ED_OK && steps_key(pb, 0, t3, 48, 3, 90) == ED_OK, "4 notes");
    memcpy(want, pb, plen(pb));
    CHECK(steps_key(pb, 0, t3, 72, 3, 90) == ED_CHORD && !memcmp(pb, want, plen(want)), "a 5th refused, nothing changed");
    n = steps_cell(pb, 0, t3, cn);
    CHECK(n == 4 && cn[0].pitch == 48 && cn[3].pitch == 71 && cn[0].dur == 72 && cn[0].vel == 90, "the step: %u notes", n);
    CHECK(steps_cell(pb, 0, t3 + ST_TICKS, cn) == 0 && steps_cell(pb, 0, t3 - ST_TICKS, cn) == 0 && steps_cell(pb, 1, t3, cn) == 0,
          "only that step, that channel");
    piece_view(pb, &v);
    steps_row(&v, 0, 0, cell);
    CHECK(cell[0] == 2 && cell[1] == 0 && cell[3] == 2 && cell[4] == 1 && cell[5] == 1 && cell[6] == 0, "the row: starts and ties");
    steps_row(&v, 0, 4u * ST_TICKS, cell);      /* a page from inside the tie */
    CHECK(cell[0] == 1 && cell[1] == 1 && cell[2] == 0, "a note begun before the page sounds through");
    edit_mark();
    CHECK(steps_set(pb, 0, t3, 2, 127) == ED_OK, "length 2, velocity 127");
    n = steps_cell(pb, 0, t3, cn);
    CHECK(n == 4 && cn[1].dur == 48 && cn[2].vel == 127, "set on every note of the step");
    CHECK(steps_set(pb, 0, 5u * ST_TICKS, 2, 100) == ED_NONE, "an empty step");
    edit_mark();
    CHECK(steps_key(pb, 0, t3, 67, 1, 100) == ED_OK && steps_cell(pb, 0, t3, cn) == 3, "the same key again takes it away");
    edit_mark();
    CHECK(edit_chan(pb, 1, PIECE_CH_STEPS, 12) == ED_OK && edit_chan(pb, 1, PIECE_CH_SWING, 60) == ED_OK, "loop, swing");
    CHECK(piece_check(pb, plen(pb)) == PE_OK, "still a piece");
    piece_view(pb, &v);
    CHECK(v.ch[1].steps == 12 && v.ch[1].swing == 60 && v.ch[0].steps == 0, "read back");
    CHECK(steps_key(pb, 0, 8u * 384u, 60, 1, 100) == ED_ARG, "past the end");
    while (edit_undoable()) CHECK(edit_undo(pb, &uc, &ut) == ED_OK, "undo");
    CHECK(plen(pb) == orig && !memcmp(pb, first, orig), "undone to the first byte");
    {   /* an off-grid note (text2midi's) in a step: shown, taken away by its key */
        static const tn_t og[1] = {{30, 10, 62, 80}};
        const tn_t *ol[1] = {og};
        uint32_t oc[1] = {1};
        build(pb, 1200, 384, 1, ol, oc);
        CHECK(steps_cell(pb, 0, ST_TICKS, cn) == 1 && cn[0].tick == 30, "off the grid, in step 2");
        CHECK(steps_key(pb, 0, ST_TICKS, 62, 1, 100) == ED_OK && steps_cell(pb, 0, ST_TICKS, cn) == 0, "removed");
    }
}

/* ---- 25.2: loop lengths (polymeter) ---- */
static tn_t ln0[64], ln1[64];
static void two_tracks(uint32_t len, uint32_t nst)  /* ch 1: 40 + step, ch 2: 70 + step, a note each step */
{
    const tn_t *nl[2] = {ln0, ln1};
    uint32_t cnt[2], i;
    for (i = 0; i < nst; i++) { ln0[i] = (tn_t){i * 24, 20, (uint8_t)(40 + i), 100}; ln1[i] = (tn_t){i * 24, 20, (uint8_t)(70 + i % 16), 100}; }
    cnt[0] = cnt[1] = nst;
    CHECK(piece_check(pb, build(pb, 3000, len, 2, nl, cnt)) == PE_OK, "two tracks");
    CHECK(player_load(pb) == 0, "load");
}

static void test_25_2_polymeter(void)
{
    uint8_t a[80], b[80];
    uint32_t na, nb, i, worst = 0, ok = 1;
    /* a section of 16 steps looping; channel 1 loops 12 steps: 12 against 16, gapless, in phase across the loop */
    reset();
    bs_n = 2; bs_t[0] = 0; bs_t[1] = 384; bs_steps[0] = 12;
    two_tracks(768, 32);
    player_section_loop(1);
    player_play();
    for (i = 0; i < frames_of(48 * 24, 3000) / CTL; i++) {
        uint32_t s0 = steps;
        play_frames(CTL);
        if (steps - s0 > worst) worst = steps - s0;
    }
    player_stop(); play_frames(CTL);
    na = ons_of(0, a, 80); nb = ons_of(1, b, 80);
    CHECK(na == 48 && nb == 48, "48 steps each: %u %u", na, nb);
    for (i = 0; i < 48 && i < na && i < nb; i++) ok &= a[i] == 40 + i % 12 && b[i] == 70 + i % 16;
    CHECK(ok, "12 against 16 (a[13] %u, a[16] %u)", a[13], a[16]);
    for (i = 0, na = 0, nb = 0; i < nrec; i++) if (rec[i].on) { if (rec[i].ch) nb++; else na++; }
    {   /* each step's two notes start in the same block: no gap at a wrap */
        uint32_t f0[48], f1[48], k0 = 0, k1 = 0;
        for (i = 0; i < nrec; i++) if (rec[i].on) { if (rec[i].ch == 0 && k0 < 48) f0[k0++] = rec[i].frame; else if (rec[i].ch == 1 && k1 < 48) f1[k1++] = rec[i].frame; }
        for (i = 0, ok = 1; i < 48 && i < k0 && i < k1; i++) ok &= f0[i] == f1[i];
        CHECK(ok && k0 == 48, "in step");
    }
    CHECK(worst <= 2, "O(1) in the interrupt: at most a cursor copy a channel a block (%u)", worst);
    CHECK(balanced(), "no stuck note");

    /* sections played through (no loop): a looping channel starts again at the next section's start */
    reset();
    bs_n = 2; bs_t[0] = 0; bs_t[1] = 384; bs_steps[0] = 3;
    two_tracks(768, 32);
    pl_loop = 0;
    player_play();
    play_frames(frames_of(768, 3000) + 44100);
    na = ons_of(0, a, 80);
    for (i = 0, ok = na == 32; i < 32 && i < na; i++) ok &= a[i] == 40 + (i / 16) * 16 + i % 16 % 3;
    CHECK(ok, "3-step loops, section by section (%u notes, a[16] %u)", na, a[16]);

    /* a piece without sections, looping: the channel keeps its place in its loop across the piece's end */
    reset();
    bs_steps[0] = 3;
    two_tracks(384, 16);
    player_play();
    play_frames(frames_of(32 * 24, 3000) - 10);
    player_stop(); play_frames(CTL);
    na = ons_of(0, a, 80);
    for (i = 0, ok = na == 32; i < 32 && i < na; i++) ok &= a[i] == 40 + i % 3;
    CHECK(ok, "the piece loops, the 3 steps go on (%u, a[16] %u)", na, a[16]);

    /* as long as the section or longer: no loop; 1 step: the same note every step */
    for (i = 0; i < 3; i++) {
        static const uint8_t L[3] = {16, 64, 1};
        uint32_t k;
        reset();
        bs_steps[0] = L[i];
        two_tracks(384, 16);
        pl_loop = 0;
        player_play();
        play_frames(frames_of(384, 3000) + 4410);
        na = ons_of(0, a, 80);
        for (k = 0, ok = na == 16; k < 16 && k < na; k++) ok &= a[k] == (L[i] == 1 ? 40 : 40 + k);
        CHECK(ok, "loop %u: %u notes", L[i], na);
    }

    /* PLAY from a tick: the loop's place there */
    reset();
    bs_steps[0] = 12;
    two_tracks(384, 16);
    player_play_from(14 * 24);
    play_frames(CTL);
    na = ons_of(0, a, 80);
    CHECK(na == 1 && a[0] == 42, "from step 15 of 16 a 12-step loop is at its 3rd: %u", na ? a[0] : 0);
    player_stop(); play_frames(CTL);
    CHECK(player_local(0) == 0 && player_local(9) == 0, "stopped: its own tick at the start");
}

/* ---- 45.1: the ENERGY and SWING macros ---- */
static uint32_t macro_run(int32_t e, uint32_t ms, uint8_t own, const tn_t *ns, uint32_t n, rec_t *out)
{
    const tn_t *nl[1] = {ns};
    uint32_t cnt[1] = {n};
    reset();
    bs_swing[0] = own;
    CHECK(piece_check(pb, build(pb, 1200, 16 * 24, 1, nl, cnt)) == PE_OK, "macro piece");
    CHECK(player_load(pb) == 0, "load");
    pl_loop = 0;
    player_macros(e, ms);
    player_play();
    play_frames(frames_of(16 * 24, 1200) + 4410);
    memcpy(out, rec, nrec * sizeof *rec);
    return nrec;
}

static void test_45_1_energy_scales_velocity_swing_macro_swings_every_channel(void)
{
    static const uint8_t V[6] = {1, 2, 50, 99, 100, 127};
    static const int32_t E[6] = {-50, -1, 0, 1, 25, 50};
    static tn_t ns[16];
    static rec_t a[64], b[64];
    uint32_t i, k, n, m;
    for (i = 0; i < 16; i++) ns[i] = (tn_t){i * 24, 12, (uint8_t)(60 + i), V[i % 6]};
    for (k = 0; k < 6; k++) {
        uint32_t ok = 1, ons = 0;
        n = macro_run(E[k], 0, 0, ns, 16, a);
        for (i = 0; i < n; i++) {
            int32_t want;
            if (!a[i].on) continue;
            want = ((int32_t)ns[a[i].pitch - 60].vel * (100 + E[k]) + 50) / 100;
            want = want < 1 ? 1 : want > 127 ? 127 : want;
            ok &= a[i].vel == (uint32_t)want;
            if (E[k] == 0) ok &= a[i].vel == ns[a[i].pitch - 60].vel;   /* 0: the piece's own, exactly */
            ons++;
        }
        CHECK(ok && ons == 16, "energy %d: vel x (100 + e) / 100, rounded, 1..127", (int)E[k]);
    }
    n = macro_run(-50, 0, 0, ns, 16, a);
    for (i = 0; i < n; i++) if (a[i].on && ns[a[i].pitch - 60].vel == 1) CHECK(a[i].vel == 1, "vel 1 never silent");
    n = macro_run(50, 0, 0, ns, 16, a);
    for (i = 0; i < n; i++) if (a[i].on && ns[a[i].pitch - 60].vel == 127) CHECK(a[i].vel == 127, "127 stays 127");
    /* the SWING macro: as if the channel's own swing were max(own, macro) */
    {
        static const uint8_t OWN[4] = {0, 0, 100, 30};
        static const uint8_t MS[4] = {60, 100, 60, 30};
        for (k = 0; k < 4; k++) {
            uint8_t eff = OWN[k] > MS[k] ? OWN[k] : MS[k];
            n = macro_run(0, MS[k], OWN[k], ns, 16, a);
            m = macro_run(0, 0, eff, ns, 16, b);
            CHECK(n == m && n == 32 && !memcmp(a, b, n * sizeof *a), "own %u, macro %u: plays as swing %u", OWN[k], MS[k], eff);
        }
        n = macro_run(0, 60, 0, ns, 16, a);
        m = macro_run(0, 0, 0, ns, 16, b);
        CHECK(n == m && memcmp(a, b, n * sizeof *a), "a SWING macro moves the odd steps");
    }
    player_macros(-99, 500);
    CHECK(pl_energy == -50 && pl_mswing == 100, "clamped: -50, 100");
    player_macros(77, 0);
    CHECK(pl_energy == 50 && pl_mswing == 0, "clamped: +50, 0");
    player_macros(0, 0);
}

/* ---- 25.3: swing ---- */
static void test_25_3_swing(void)
{
    static const uint8_t SW[4] = {0, 60, 100, 33};
    uint32_t k, i;
    CHECK(swing_units(60, 24000) == 7200 && swing_units(100, 24000) == 12000 && swing_units(101, 24000) == 12000 &&
          swing_units(-5, 24000) == 0, "SLOOP's formula: 100 = half a step");
    for (k = 0; k < 4; k++) {
        static tn_t ns[34];
        const tn_t *nl[1] = {ns};
        uint32_t cnt[1] = {0}, on = 0, off = 0, ok = 1, dur_ok = 1;
        uint64_t late = (uint64_t)swing_units(SW[k], 24000) * (PL_TICK_UNITS / PL_SUB);
        reset();
        for (i = 0; i < 32; i++) ns[cnt[0]++] = (tn_t){i * 24, i == 5 ? 1 : 20, (uint8_t)(50 + i), 100};
        ns[cnt[0]++] = (tn_t){32 * 24 + 12, 10, 90, 100};   /* off the grid: never swung */
        bs_swing[0] = SW[k];
        CHECK(piece_check(pb, build(pb, 1200, 64 * 24, 1, nl, cnt)) == PE_OK, "swing piece");
        CHECK(player_load(pb) == 0, "load");
        pl_loop = 0;
        player_play();
        play_frames(frames_of(64 * 24, 1200) + 4410);
        for (i = 0; i < nrec; i++) {
            uint64_t exact;
            int64_t d;
            const tn_t *x;
            if (!rec[i].on) continue;
            x = &ns[on++];
            exact = ((uint64_t)x->tick * PL_TICK_UNITS + (x->tick % 48u == 24u ? late : 0u)) / (1200u * 96u);
            d = (int64_t)rec[i].frame - (int64_t)exact;
            ok &= d <= 0 && d > -(int64_t)CTL && rec[i].pitch == x->pitch;
        }
        for (i = 0; i < nrec; i++) {           /* each ends where it would have, never before it starts */
            uint32_t j;
            const tn_t *x;
            if (rec[i].on) continue;
            x = &ns[rec[i].pitch - 50u < 32u ? rec[i].pitch - 50u : 32u];
            for (j = 0; j < i && !(rec[j].on && rec[j].pitch == rec[i].pitch); j++) {}
            dur_ok &= j < i && (rec[i].frame > rec[j].frame || x->dur == 1u) &&
                      (x->dur == 1u || (int64_t)rec[i].frame - (int64_t)frames_of(x->tick + x->dur, 1200) > -(int64_t)CTL);
            off++;
        }
        CHECK(on == 33 && off == 33 && ok, "swing %u: odd steps late by %u/200 of a step (%u notes)", SW[k], SW[k], on);
        CHECK(dur_ok && balanced(), "swing %u: the notes end on time", SW[k]);
    }
    /* swung notes waiting when the piece stops, or the channel is muted, never start */
    {
        static tn_t ns[2] = {{24, 20, 60, 100}, {72, 20, 61, 100}};
        const tn_t *nl[1] = {ns};
        uint32_t cnt[1] = {2};
        reset();
        bs_swing[0] = 100;
        build(pb, 300, 384, 1, nl, cnt);
        player_load(pb);
        player_play();
        play_frames(frames_of(26, 300));
        CHECK(pl_nsw == 1 && nrec == 0, "waiting: %u", pl_nsw);
        player_mute(0, 1);
        play_frames(CTL);
        CHECK(pl_nsw == 0, "muted: dropped");
        player_mute(0, 0);
        play_frames(frames_of(50, 300));
        CHECK(pl_nsw == 1, "the next one waits");
        player_stop(); play_frames(CTL);
        CHECK(pl_nsw == 0 && nrec == 0, "stopped: none started");
    }
}

/* ---- 25.5: hostile ---- */
static void test_25_5_hostile(void)
{
    static tn_t base[8][660];
    const tn_t *nl[8];
    uint32_t cnt[8], ch, s, k, n, full = 0, added = 0, uc, ut;
    piece_view_t v;
    reset();
    for (ch = 0; ch < 8; ch++) {               /* 20 KB of notes past the 64 steps */
        for (k = 0; k < 440; k++) base[ch][k] = (tn_t){64 * 24 + k * 6, 6, (uint8_t)(30 + k % 60), 90};
        nl[ch] = base[ch]; cnt[ch] = 440;
    }
    n = build(pb, 1200, 64 * 24 + 440 * 6 + 24, 8, nl, cnt);
    CHECK(piece_check(pb, n) == PE_OK && n > 20000, "a 20 KB piece: %u", n);
    for (ch = 0; ch < 8 && !full; ch++)        /* 64 steps x 4-note chords on all 8: the 32 KB limit */
        for (s = 0; s < 64 && !full; s++)
            for (k = 0; k < 4 && !full; k++) {
                int r;
                memcpy(want, pb, plen(pb));
                r = steps_key(pb, ch, s * ST_TICKS, 50 + k * 4, 1 + s % 3, 100);
                if (r == ED_FULL) full = 1;
                else if (r == ED_OK) added++;
                else CHECK(0, "ch %u step %u note %u: %d", ch, s, k, r);
            }
    CHECK(full && plen(pb) > 32768 - 32 && !memcmp(pb, want, plen(pb)), "refused at %u bytes, nothing changed", plen(pb));
    CHECK(piece_check(pb, plen(pb)) == PE_OK, "still a piece");
    piece_view(pb, &v);
    for (ch = 0, n = 0; ch < 8; ch++) n += v.ch[ch].nev;
    CHECK(n == 8 * 440 + added && added > 1500, "every note kept: %u added", added);
    /* length 0 / 65, loop 65, swing 101: refused, nothing changed */
    memcpy(want, pb, plen(pb));
    CHECK(steps_key(pb, 7, 63 * ST_TICKS, 99, 0, 100) == ED_ARG && steps_key(pb, 7, 63 * ST_TICKS, 99, 65, 100) == ED_ARG, "len");
    CHECK(steps_set(pb, 0, 0, 0, 100) == ED_ARG && steps_set(pb, 0, 0, 65, 100) == ED_ARG, "set len");
    CHECK(edit_chan(pb, 0, PIECE_CH_STEPS, 65) == ED_ARG && edit_chan(pb, 0, PIECE_CH_SWING, 101) == ED_ARG, "loop 65, swing 101");
    CHECK(edit_chan(pb, 8, PIECE_CH_STEPS, 1) == ED_ARG && edit_chan(pb, 0, 7, 128) == ED_ARG && edit_chan(pb, 0, 4, 1) == ED_ARG, "not a field (US-30: byte 7 = glide 0..127)");
    edit_mark();
    CHECK(edit_chan(pb, 0, PIECE_CH_STEPS, 64) == ED_OK && edit_chan(pb, 0, PIECE_CH_SWING, 100) == ED_OK, "64, 100: the limits");
    CHECK(edit_undo(pb, &uc, &ut) == ED_OK && uc == 0 && !memcmp(pb, want, plen(pb)), "one gesture, one undo");
    {   /* bytes a writer must not make: loop 65, swing 101, glide 128 (US-30: the last reserved byte is the glide) */
        static const uint32_t at[3] = {5, 6, 7};
        static const uint8_t val[3] = {65, 101, 128};
        for (k = 0; k < 3; k++) {
            memcpy(want, pb, plen(pb));
            want[PIECE_HDR + 3 * PIECE_CHREC + at[k]] = val[k];
            put32(want + plen(pb) - 4, piece_crc32(want, plen(pb) - 4));
            CHECK(piece_check(want, plen(pb)) == PE_TABLE, "byte %u = %u refused", at[k], val[k]);
            want[PIECE_HDR + 3 * PIECE_CHREC + at[k]] = (uint8_t)(val[k] - 1u);
            put32(want + plen(pb) - 4, piece_crc32(want, plen(pb) - 4));
            CHECK(piece_check(want, plen(pb)) == (k == 2 ? PE_OK : PE_OK), "byte %u = %u read", at[k], val[k] - 1u);
        }
    }

    /* edits while playing: every 7 blocks one (a key, a length, a loop, a swing, an undo), held; clean */
    reset();
    bs_steps[0] = 12; bs_swing[1] = 60;
    two_tracks(768, 32);
    player_play();
    for (k = 0; k < 400; k++) {
        int r;
        uint32_t t0 = (k * 5u % 32u) * ST_TICKS, tk;
        play_frames(7 * CTL);
        player_hold();
        tk = pl_tick;
        play_frames(CTL);
        CHECK(pl_tick == tk, "held: no tick passes");
        edit_mark();
        switch (k % 5) {
        case 0: r = steps_key(pb, k % 2, t0, 60 + k % 12, 1 + k % 4, 100); break;
        case 1: r = steps_set(pb, k % 2, t0, 1 + k % 5, 80); break;
        case 2: r = edit_chan(pb, k % 2, PIECE_CH_STEPS, k % 20); break;
        case 3: r = edit_chan(pb, k % 2, PIECE_CH_SWING, k % 101); break;
        default: r = edit_undo(pb, &uc, &ut); break;
        }
        player_edited();
        CHECK(r == ED_OK || r == ED_NONE, "edit %u: %d", k, r);
        CHECK(piece_check(pb, plen(pb)) == PE_OK, "edit %u: a piece", k);
    }
    CHECK(player_playing(), "still playing");
    player_stop(); play_frames(CTL);
    CHECK(balanced() && nrec > 200, "no stuck note (%u calls)", nrec);
    CHECK(pl_noff == 0 && pl_nsw == 0, "nothing held");
}

/* a note added while playing plays once at its step; the notes of the tick being played do not play again */
static void test_25_5_edit_while_playing_in_time(void)
{
    uint8_t a[80];
    uint32_t na, i, ok;
    reset();
    two_tracks(384, 16);
    player_play();
    play_frames(frames_of(4 * 24 + 10, 3000));  /* in step 5 */
    player_hold();
    CHECK(steps_key(pb, 0, 8 * ST_TICKS, 100, 1, 100) == ED_OK, "a note at step 9");
    CHECK(steps_key(pb, 0, 4 * ST_TICKS, 101, 1, 100) == ED_OK, "one at step 5 (already passed)");
    player_edited();
    play_frames(frames_of(6 * 24, 3000));       /* to step 11 */
    na = ons_of(0, a, 80);
    for (i = 0, ok = 1; i < 9 && i < na; i++) ok &= a[i] == 40 + i;
    CHECK(na == 12 && ok && a[9] == 100 && a[10] == 49, "in time, once (%u notes, a[9] %u)", na, na > 9 ? a[9] : 0);
    player_stop(); play_frames(CTL);
    reset();                                    /* stopped: player_edited reads the bytes again, from the start */
    two_tracks(384, 16);
    player_hold();
    CHECK(edit_chan(pb, 0, PIECE_CH_STEPS, 2) == ED_OK, "loop 2");
    player_edited();
    CHECK(pl_hold == 0 && pl_lp[0] == 48 && pl_wrap[0] == 48, "read again: loop %u wraps at %u", pl_lp[0], pl_wrap[0]);
}

int main(void)
{
    test_25_1_grid_and_notes();
    test_45_1_energy_scales_velocity_swing_macro_swings_every_channel();
    test_25_2_polymeter();
    test_25_3_swing();
    test_25_5_hostile();
    test_25_5_edit_while_playing_in_time();
    if (fails) { printf("%d failure(s)\n", fails); return 1; }
    printf("ok\n");
    return 0;
}
