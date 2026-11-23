/* SPDX-License-Identifier: GPL-3.0-only */
/* US-24: live.c (the live keyboard in the piece's key, auto-chords, MIDI IN) and synth.c's live voices, through their
 * interfaces: SLOOP's scales and chords as copied (24.1, 24.2), 4 of the 16 voices kept for live notes (24.3), and
 * hostile input: every key at once, MIDI's edges, running status, lost note-offs (24.5). The host e2e is
 * tools/tests/test_live.py. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#define VCHK(k) ((k) < NVOICE ? (k) : (abort(), 0u))   /* every voice index in range */
#include "synth.c"

static void ui_fmt(char *b, uint32_t max, const char *f, ...)   /* (ui.c's: %s %u %c, as vsnprintf does them) */
{
    va_list ap;
    va_start(ap, f);
    vsnprintf(b, max, f, ap);
    va_end(ap);
}

typedef struct { uint8_t on, ch, note, vel; } lev_t;
static lev_t ev[8192];
static uint32_t nev;
static void t_on(uint32_t ch, uint32_t note, uint32_t vel) { if (nev < 8192u) ev[nev++] = (lev_t){1, (uint8_t)ch, (uint8_t)note, (uint8_t)vel}; synth_live_on(ch, note, vel); }
static void t_off(uint32_t ch, uint32_t note) { if (nev < 8192u) ev[nev++] = (lev_t){0, (uint8_t)ch, (uint8_t)note, 0}; synth_live_off(ch, note); }
#define LK_ON t_on
#define LK_OFF t_off
#include "live.c"

static uint32_t midi_in_q[64], mi_w, mi_r;      /* the jack's parser (midi_uart.c), as on the FM-1 */
#define MQ 64u
/* (RING_PUBLISH: sloop_glue.c's, empty) */
static volatile uint8_t midi_in_lost;
#include "midi_uart.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

static void run(uint32_t blocks)
{
    int32_t out[2 * CTL];
    while (blocks--) { memset(out, 0, sizeof out); synth_apply(); synth_render(out, CTL); }
}
static uint32_t live_gated(void)                /* live voices still held (gate on) */
{
    uint32_t ch, k, n = 0;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < NVOICE; k++) n += (vlive[ch] >> k & 1u) && trk[ch].v[k].active && trk[ch].v[k].gate;
    return n;
}
static uint32_t held_on(void)                   /* LK_ON minus LK_OFF per (channel, note), summed over the positive */
{
    static int cnt[NPART][128];
    uint32_t i, n = 0, ch, k;
    memset(cnt, 0, sizeof cnt);
    for (i = 0; i < nev; i++) cnt[ev[i].ch][ev[i].note] += ev[i].on ? 1 : -1;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < 128u; k++) n += cnt[ch][k] > 0 ? (uint32_t)cnt[ch][k] : 0u;
    return n;
}

/* ---- a piece builder (piece.h's format): channel 1 pitched, channel 2 drums; v2 with key root + 1 / scale ---- */
static uint8_t pb[PIECE_MAX_BYTES];
static void put16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void put32(uint8_t *p, uint32_t v) { put16(p, v & 0xFFFFu); put16(p + 2, v >> 16); }
static uint32_t build(int v2, uint32_t key, uint32_t scale, const uint8_t *pitch, const uint16_t *dur, uint32_t n)
{
    uint32_t off = PIECE_HDR + 2u * PIECE_CHREC, i, ch;
    memset(pb, 0, sizeof pb);
    if (v2) {
        pb[off] = 1; pb[off + 1] = (uint8_t)key; pb[off + 2] = (uint8_t)scale;
        memcpy(pb + off + 8, "A", 1);
        off += 4u + PIECE_SECT;
    }
    memcpy(pb, "F1P1", 4); pb[4] = v2 ? 2 : 1; pb[5] = 2; put16(pb + 8, 1200); put16(pb + 10, 96); put32(pb + 12, 96u * n + 96u);
    memcpy(pb + 16, "live", 4);
    for (ch = 0; ch < 2u; ch++) {
        uint8_t *c = pb + PIECE_HDR + ch * PIECE_CHREC;
        c[0] = ch ? ROLE_DRUMS : ROLE_CHORDS; c[1] = 255; c[2] = 100; c[3] = 64;
        put32(c + 8, n); put32(c + 12, off);
        for (i = 0; i < n; i++) {                /* (the drums: the same pitches, ten times as long: never counted) */
            put16(pb + off, i ? 96 : 0); put16(pb + off + 2, ch ? 10u * dur[i] : dur[i]); pb[off + 4] = ch ? (uint8_t)(pitch[i] + 1u) : pitch[i]; pb[off + 5] = 100;
            off += 6;
        }
    }
    put32(pb + 40, off + 4);
    put32(pb + off, piece_crc32(pb, off));
    return off + 4;
}

static int in_scale(uint32_t note, uint32_t root, uint32_t scale) { return SCALE_MASK[scale] >> ((note + 120u - root) % 12u) & 1u; }
static int white(uint32_t k) { return (int)(0xAB5u >> ((53u + k) % 12u) & 1u); }   /* C D E F G A B */

