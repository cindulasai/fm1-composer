/* SPDX-License-Identifier: GPL-3.0-only */
/* US-5: player.c through its interface, with the real synth behind it: timing without drift, tempo changes,
 * looping, mute / solo, hostile pieces, stop. The note calls are recorded on their way to the synth. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "synth.c"

static uint32_t now;                            /* the frame the current block starts at */
typedef struct { uint32_t frame; uint8_t on, ch, pitch; } rec_t;
static rec_t rec[20000];
static uint32_t nrec, nchoff, nhush;
static void t_on(uint32_t ch, uint32_t p, uint32_t v) { if (nrec < 20000u) rec[nrec++] = (rec_t){now, 1, (uint8_t)ch, (uint8_t)p}; synth_on(ch, p, v); }
static void t_off(uint32_t ch, uint32_t p) { if (nrec < 20000u) rec[nrec++] = (rec_t){now, 0, (uint8_t)ch, (uint8_t)p}; synth_off(ch, p); }
static void t_choff(uint32_t ch) { nchoff++; synth_ch_off(ch); }
static void t_hush(void) { nhush++; synth_hush(); }
#define PL_NOTE_ON t_on
#define PL_NOTE_OFF t_off
#define PL_CH_OFF t_choff
#define PL_HUSH t_hush
#include "player.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

/* ---- a piece builder (the format of piece.h) ---- */
typedef struct { uint32_t tick, dur; uint8_t pitch, vel; } tn_t;
static uint8_t pb[PIECE_MAX_BYTES];
static void put16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void put32(uint8_t *p, uint32_t v) { put16(p, v & 0xFFFFu); put16(p + 2, v >> 16); }
static uint32_t build(uint32_t tempo, uint32_t len, uint32_t nch, const tn_t *const *notes, const uint32_t *counts)
{
    uint32_t off = PIECE_HDR + nch * PIECE_CHREC, ch, i, n;
    memset(pb, 0, sizeof pb);
    memcpy(pb, "F1P1", 4); pb[4] = 1; pb[5] = (uint8_t)nch; put16(pb + 8, tempo); put16(pb + 10, 96); put32(pb + 12, len);
    memcpy(pb + 16, "test", 4);
    for (ch = 0; ch < nch; ch++) {
        uint8_t *c = pb + PIECE_HDR + ch * PIECE_CHREC;
        uint32_t t = 0;
        c[0] = ROLE_PAD; c[1] = 255; c[2] = 100; c[3] = 64;
        put32(c + 8, counts[ch]); put32(c + 12, off);
        for (i = 0; i < counts[ch]; i++) {
            const tn_t *x = &notes[ch][i];
            put16(pb + off, x->tick - t); put16(pb + off + 2, x->dur); pb[off + 4] = x->pitch; pb[off + 5] = x->vel;
            t = x->tick; off += 6;
        }
    }
    n = off + 4;
    put32(pb + 40, n);
    put32(pb + off, piece_crc32(pb, off));
    return n;
}

static int32_t blk_peak;
static void play_frames(uint32_t frames)
{
    int32_t out[2 * CTL];
    uint32_t f, i;
    for (f = 0; f < frames; f += CTL) {
        player_tick(CTL);
        memset(out, 0, sizeof out);
        synth_apply();
        synth_render(out, CTL);
        blk_peak = 0;
        for (i = 0; i < 2 * CTL; i++) { int32_t a = out[i] < 0 ? -out[i] : out[i]; if (a > blk_peak) blk_peak = a; }
        now += CTL;
    }
}

static void reset(void) { synth_init(); nrec = nchoff = nhush = 0; now = 0; pl_want_run = 0; pl_state = 0; pl_running = 0; pl_loop = 1; }

static void timing(uint32_t tempo)
{
    static tn_t ns[70];
    const tn_t *nl[1] = {ns};
    uint32_t cnt[1], i, k = 0, n;
    for (i = 0; i < 64; i++) ns[k++] = (tn_t){i * 384, 48, 60, 100};
    ns[k++] = (tn_t){63 * 384 + 2 * 96 + 17, 10, 72, 100};           /* an odd tick near the end */
    cnt[0] = k;
    reset();
    n = build(tempo, 64 * 384, 1, nl, cnt);
    CHECK(piece_check(pb, n) == PE_OK, "test piece ok");
    CHECK(player_load(pb) == 0, "load");
    pl_loop = 0;
    player_play();
    play_frames((uint32_t)((uint64_t)64 * 384 * PL_TICK_UNITS / (tempo * 96u)) + 44100u);
    for (i = 0, k = 0; i < nrec; i++) {
        uint64_t exact;
        int64_t d;
        if (!rec[i].on) continue;
        exact = (uint64_t)ns[k].tick * PL_TICK_UNITS / (tempo * 96u);   /* the frame the note belongs at */
        d = (int64_t)rec[i].frame - (int64_t)exact;
        CHECK(d <= 0 && d > -(int64_t)CTL, "tempo %u note %u at frame %u, exact %llu", tempo, k, rec[i].frame, (unsigned long long)exact);
        k++;
    }
    CHECK(k == cnt[0], "all %u notes played: %u", cnt[0], k);
    for (i = 0, k = 0; i < nrec; i++) {                 /* and each note-off at its tick + duration */
        int64_t d;
        if (rec[i].on) continue;
        d = (int64_t)rec[i].frame - (int64_t)((uint64_t)(ns[k].tick + ns[k].dur) * PL_TICK_UNITS / (tempo * 96u));
        CHECK(d <= 0 && d > -(int64_t)CTL, "tempo %u note-off %u off by %lld frames", tempo, k, (long long)d);
        k++;
    }
}