/* ------------------------------------------------------------------ 24.1 --- */
static void test_24_1_carried_key_white_keys_walk_the_scale(void)
{
    static const uint8_t p[] = {60, 64, 67};
    static const uint16_t d[] = {96, 96, 96};
    piece_view_t v;
    uint32_t len = build(1, 3, 3, p, d, 3), k, last = 0;             /* D (2 + 1), DOR */
    CHECK(piece_check(pb, len) == PE_OK, "a v2 piece with a key reads: %d", piece_check(pb, len));
    piece_view(pb, &v);
    CHECK(v.key == 3 && v.scale == 3, "the view carries it: %u %u", v.key, v.scale);
    lk_open(&v);                                 /* (the notes say C major: the carried key wins) */
    CHECK(lk.t.p[P_ROOT] == 2 && lk_scale() == 3, "D dorian: %d %u", lk.t.p[P_ROOT], lk_scale());
    CHECK(kb_pitch(&lk.t, 7, 0) == 62, "C4 plays the root, D4: %u", kb_pitch(&lk.t, 7, 0));
    for (k = 0; k < LK_KEYS; k++) {
        uint32_t n = kb_pitch(&lk.t, k, 0);
        if (!white(k)) { CHECK(n == KB_SILENT, "black key %u silent: %u", k, n); continue; }
        CHECK(in_scale(n, 2, 3), "white key %u plays a scale note: %u", k, n);
        CHECK(!last || (n > last && n - last <= 4u), "the next scale note up: %u after %u", n, last);
        last = n;
    }
    CHECK(kb_pitch(&lk.t, 7, 1) == 74 && kb_pitch(&lk.t, 7, -2) == 38, "OCT+ / OCT- move by octaves");
    len = build(1, 0, 0, p, d, 3);              /* a key byte 0: none (pieces before US-24) */
    CHECK(piece_check(pb, len) == PE_OK, "no key reads");
    piece_view(pb, &v);
    CHECK(v.key == 0 && v.scale == 0, "no key in the view");
    pb[PIECE_HDR + 2 * PIECE_CHREC + 1] = 13; put32(pb + len - 4, piece_crc32(pb, len - 4));
    CHECK(piece_check(pb, len) == PE_SECT, "root 13 refused");
    pb[PIECE_HDR + 2 * PIECE_CHREC + 1] = 0; pb[PIECE_HDR + 2 * PIECE_CHREC + 2] = 2; put32(pb + len - 4, piece_crc32(pb, len - 4));
    CHECK(piece_check(pb, len) == PE_SECT, "a scale without a key refused");
    pb[PIECE_HDR + 2 * PIECE_CHREC + 1] = 1; pb[PIECE_HDR + 2 * PIECE_CHREC + 2] = 16; put32(pb + len - 4, piece_crc32(pb, len - 4));
    CHECK(piece_check(pb, len) == PE_SECT, "scale 16 refused");
    pb[PIECE_HDR + 2 * PIECE_CHREC + 2] = 15; pb[PIECE_HDR + 2 * PIECE_CHREC + 3] = 1; put32(pb + len - 4, piece_crc32(pb, len - 4));
    CHECK(piece_check(pb, len) == PE_SECT, "the reserved byte still 0");
    pb[PIECE_HDR + 2 * PIECE_CHREC + 3] = 0; put32(pb + len - 4, piece_crc32(pb, len - 4));
    CHECK(piece_check(pb, len) == PE_OK, "B (12), DIMWH (15) read");
}

static void test_24_1_estimated_from_the_notes(void)
{
    static const uint8_t am[] = {57, 60, 64, 69, 72, 76, 59, 62, 64, 57, 65, 69, 72, 64, 68, 71, 57, 60, 64, 57};   /* A minor */
    static const uint8_t gm[] = {55, 59, 62, 67, 60, 64, 67, 62, 66, 69, 55, 59, 62, 71, 74, 67, 62, 66, 69, 55};   /* G major */
    static const uint8_t ef[] = {51, 55, 58, 63, 56, 60, 63, 58, 62, 65, 51, 55, 58, 63, 67, 70, 58, 62, 65, 51};   /* Eb major */
    static const uint16_t d[] = {192, 96, 96, 192, 96, 96, 96, 96, 96, 192, 96, 96, 96, 96, 96, 96, 192, 96, 96, 384};
    static const uint8_t none[] = {60};
    piece_view_t v;
    uint32_t r, s;
    build(0, 0, 0, am, d, 20); piece_view(pb, &v); lk_estimate(&v, &r, &s);
    CHECK(r == 9 && s == 2, "A minor: %u %u", r, s);
    build(0, 0, 0, gm, d, 20); piece_view(pb, &v); lk_estimate(&v, &r, &s);
    CHECK(r == 7 && s == 1, "G major: %u %u", r, s);
    build(1, 0, 0, ef, d, 20); piece_view(pb, &v); lk_open(&v);
    CHECK(lk.t.p[P_ROOT] == 3 && lk_scale() == 1, "a v2 piece without a key: estimated, Eb major: %d %u", lk.t.p[P_ROOT], lk_scale());
    build(0, 0, 0, none, d, 1); piece_view(pb, &v); v.ch[0].role = ROLE_PERC; lk_estimate(&v, &r, &s);
    CHECK(r == 0 && s == 1, "nothing pitched: C major: %u %u", r, s);
}

static void test_24_1_sloops_16_scales_and_chromatic(void)
{
    static const uint16_t WANT[16] = {0xFFF, 0xAB5, 0x5AD, 0x6AD, 0x6B5, 0x295, 0x4A9, 0x9AD, 0x5AB, 0xAD5, 0x56B, 0xAAD,
                                      0x4E9, 0x555, 0x6DB, 0xB6D};
    uint32_t s, r, k;
    CHECK(NSCALES == 16u, "16 scales: %u", (uint32_t)NSCALES);
    for (s = 0; s < 16u; s++) {
        CHECK(SCALE_MASK[s] == WANT[s], "scale %u (%s): %03X", s, LK_SCALE_NAME[s], SCALE_MASK[s]);
        for (r = 0; r < 12u; r++) {
            uint32_t last = 0, seen = 0;
            lk_key(r, s);
            CHECK(lk_scale() == s && lk.t.p[P_QUANT] == 2, "WHITE mode");
            for (k = 0; k < LK_KEYS; k++) {
                uint32_t n = kb_pitch(&lk.t, k, 0);
                if (!white(k)) continue;
                CHECK(in_scale(n, r, s) && n > last, "scale %u root %u key %u: %u", s, r, k, n);
                if (k == 7) CHECK(n == 60 + r, "C4 = the root: %u", n);
                seen |= 1u << ((n + 120u - r) % 12u);
                last = n;
            }
            CHECK(seen == SCALE_MASK[s], "16 white keys cover the scale %u: %03X", s, seen);
        }
    }
    lk_key(5, LK_CHROMATIC);
    CHECK(lk_scale() == LK_CHROMATIC && lk.t.p[P_QUANT] == 0, "CHROMATIC");
    for (k = 0; k < LK_KEYS; k++) CHECK(kb_pitch(&lk.t, k, 0) == 53 + k && kb_pitch(&lk.t, k, 1) == 65 + k, "every key as it is: %u", k);
    lk_key(14, 99);
    CHECK(lk.t.p[P_ROOT] == 2 && lk_scale() == LK_CHROMATIC && lk.t.p[P_SCALE] == 0, "root mod 12, past the scales: chromatic");
    lk_key(0, LK_CHROMATIC);
    CHECK(lk.t.p[P_SCALE] == 0, "CHROMATIC keeps SLOOP's scale 0 (CHR): its chords are minor");
    lk_key(0, 15);
    CHECK(lk.t.p[P_SCALE] == 15 && lk.t.p[P_QUANT] == 2, "15: DIMWH, WHITE");
}

/* SLOOP's kb_pitch beyond what the PLAYER sets: SNAP (QNT 1) and TRN, against their rule; scale_mask's clamp */
static void test_24_1_kb_pitch_snap_transpose_and_clamp(void)
{
    uint32_t i, k;
    srand(2411);
    memset(&lk, 0, sizeof lk);
    for (i = 0; i < 4000u; i++) {
        uint32_t root = (uint32_t)rand() % 12u, scale = (uint32_t)rand() % 16u, n0, mask = SCALE_MASK[scale];
        int32_t oct = rand() % 7 - 3, trn = rand() % 49 - 24, n, guard = 12;
        k = (uint32_t)rand() % LK_KEYS;
        lk.t.p[P_ROOT] = (int16_t)root; lk.t.p[P_SCALE] = (int16_t)scale; lk.t.p[P_CHORD] = 0; lk.t.p[P_TRANS] = (int16_t)trn;
        lk.t.p[P_QUANT] = 1;                    /* SNAP: rounded down into the scale */
        n = 53 + (int32_t)k + 12 * oct + trn;
        while (guard-- > 0 && !(mask >> ((uint32_t)(n - (int32_t)root + 120) % 12u) & 1u)) n--;
        n = n < 0 ? 0 : n > 127 ? 127 : n;
        CHECK(kb_pitch(&lk.t, k, oct) == (uint32_t)n, "SNAP key %u oct %d trn %d root %u scale %u: %u, not %d", k, oct, trn, root, scale, kb_pitch(&lk.t, k, oct), n);
        lk.t.p[P_QUANT] = 0;                    /* OFF: as it is, plus TRN */
        n = 53 + (int32_t)k + 12 * oct + trn;
        CHECK(kb_pitch(&lk.t, k, oct) == (uint32_t)(n < 0 ? 0 : n > 127 ? 127 : n), "OFF + TRN");
        lk.t.p[P_QUANT] = 2;                    /* WHITE: TRN shifts what the white key plays */
        lk.t.p[P_TRANS] = 0;
        n0 = kb_pitch(&lk.t, k, oct);
        lk.t.p[P_TRANS] = (int16_t)trn;
        if (n0 != KB_SILENT) {
            n = (int32_t)n0 + trn;
            if (n0 > 0u && n0 < 127u) CHECK(kb_pitch(&lk.t, k, oct) == (uint32_t)(n < 0 ? 0 : n > 127 ? 127 : n), "WHITE + TRN");
        }
    }
    lk.t.p[P_SCALE] = 20;
    CHECK(scale_mask(&lk.t) == SCALE_MASK[15], "an index past 15: DIMWH");
    lk.t.p[P_SCALE] = -3;
    CHECK(scale_mask(&lk.t) == SCALE_MASK[0], "below 0: CHR");
    memset(&lk, 0, sizeof lk);
}

static void test_24_1_label(void)
{
    char b[40];
    memset(&lk, 0, sizeof lk);
    lk_key(2, 3); lk.t.p[P_CHORD] = 1; lk.oct = 1;
    lk_label(b, sizeof b);
    CHECK(!strcmp(b, "KEYS D DOR TRIAD OCT+1"), "%s", b);
    lk_key(0, LK_CHROMATIC); lk.t.p[P_CHORD] = 0; lk.oct = -2;
    lk_label(b, sizeof b);
    CHECK(!strcmp(b, "KEYS CHROMATIC OCT-2"), "%s", b);
    lk_key(11, 12); lk.oct = 0; lk.t.p[P_CHORD] = 5;
    lk_label(b, sizeof b);
    CHECK(!strcmp(b, "KEYS B BLUES POWER"), "%s", b);
    lk.oct = 3; lk_label(b, sizeof b);
    CHECK(!strcmp(b, "KEYS B BLUES POWER OCT+3"), "%s", b);
    lk.oct = -3; lk.t.p[P_CHORD] = 0; lk_label(b, sizeof b);
    CHECK(!strcmp(b, "KEYS B BLUES OCT-3"), "%s", b);
}

/* ------------------------------------------------------------------ 24.2 --- */
static uint32_t chord_of(uint32_t k, uint32_t ch, uint8_t *c)     /* key k pressed alone on ch: the notes it started */
{
    uint32_t i, n0 = nev, n = 0;
    lk_keys(1u << k, 1u << k, ch);
    for (i = n0; i < nev; i++) if (ev[i].on) { CHECK(ev[i].ch == ch && ev[i].vel == LK_VEL, "on the selected channel"); if (n < 8u) c[n++] = ev[i].note; }
    lk_keys(0, 0, ch);
    return n;
}
static uint32_t pcs(const uint8_t *c, uint32_t n, uint32_t root)
{
    uint32_t i, m = 0;
    for (i = 0; i < n; i++) m |= 1u << ((c[i] + 120u - root) % 12u);
    return m;
}