static void test_5_1_timing_without_drift(void) { timing(1200); timing(973); timing(3000); timing(300); }

static void test_5_2_tempo_change_keeps_position_and_loop_wraps(void)
{
    static tn_t ns[] = {{0, 500, 60, 100}, {384, 400, 62, 100}, {700, 300, 64, 100}};   /* the last two cross the end */
    const tn_t *nl[1] = {ns};
    uint32_t cnt[1] = {3}, p0, p1, i, ons = 0, offs = 0, first_on_again = 0;
    reset();
    build(1200, 768, 1, nl, cnt);
    player_load(pb);
    player_play();
    play_frames(44100 / 4);                     /* 0.25 s at 120: 48 ticks */
    p0 = player_pos();
    CHECK(p0 >= 47 && p0 <= 48, "pos at 0.25 s: %u", p0);
    player_tempo(900);
    play_frames(44100);                         /* 1 s at 90: 144 ticks more */
    p1 = player_pos();
    CHECK(p1 - p0 >= 143 && p1 - p0 <= 145, "after tempo change: +%u ticks", p1 - p0);
    play_frames(44100 * 5);                     /* past the end (768 ticks at 90 = 5.33 s from 0): it loops */
    for (i = 0; i < nrec; i++) {
        if (rec[i].on) { ons++; if (ons == 4) first_on_again = rec[i].pitch; }
        else offs++;
    }
    CHECK(ons >= 4 && first_on_again == 60, "loop starts again with the first note (%u ons)", ons);
    CHECK(offs >= 3, "notes crossing the end were released at the wrap: %u offs", offs);
    player_tempo(10); CHECK(pl_tempo == 300, "tempo floor"); player_tempo(99999); CHECK(pl_tempo == 3000, "tempo ceiling");
}

static void test_5_2_loop_wraps_on_time_8_channels(void)
{
    static tn_t ns[8][1];
    const tn_t *nl[8];
    uint32_t cnt[8], ch, i, ons = 0;
    for (ch = 0; ch < 8; ch++) { ns[ch][0] = (tn_t){ch * 10, 5, (uint8_t)(40 + ch), 100}; nl[ch] = ns[ch]; cnt[ch] = 1; }
    reset();
    build(1200, 384, 8, nl, cnt);
    player_load(pb);
    player_play();
    play_frames(44100 * 6 - 1000);              /* 384 ticks at 120 = 2 s: three passes */
    for (i = 0; i < nrec; i++) {
        uint32_t pass, c;
        int64_t d;
        if (!rec[i].on) continue;
        pass = ons / 8; c = ons % 8;
        d = (int64_t)rec[i].frame - (int64_t)((uint64_t)(pass * 384 + c * 10) * PL_TICK_UNITS / (1200u * 96u));
        CHECK(rec[i].ch == c && d <= 0 && d > -(int64_t)CTL, "pass %u channel %u off by %lld", pass, c, (long long)d);
        ons++;
    }
    CHECK(ons == 24, "8 channels x 3 passes: %u", ons);
}

static void test_5_3_mute_and_solo(void)
{
    static tn_t a[] = {{0, 2000, 60, 100}, {500, 10, 61, 100}}, b[] = {{0, 2000, 40, 100}, {500, 10, 41, 100}},
                c[] = {{0, 2000, 70, 100}, {500, 10, 71, 100}};
    const tn_t *nl[3] = {a, b, c};
    uint32_t cnt[3] = {2, 2, 2}, i, seen[3] = {0, 0, 0};
    reset();
    build(1200, 3000, 3, nl, cnt);
    player_load(pb);
    player_play();
    play_frames(4410);                          /* all three hold their first note */
    CHECK(synth_count(1) == 1, "channel 2 sounds");
    player_mute(1, 1);
    play_frames(CTL);
    CHECK(nchoff == 1 && trk[1].v[0].stage == 3u, "mute releases what sounds (%u)", nchoff);
    CHECK(pl_noff == 2, "the other channels' notes still due: %u", pl_noff);
    player_solo(0, 1); player_solo(2, 1);
    play_frames(CTL);
    CHECK(nchoff == 6, "solo releases exactly channels 4-8 (%u)", nchoff);
    player_mute(8, 1); player_solo(8, 1);       /* out of range: nothing */
    CHECK(pl_solo == 5u, "channel 8 ignored");
    nrec = 0;
    play_frames(44100 * 3);                     /* tick 500 passes (2.6 s at 120) */
    for (i = 0; i < nrec; i++) if (rec[i].on) seen[rec[i].ch]++;
    CHECK(seen[0] == 1 && seen[1] == 0 && seen[2] == 1, "solo 1+3 only: %u %u %u", seen[0], seen[1], seen[2]);
    play_frames(44100 * 8);                     /* the held notes end at tick 2000 (10.4 s) */
    for (i = 0, seen[0] = seen[2] = 0; i < nrec; i++) if (!rec[i].on && (rec[i].pitch == 60 || rec[i].pitch == 70)) seen[rec[i].ch]++;
    CHECK(seen[0] == 1 && seen[2] == 1, "held notes of the playing channels end on time");
    player_solo(0, 0); player_solo(2, 0);
    CHECK(pl_mute == 2u && pl_solo == 0, "unsolo keeps the mute");
    player_solo(2, 1);
    play_frames(CTL);
    CHECK(nchoff >= 2, "solo of another channel releases channel 1");
}