static void test_24_2_auto_chords_of_the_degree(void)
{
    uint8_t c[8];
    uint32_t n, i;
    static const uint8_t pc[] = {60, 64, 67};
    static const uint16_t d[] = {96, 96, 96};
    piece_view_t v;
    synth_init();
    build(1, 1, 1, pc, d, 3); piece_view(pb, &v);
    memset(&lk, 0, sizeof lk);
    lk_open(&v);                                 /* C major; voice leading on */
    lk.t.p[P_VLEAD] = 0;
    lk.t.p[P_CHORD] = 1;                         /* TRIAD */
    n = chord_of(7, 3, c);
    CHECK(n == 3 && c[0] == 60 && c[1] == 64 && c[2] == 67, "C4: C E G, %u notes", n);
    n = chord_of(9, 3, c);
    CHECK(n == 3 && pcs(c, n, 0) == (1u << 2 | 1u << 5 | 1u << 9), "D4: D F A (the degree's diatonic triad)");
    n = chord_of(1, 3, c);
    CHECK(n == 0, "a black key alone: a modifier, no note");
    lk.t.p[P_CHORD] = 2;
    n = chord_of(14, 3, c);
    CHECK(n == 4 && pcs(c, n, 0) == (1u << 7 | 1u << 11 | 1u << 2 | 1u << 5), "G4 7TH: G B D F");
    lk.t.p[P_CHORD] = 3;
    n = chord_of(7, 3, c);
    CHECK(n == 4 && pcs(c, n, 0) == (1u << 0 | 1u << 4 | 1u << 11 | 1u << 2), "9TH: 1 3 7 9");
    lk.t.p[P_CHORD] = 4;
    n = chord_of(7, 3, c);
    CHECK(n == 3 && pcs(c, n, 0) == (1u << 0 | 1u << 5 | 1u << 7), "SUS4: C F G");
    lk.t.p[P_CHORD] = 5;
    n = chord_of(7, 3, c);
    CHECK(n == 3 && c[0] == 60 && c[1] == 67 && c[2] == 72, "POWER: 1 5 8");
    /* CHORD+: a black key held, then the white key (or both in one pass) */
    lk.t.p[P_CHORD] = 1;
    {
        static const struct { uint32_t key; uint32_t want; uint32_t n; const char *what; } M[] = {
            {1, 1u << 0 | 1u << 3 | 1u << 7, 3, "F#: the minor third"},
            {3, 1u << 0 | 1u << 4 | 1u << 7 | 1u << 11, 4, "G#: +7th"},
            {5, 1u << 0 | 1u << 5 | 1u << 7, 3, "A#: sus4"},
            {8, 1u << 0 | 1u << 4 | 1u << 7 | 1u << 2, 4, "C#: +9th"},
        };
        for (i = 0; i < 4u; i++) {
            uint32_t j, n0;
            n0 = nev;
            lk_keys(1u << M[i].key | 1u << 7, 1u << M[i].key | 1u << 7, 2);   /* (one pass: the modifier counts) */
            for (n = 0, j = n0; j < nev; j++) if (ev[j].on && n < 8u) c[n++] = ev[j].note;
            lk_keys(0, 0, 2);
            CHECK(n == M[i].n && pcs(c, n, 0) == M[i].want, "%s: %u notes, pcs %03X", M[i].what, n, pcs(c, n, 0));
        }
        {
            uint32_t j, n0;
            lk_keys(1u << 10, 1u << 10, 2);     /* D# held first: the inversion, the lowest an octave up */
            n0 = nev;
            lk_keys(1u << 10 | 1u << 7, 1u << 7, 2);
            for (n = 0, j = n0; j < nev; j++) if (ev[j].on && n < 8u) c[n++] = ev[j].note;
            lk_keys(0, 0, 2);
            CHECK(n == 3 && c[0] == 64 && c[1] == 67 && c[2] == 72, "D#: E G C (%u %u %u)", c[0], c[1], c[2]);
        }
    }
    lk_key(0, LK_CHROMATIC);                    /* CHROMATIC with chords: the white keys, minor chords (SLOOP: CHR -> MIN) */
    lk.t.p[P_CHORD] = 1;
    n = chord_of(7, 3, c);
    CHECK(n == 3 && c[0] == 60 && c[1] == 63 && c[2] == 67, "C4: C Eb G (%u %u %u)", c[0], c[1], c[2]);
    CHECK(chord_of(8, 3, c) == 0, "a black key: a modifier");
    lk.t.p[P_CHORD] = 0;
    n = chord_of(8, 3, c);
    CHECK(n == 1 && c[0] == 61, "chords off: C#4 as it is");
    CHECK(held_on() == 0, "every chord ended with its key");
}

/* chord_play_notes against SLOOP's rule written out again (degrees counted on the scale, CHORD+, then the inversion
 * and octave with the least motion from the channel's last chord, inside 24..108, the first such kept; D# inverts):
 * random keys, scales, chord types, modifiers, channels; every note 0..127 as the key's */
static uint8_t r_prev[NPART][4], r_n[NPART];
static int32_t r_up(int32_t n, int32_t deg, uint32_t root, uint32_t mask)
{
    int32_t m = n, g = 48;
    while (deg > 0 && g-- > 0) { m++; if (mask >> ((uint32_t)(m - (int32_t)root + 120) % 12u) & 1u) deg--; }
    return m;
}
static void r_sort(int32_t *c, uint32_t n)
{
    uint32_t a, b;
    for (a = 0; a < n; a++) for (b = a + 1; b < n; b++) if (c[b] < c[a]) { int32_t x = c[a]; c[a] = c[b]; c[b] = x; }
}
static uint32_t r_chord(uint32_t root, uint32_t scale, uint32_t type, uint32_t vlead, uint32_t part, int32_t n, uint32_t mods, int32_t *c)
{
    static const int DEG[6][4] = {{0, -1, -1, -1}, {0, 2, 4, -1}, {0, 2, 4, 6}, {0, 2, 6, 8}, {0, 3, 4, -1}, {0, -1, -1, -1}};
    uint32_t mask = scale ? SCALE_MASK[scale > 15u ? 15u : scale] : SCALE_MASK[2], k = 0, j;
    if (type == 5u) {
        static const int PW[3] = {0, 7, 12};
        for (j = 0; j < 3u; j++) if (n + PW[j] < 128) c[k++] = n + PW[j];
        return k;                                /* (POWER: as it is, no modifiers, no voice leading) */
    }
    for (j = 0; j < 4u && DEG[type][j] >= 0; j++) { int32_t m = r_up(n, DEG[type][j], root, mask); if (m < 128) c[k++] = m; }
    if (k >= 2u && mods) {
        int32_t third = r_up(n, 2, root, mask);
        for (j = 0; j < k; j++) {
            if (c[j] == third && (mods & CM_SUS4)) c[j] = r_up(n, 3, root, mask);
            else if (c[j] == third && (mods & CM_MINOR)) c[j] = c[j] - n == 4 ? c[j] - 1 : c[j] - n == 3 ? c[j] + 1 : c[j];
        }
        if ((mods & CM_SEVEN) && k < 4u) {
            int32_t s7 = r_up(n, 6, root, mask), have = 0;
            for (j = 0; j < k; j++) have |= c[j] == s7;
            if (!have && s7 < 128) c[k++] = s7;
        }
        if (mods & CM_NINE) {
            int32_t s9 = r_up(n, 8, root, mask), s5 = r_up(n, 4, root, mask), have = 0, at = -1;
            for (j = 0; j < k; j++) { have |= c[j] == s9; if (at < 0 && c[j] == s5) at = (int32_t)j; }
            if (!have && s9 < 128) { if (k < 4u) c[k++] = s9; else c[at >= 0 ? (uint32_t)at : k - 1u] = s9; }
        }
    }
    r_sort(c, k);
    if (vlead && r_n[part] && k) {
        int32_t best[4], cand[4], bc = -1, inv, oct;
        for (inv = 0; inv < (int32_t)k; inv++)
            for (oct = -1; oct <= 1; oct++) {
                int32_t cost = 0, ok = 1;
                for (j = 0; j < k; j++) {
                    cand[j] = c[j] + 12 * oct + ((int32_t)j < inv ? 12 : 0);
                    ok &= cand[j] >= 24 && cand[j] <= 108;
                }
                if (!ok) continue;
                r_sort(cand, k);
                for (j = 0; j < k; j++) { int32_t d = cand[j] - r_prev[part][j < r_n[part] ? j : r_n[part] - 1u]; cost += d < 0 ? -d : d; }
                if (bc < 0 || cost < bc) { bc = cost; memcpy(best, cand, sizeof best); }
            }
        if (bc >= 0) memcpy(c, best, k * sizeof *c);
    }
    if ((mods & CM_INV) && k >= 2u && c[0] + 12 < 128) { c[0] += 12; r_sort(c, k); }
    for (j = 0; j < k; j++) r_prev[part][j] = (uint8_t)c[j];
    r_n[part] = (uint8_t)k;
    return k;
}

static void test_24_2_chords_as_sloops_rule_random(void)
{
    uint32_t i, j, bad = 0;
    srand(242);
    memset(&lk, 0, sizeof lk);
    memset(vl_n, 0, sizeof vl_n); memset(vl_prev, 0, sizeof vl_prev);
    memset(r_n, 0, sizeof r_n); memset(r_prev, 0, sizeof r_prev);
    for (i = 0; i < 60000u && bad < 5u; i++) {
        uint32_t root = (uint32_t)rand() % 12u, scale = (uint32_t)rand() % 17u, type = (uint32_t)rand() % 6u, vlead = (uint32_t)rand() % 4u != 0u;
        uint32_t part = (uint32_t)rand() % NPART, mods = (uint32_t)rand() % 32u, n = (uint32_t)rand() % 128u, k, kr;
        uint8_t c[4];
        int32_t want[4];
        if (rand() % 4 == 0) mods = 0;
        if (rand() % 3 == 0) n = 60u + (uint32_t)rand() % 30u;     /* (more voice leading inside 24..108) */
        lk.t.p[P_ROOT] = (int16_t)root; lk.t.p[P_SCALE] = (int16_t)scale; lk.t.p[P_CHORD] = (int16_t)type; lk.t.p[P_VLEAD] = (int16_t)vlead;
        lk.t.part = (uint8_t)part;
        k = chord_play_notes(&lk.t, n, mods, c);
        kr = r_chord(root, scale, type, vlead, part, (int32_t)n, mods, want);
        if (type == 5u) {                        /* (POWER keeps SLOOP's own path: the inversion, voice leading) */
            memcpy(want, (int32_t[4]){0}, sizeof want);
            for (j = 0; j < k; j++) want[j] = c[j];
            kr = k;
            for (j = 0; j < k; j++) r_prev[part][j] = c[j];
            r_n[part] = (uint8_t)k;
        }
        for (j = 0; j < k && j < kr; j++) if (c[j] != want[j]) break;
        if (k != kr || j < k) {
            bad++;
            CHECK(0, "#%u root %u scale %u type %u vlead %u part %u n %u mods %u: %u notes %u %u %u %u, want %u: %d %d %d %d", i, root,
                  scale, type, vlead, part, n, mods, k, c[0], c[1], c[2], c[3], kr, want[0], want[1], want[2], want[3]);
        }
        CHECK(k <= 4u && vl_n[part] == k, "at most 4 notes, remembered for the channel");
    }
}