static void test_5_4_hostile_pieces_and_stop(void)
{
    static tn_t big[4096], one[] = {{0, 5, 60, 100}};
    const tn_t *nl[2] = {big, one};
    uint32_t cnt[2] = {0, 0}, i;
    const tn_t *el[1] = {big};
    reset();                                    /* empty channels */
    build(1200, 384, 1, el, cnt);
    CHECK(player_load(pb) == 0, "empty loads");
    player_play(); play_frames(44100);
    CHECK(nrec == 0 && player_playing(), "empty plays silence");
    player_stop(); play_frames(CTL);
    reset();                                    /* a 1-tick piece, looping */
    cnt[0] = 1;
    build(1200, 1, 1, (const tn_t *const[]){one}, cnt);
    player_load(pb); player_play(); play_frames(44100);
    CHECK(nrec > 100, "1-tick piece loops: %u events", nrec);
    player_stop(); play_frames(CTL);
    reset();                                    /* 4096 notes on one tick */
    for (i = 0; i < 4096; i++) big[i] = (tn_t){0, 50, (uint8_t)(i % 128), 100};
    cnt[0] = 4096; cnt[1] = 1;
    {
        uint32_t n = build(1200, 384, 1, nl, cnt);
        CHECK(n <= PIECE_MAX_BYTES && piece_check(pb, n) == PE_OK, "4096 notes fit");
    }
    player_load(pb); player_play();
    CHECK(player_load(pb) == -1, "no load once play is asked, before the interrupt starts it");
    play_frames(CTL);
    CHECK(pl_noff == PL_MAXOFF && pl_dropped == 4096 - PL_MAXOFF, "capped at %u held, %u dropped", pl_noff, pl_dropped);
    nrec = 0;
    player_mute(1, 1); play_frames(CTL);        /* (a channel with nothing held) */
    CHECK(pl_noff == PL_MAXOFF, "muting another channel keeps these");
    player_stop(); play_frames(CTL);
    {
        uint32_t offs = 0;
        for (i = 0; i < nrec; i++) offs += !rec[i].on;
        CHECK(offs == PL_MAXOFF && pl_noff == 0, "stop releases each held note once: %u", offs);
    }
    player_play(); play_frames(CTL);
    player_mute(0, 1); play_frames(CTL);
    CHECK(pl_noff == 0, "muting the full channel drops its notes");
    play_frames(44100);
    CHECK(synth_active() <= VOICE_BUDGET, "budget kept");
    reset();                                    /* stop during a long note: silent within 50 ms */
    {
        static tn_t pad[] = {{0, 3000, 60, 120}};
        const tn_t *pl[1] = {pad};
        uint32_t c1[1] = {1};
        build(1200, 3000, 1, pl, c1);
        synth_sound(0, sound_find("ANALOG", "SOFT PAD"));
        player_load(pb); player_play(); play_frames(44100 * 3);
        CHECK(blk_peak > 500, "the pad sounds (%d)", blk_peak);
        CHECK(player_load(pb) == -1, "no load while playing");
        player_stop(); play_frames(44100 / 20);
        CHECK(nhush == 1 && blk_peak < 30 && !player_playing() && player_pos() == 0, "stopped: peak %d", blk_peak);
        player_play(); play_frames(CTL);
        CHECK(player_playing() && nrec >= 2, "plays again from the start");
    }
    reset();
    pl_loaded = 0;
    player_play(); player_tick(CTL);
    CHECK(!player_playing(), "nothing loaded: nothing plays");
}

int main(void)
{
    test_5_1_timing_without_drift();
    test_5_2_tempo_change_keeps_position_and_loop_wraps();
    test_5_2_loop_wraps_on_time_8_channels();
    test_5_3_mute_and_solo();
    test_5_4_hostile_pieces_and_stop();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