static void test_24_2_power_chords(void)
{
    uint8_t c[4];
    uint32_t n, k;
    memset(&lk, 0, sizeof lk);
    memset(vl_n, 0, sizeof vl_n);
    lk.t.p[P_CHORD] = 5;
    for (n = 0; n < 128u; n++) {                /* 1 5 8, what fits under 128 */
        k = chord_notes(&lk.t, n, c);
        CHECK(k == (n + 12u < 128u ? 3u : n + 7u < 128u ? 2u : 1u) && c[0] == n && (k < 2u || c[1] == n + 7u) && (k < 3u || c[2] == n + 12u),
              "POWER on %u: %u notes", n, k);
    }
    k = chord_play_notes(&lk.t, 60, CM_INV | CM_MINOR | CM_SEVEN, c);
    CHECK(k == 3 && c[0] == 67 && c[1] == 72 && c[2] == 72, "POWER takes only the inversion: %u %u %u", c[0], c[1], c[2]);
}

static void test_24_2_voice_leading_per_channel_for_8(void)
{
    uint8_t c[8], first[8];
    uint32_t n, i, nf;
    int32_t moved = 0, root_pos = 0;
    lk_key(0, 1);
    lk.t.p[P_CHORD] = 1;
    lk.t.p[P_VLEAD] = 1;
    memset(vl_n, 0, sizeof vl_n);
    nf = chord_of(7, 7, first);                 /* C on channel 8: remembered for it */
    CHECK(nf == 3 && first[0] == 60, "the first chord as it is");
    for (i = 0; i < 6u; i++) chord_of(16, 0, c); /* A minor over and over on channel 1: its own memory */
    n = chord_of(11, 7, c);                     /* E minor on channel 8: nearest C E G, not channel 1's last */
    for (i = 0; i < n; i++) moved += c[i] > first[i] ? c[i] - first[i] : first[i] - c[i];
    root_pos = (64 - 60) + (67 - 64) + (71 - 67);
    CHECK(n == 3 && pcs(c, n, 0) == (1u << 4 | 1u << 7 | 1u << 11), "E minor");
    CHECK(moved < root_pos && c[0] >= 24 && c[2] <= 108, "voice-led: %d semitones moved (root position %d)", moved, root_pos);
    CHECK(vl_n[7] == 3 && vl_n[0] == 3 && sizeof vl_n == 8u, "a memory for each of the 8 channels");
    CHECK(vl_prev[7][0] == c[0] && vl_prev[0][0] != c[0], "channel 8 remembers its own chord");
}

/* ------------------------------------------------------------------ 24.3 --- */
static void test_24_3_four_voices_kept_for_live_notes(void)
{
    uint32_t ch, k, pad;
    synth_init();
    pad = sound_find("ANALOG", "SOFT PAD");
    for (ch = 0; ch < NPART; ch++) synth_sound(ch, pad);
    for (ch = 0; ch < NPART; ch++) { synth_on(ch, 48, 100); run(1); synth_on(ch, 52, 100); run(1); }
    CHECK(synth_active() == 16 && synth_live() == 0, "the piece fills the 16: %u", synth_active());
    for (k = 0; k < 4u; k++) { synth_live_on(2, 72 + k, 100); run(1); }
    CHECK(synth_live() == 4 && synth_active() <= 16, "4 live notes take 4 of the piece's: %u live, %u", synth_live(), synth_active());
    for (ch = 0; ch < NPART; ch++) { synth_on(ch, 55, 100); run(1); synth_on(ch, 59, 100); run(1); synth_on(ch, 62, 100); run(1); }
    CHECK(synth_live() == 4, "the piece never steals a live voice: %u", synth_live());
    CHECK(synth_active() - synth_live() <= 12, "the piece keeps to 12: %u", synth_active() - synth_live());
    for (k = 0; k < NVOICE; k++)
        if (live_v(2, k)) CHECK(trk[2].v[k].note >= 72 && trk[2].v[k].note < 76 && trk[2].v[k].gate, "live voice %u kept: %u", k, trk[2].v[k].note);
    synth_off(2, 72); synth_ch_off(2); synth_all_off();
    CHECK(live_gated() == 4, "the piece's note-offs, mutes, stop of notes end no live note: %u", live_gated());
    synth_live_on(5, 90, 100); run(1);
    CHECK(synth_live() == 4, "a 5th live note: still 4 (%u)", synth_live());
    for (k = 0; k < NVOICE; k++) if (trk[2].v[k].note == 72 && trk[2].v[k].active) CHECK(trk[2].v[k].stage == 4u || !(vlive[2] >> k & 1u), "the oldest gave way");
    synth_live_on(5, 90, 100); run(1);
    CHECK(synth_live() == 4, "the same live note again: retriggered, not a 5th");
    for (k = 0; k < 4u; k++) synth_live_off(2, 72 + k);
    synth_live_off(5, 90);
    run(44100 / CTL * 8);                       /* (the releases end) */
    CHECK(synth_live() == 0 && live_gated() == 0, "released live notes end: %u", synth_live());
    for (ch = 0; ch < NPART; ch++) { synth_on(ch, 40, 100); run(1); synth_on(ch, 43, 100); run(1); }
    CHECK(synth_active() == 16, "no live note: the piece has all 16 again: %u", synth_active());
}

/* a channel full of the piece's notes and live ones: a new live note takes the piece's (a released one first, else its
 * oldest), never one of its live notes */
static void test_24_3_live_note_on_a_full_channel(void)
{
    uint32_t k, lives = 0, pad;
    synth_init();
    pad = sound_find("ANALOG", "SOFT PAD");
    synth_sound(6, pad);
    CHECK(chs[6].cap == NVOICE, "a pad has the 8 voices: %u", chs[6].cap);
    for (k = 0; k < 3u; k++) { synth_live_on(6, 80 + k, 100); run(1); }
    for (k = 0; k < 5u; k++) { synth_on(6, 40 + k, 100); run(1); }
    synth_off(6, 42);                           /* one of the piece's released (not the oldest) */
    run(1);
    synth_live_on(6, 90, 100); run(1);
    for (k = 0; k < NVOICE; k++) {
        lives += live_v(6, k);
        if (trk[6].v[k].active && trk[6].v[k].stage != 4u) CHECK(trk[6].v[k].note != 42, "the released one was taken");
    }
    CHECK(lives == 4, "4 live: %u", lives);
    for (k = 0; k < 3u; k++) {
        uint32_t j, found = 0;
        for (j = 0; j < NVOICE; j++) found |= live_v(6, j) && trk[6].v[j].note == 80 + k;
        CHECK(found, "live %u kept", 80 + k);
    }
    synth_live_off(6, 90);
    for (k = 0; k < 3u; k++) synth_live_off(6, 80 + k);
    run(44100 / CTL * 8);
    for (k = 0; k < 3u; k++) { synth_live_on(6, 70 + k, 100); run(1); }
    for (k = 0; k < 5u; k++) { synth_on(6, 50 + k, 100); run(1); }   /* (8 sounding, none released) */
    synth_live_on(6, 91, 100); run(1);
    for (k = 0; k < NVOICE; k++) if (trk[6].v[k].active && trk[6].v[k].stage != 4u) CHECK(trk[6].v[k].note != 50, "the piece's oldest taken");
    for (lives = 0, k = 0; k < NVOICE; k++) lives += live_v(6, k);
    CHECK(lives == 4, "4 live: %u", lives);
}

static void test_24_3_a_mono_channel_and_a_full_channel(void)
{
    uint32_t mono = sound_count, s, k, n = 0;
    synth_init();
    for (s = 0; s < sound_count; s++) if (sounds[s].pr->mono && !sounds[s].e->oneshot) { mono = s; break; }
    CHECK(mono < sound_count, "a mono sound");
    synth_sound(1, mono);
    synth_on(1, 40, 100); run(1);
    synth_live_on(1, 70, 100); run(1);
    CHECK(live_v(1, 0) && trk[1].v[0].note == 70, "the live note takes the mono voice");
    synth_on(1, 41, 100); run(1);
    CHECK(trk[1].v[0].note == 70 && live_v(1, 0), "the piece's next note does not take it back");
    synth_live_off(1, 70);
    synth_sound(4, sound_find("ANALOG", "SOFT PAD"));
    for (k = 0; k < 4u; k++) synth_live_on(4, 60 + k, 100);
    for (k = 0; k < 8u; k++) { synth_on(4, 30 + k, 100); run(1); }
    for (k = 0; k < NVOICE; k++) n += live_v(4, k);
    CHECK(n == 4 && synth_count(4) <= chs[4].cap, "a channel full of the piece's notes keeps its 4 live ones: %u", n);
    synth_live_on(NPART, 60, 100); synth_live_on(4, 128, 100); synth_live_on(4, 61, 128); synth_live_off(NPART, 60); synth_live_off(4, 128);
    CHECK(n == 4 && synth_live() == 4, "hostile live calls ignored");
    synth_live_on(4, 60, 0);
    CHECK(live_gated() == 3, "velocity 0: its note-off (%u held)", live_gated());
}

/* ------------------------------------------------------------------ 24.5 --- */
static void midi_feed(const uint8_t *b, uint32_t n, uint32_t sel, uint32_t nch)   /* bytes into the jack, then to live.c */
{
    uint32_t i;
    for (i = 0; i < n; i++) um_byte(b[i]);
    while (mi_r != mi_w) { uint32_t p = midi_in_q[mi_r++ % MQ]; lk_midi(p >> 8 & 0xFFu, p >> 16 & 0xFFu, p >> 24 & 0xFFu, sel, nch); }
}

static void test_24_5_midi_in_routing_edges_running_status(void)
{
    static const uint8_t a[] = {0x90, 0, 100, 127, 90, 0x9F, 64, 80, 0x93, 60, 70};   /* running status, note 0 / 127 */
    static const uint8_t b[] = {0x90, 0, 0, 127, 0, 0x8F, 64, 0, 0x93, 60, 0};     /* note-on velocity 0 = note-off */
    uint32_t i;
    synth_init();
    nev = 0; mi_w = mi_r = 0;
    memset(midi_sel_on, 0, sizeof midi_sel_on);
    {
        static const uint8_t stray[] = {0xF6, 60, 100, 61, 100};   /* data bytes with no status (after a system common): dropped */
        midi_feed(stray, sizeof stray, 5, 4);
        CHECK(nev == 0 && mi_w == 0, "no status, no message: %u", nev);
    }
    midi_feed(a, sizeof a, 5, 4);
    CHECK(nev == 4 && ev[0].ch == 0 && ev[0].note == 0 && ev[1].note == 127 && ev[1].vel == 90, "MIDI ch 1 -> channel 1, notes 0 and 127");
    CHECK(ev[2].ch == 5 && ev[2].note == 64, "MIDI ch 16 -> the selected channel: %u", ev[2].ch);
    CHECK(ev[3].ch == 3 && ev[3].note == 60 && ev[3].vel == 70, "MIDI ch 4 -> channel 4 (the piece has 4): %u", ev[3].ch);
    midi_feed(b, sizeof b, 1, 4);               /* (selected another channel meanwhile: the offs go where the ons went) */
    for (i = 4; i < nev; i++) CHECK(!ev[i].on, "note-offs");
    CHECK(nev == 8 && ev[6].ch == 5 && ev[7].ch == 3 && held_on() == 0, "each note-off ended its note-on: %u events", nev);
    nev = 0;
    lk_midi(0x96, 50, 100, 2, 4); lk_midi(0x9C, 51, 100, 2, 4);   /* MIDI ch 7 when the piece has 4: the selected one */
    CHECK(ev[0].ch == 2 && ev[1].ch == 2, "a channel the piece lacks: the selected one");
    lk_midi(0x94, 52, 127, 1, 4); lk_midi(0x84, 52, 0, 1, 4);    /* MIDI ch 5, the piece has 4 */
    CHECK(nev == 4 && ev[2].ch == 1 && ev[2].vel == 127 && ev[3].ch == 1, "MIDI ch 5 of a 4-channel piece: the selected one");
    nev = 2;
    lk_midi(0x96, 50, 100, 1, 4);                /* the same note again, no note-off between, another channel selected */
    CHECK(nev == 4 && !ev[2].on && ev[2].ch == 2 && ev[3].on && ev[3].ch == 1, "the first is ended before the second");
    lk_midi(0x86, 50, 0, 3, 4); lk_midi(0x8C, 51, 0, 3, 4);
    lk_midi(0xB0, 64, 127, 0, 4); lk_midi(0x90, 200, 100, 0, 4); lk_midi(0x80, 9, 0, 0, 4);   /* CC, a bad note, a stray off */
    CHECK(held_on() == 0 && nev == 6, "nothing held, nothing else sent: %u", nev);
}

static void test_24_5_lost_and_never_sent_note_offs(void)
{
    static const uint8_t on[] = {0x91, 70, 100, 72, 100};
    uint32_t i;
    synth_init();
    nev = 0; mi_w = mi_r = 0; midi_in_lost = 0;
    midi_feed(on, sizeof on, 0, 8);
    for (i = 0; i < 70u; i++) { um_byte(0xB0); um_byte(1); um_byte((uint8_t)i); }   /* the ring fills (nothing read) */
    CHECK(!midi_in_lost, "a lost CC loses no note");
    um_byte(0x81); um_byte(70); um_byte(0);      /* the note-off: no room */
    CHECK(midi_in_lost == 1 && mi_w - mi_r == 64u, "a lost note-off sets midi_in_lost (SLOOP 2.8.3)");
    mi_r = mi_w;
    run(2);
    CHECK(live_gated() == 2, "two notes held, one note-off lost");
    if (midi_in_lost) { midi_in_lost = 0; lk_midi_off(); }   /* (composer.c midi_in) */
    run(2);
    CHECK(live_gated() == 0 && held_on() == 0, "every MIDI note ended: none stuck (%u)", live_gated());
    for (i = 0; i < 16u; i++) lk_midi(0x90 | (i & 15u), 40 + i, 100, 0, 8);   /* note-offs never sent */
    run(2);
    CHECK(synth_live() == 4, "16 held, still 4 voices: %u", synth_live());
    lk_midi_off();                               /* (leaving the piece) */
    run(2);
    CHECK(live_gated() == 0 && held_on() == 0, "leaving the piece ends them");
}

static void test_24_5_all_27_keys_at_once(void)
{
    uint32_t chord, k;
    synth_init();
    for (chord = 0; chord < 6u; chord += 5u) {
        nev = 0;
        memset(&lk, 0, sizeof lk);
        lk_key(0, 1);
        lk.t.p[P_CHORD] = (int16_t)chord;
        lk_keys((1u << 27) - 1u, (1u << 27) - 1u, 0);
        run(2);
        CHECK(synth_live() <= 4 && synth_active() <= 16, "every key at once (chords %u): %u live", chord, synth_live());
        lk_keys(0xFFFFFFFFu, 0xFFFFFFFFu, 0);    /* (bits past the 27 keys: nothing) */
        lk_keys(0, 0, 0);
        run(2);
        CHECK(live_gated() == 0 && held_on() == 0 && lk.down == 0 && lk.modkeys == 0, "all let go: nothing held (%u)", live_gated());
    }
    lk_key(0, LK_CHROMATIC); lk.t.p[P_CHORD] = 0;
    lk_keys((1u << 27) - 1u, 0, 0);             /* CHROMATIC: 27 notes, the last 4 pressed (the top) sound */
    run(2);
    for (k = 0; k < NVOICE; k++) if (live_v(0, k)) CHECK(trk[0].v[k].note >= 76 && trk[0].v[k].note <= 79, "the top four: %u", trk[0].v[k].note);
    CHECK(synth_live() == 4, "4 live");
    lk_keys(0, 0, 0);
    nev = 0;
    lk.oct = 3;
    lk_keys(1u << 26, 0, 0);                     /* G5 three octaves up: 115 */
    lk_keys(1u << 26, 1u << 3, 1);     /* a short press within a pass (an edge, not held): on, then off */
    CHECK(nev == 3 && ev[0].note == 115 && ev[1].note == 3 + 53 + 36 && ev[2].on == 0, "short press: %u events", nev);
    lk_keys_off();
    CHECK(held_on() == 0, "KEYS off ends the keys");
    lk_key(0, 1); lk.oct = 3; lk.t.p[P_CHORD] = 5;
    for (k = 0; k < LK_KEYS; k++) { lk_keys(1u << k, 1u << k, 0); lk_keys(0, 0, 0); }
    for (k = 0; k < nev; k++) CHECK(ev[k].note <= 127, "notes stay in MIDI's range");
    CHECK(held_on() == 0, "nothing held");
}

int main(void)
{
    test_24_1_carried_key_white_keys_walk_the_scale();
    test_24_1_estimated_from_the_notes();
    test_24_1_sloops_16_scales_and_chromatic();
    test_24_1_kb_pitch_snap_transpose_and_clamp();
    test_24_1_label();
    test_24_2_auto_chords_of_the_degree();
    test_24_2_voice_leading_per_channel_for_8();
    test_24_2_chords_as_sloops_rule_random();
    test_24_2_power_chords();
    test_24_3_four_voices_kept_for_live_notes();
    test_24_3_a_mono_channel_and_a_full_channel();
    test_24_3_live_note_on_a_full_channel();
    test_24_5_midi_in_routing_edges_running_status();
    test_24_5_lost_and_never_sent_note_offs();
    test_24_5_all_27_keys_at_once();
    printf("%s: %d failure(s)\n", fails ? "FAIL" : "ok", fails);
    return fails != 0;
}
