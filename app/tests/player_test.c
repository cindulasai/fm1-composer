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
static uint32_t steps;                          /* cursor steps taken (pl_jump, pl_seek) */
#define PL_STEP() (steps++)
#include "player.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

/* ---- a piece builder (the format of piece.h) ---- */
typedef struct { uint32_t tick, dur; uint8_t pitch, vel; } tn_t;
static uint8_t pb[PIECE_MAX_BYTES];
static void put16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void put32(uint8_t *p, uint32_t v) { put16(p, v & 0xFFFFu); put16(p + 2, v >> 16); }
static uint32_t bs_n, bs_t[32];                 /* sections for the next build (v2 when bs_n): starts; names S1.. */
static uint32_t build(uint32_t tempo, uint32_t len, uint32_t nch, const tn_t *const *notes, const uint32_t *counts)
{
    uint32_t off = PIECE_HDR + nch * PIECE_CHREC, ch, i, n;
    memset(pb, 0, sizeof pb);
    if (bs_n) {
        pb[off] = (uint8_t)bs_n;
        for (i = 0; i < bs_n; i++) {
            put32(pb + off + 4 + i * 12, bs_t[i]);
            {   /* the name field is 8 bytes, zero-padded: format apart, copy only the characters */
                char nm[12];
                int k = snprintf(nm, sizeof nm, "S%u", (unsigned)(i + 1));
                memcpy(pb + off + 8 + i * 12, nm, (size_t)k);
            }
        }
        off += 4 + bs_n * 12;
    }
    memcpy(pb, "F1P1", 4); pb[4] = bs_n ? 2 : 1; pb[5] = (uint8_t)nch; put16(pb + 8, tempo); put16(pb + 10, 96); put32(pb + 12, len);
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

static void reset(void)
{
    synth_init(); nrec = nchoff = nhush = 0; now = 0; pl_want_run = 0; pl_state = 0; pl_running = 0; pl_loop = 1;
    bs_n = 0; pl_sloop = 0; steps = 0;
}

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

static void test_11_5_play_from_a_tick(void)
{
    static const tn_t ns[4] = {{0, 20, 60, 100}, {96, 20, 62, 100}, {192, 20, 64, 100}, {384, 20, 65, 100}};
    const tn_t *nl[1] = {ns};
    const uint32_t cnt[1] = {4};
    uint32_t i, first = 0, wrapped = 0;
    reset();
    build(1200, 768, 1, nl, cnt);
    player_load(pb);
    player_play_from(150);
    play_frames(44100);                         /* 1 s: 192 ticks at 120 BPM, past the end at 768 - 150 */
    for (i = 0; i < nrec; i++) if (rec[i].on) { if (!first) first = rec[i].pitch; else if (rec[i].pitch == 60) wrapped = 1; }
    CHECK(first == 64, "first note %u, want 64 (tick 192)", first);
    CHECK(rec[0].on && rec[0].frame >= 42u * 44100u * 60u / 120u / 96u - CTL && rec[0].frame <= 42u * 44100u * 60u / 120u / 96u + CTL,
          "tick 192 at frame %u", rec[0].frame);
    play_frames(44100 * 3);
    for (i = 0; i < nrec; i++) if (rec[i].on && rec[i].pitch == 60) wrapped = 1;
    CHECK(wrapped, "the loop comes back to bar 1");
    player_stop(); play_frames(CTL);
    player_play_from(192);                      /* exactly on a note: it sounds at once */
    nrec = 0; play_frames(CTL);
    CHECK(nrec && rec[0].on && rec[0].pitch == 64, "a note on the start tick plays, got %u", nrec ? rec[0].pitch : 0);
    player_stop(); play_frames(CTL);
    player_play_from(99999);                    /* past the end: from the start */
    nrec = 0; play_frames(CTL);
    CHECK(nrec && rec[0].pitch == 60, "past the end plays from the start");
    player_stop(); play_frames(CTL);
    pb[PIECE_HDR + PIECE_CHREC + 4] = 70;       /* first note's pitch: an edit, as edit.c would make it */
    player_refresh();
    player_play();
    nrec = 0; play_frames(CTL);
    CHECK(nrec && rec[0].pitch == 70, "player_refresh reads the edited bytes, got %u", nrec ? rec[0].pitch : 0);
}

/* ---- US-23: sections ---- */
static uint32_t frame_of(uint32_t ticks, uint32_t tempo) { return (uint32_t)((uint64_t)ticks * PL_TICK_UNITS / (tempo * 96u)); }
static int near_frame(uint32_t frame, uint32_t exact) { int64_t d = (int64_t)frame - (int64_t)exact; return d <= 0 && d > -(int64_t)CTL; }

/* 8 bars, a note each beat, pitch 40 + its bar; sections at the given bars */
static tn_t sb[64];
static uint32_t sect_piece(uint32_t n, const uint32_t *bars, uint32_t dur)
{
    const tn_t *nl[1] = {sb};
    uint32_t cnt[1] = {32}, i;
    for (i = 0; i < 32; i++) sb[i] = (tn_t){i * 96, dur, (uint8_t)(40 + i / 4), 100};
    bs_n = n;
    for (i = 0; i < n; i++) bs_t[i] = bars[i] * 384;
    return build(1200, 8 * 384, 1, nl, cnt);
}

static uint32_t ons(void) { uint32_t i, k = 0; for (i = 0; i < nrec; i++) k += rec[i].on; return k; }
static uint32_t offs(void) { uint32_t i, k = 0; for (i = 0; i < nrec; i++) k += !rec[i].on; return k; }
static int32_t first_on(uint32_t pitch) { uint32_t i; for (i = 0; i < nrec; i++) if (rec[i].on && rec[i].pitch == pitch) return (int32_t)i; return -1; }

static void test_23_1_v1_and_v2_play_the_same_without_launches(void)
{
    static rec_t a[400];
    uint32_t na, i, n;
    const uint32_t bars[3] = {0, 2, 5};
    reset();
    n = sect_piece(0, bars, 90);
    CHECK(pb[4] == 1 && piece_check(pb, n) == PE_OK, "v1");
    player_load(pb); player_play(); play_frames(44100 * 9);
    CHECK(player_section() == PL_NONE && player_queued() == PL_NONE, "v1: no section");
    na = nrec; memcpy(a, rec, sizeof a);
    player_stop(); play_frames(CTL);
    reset();
    n = sect_piece(3, bars, 90);
    CHECK(pb[4] == 2 && piece_check(pb, n) == PE_OK, "v2");
    player_load(pb); player_play(); play_frames(44100 * 9);
    CHECK(nrec == na && na > 30 && !memcmp(a, rec, na * sizeof *rec), "same notes at the same frames: %u vs %u", nrec, na);
    for (i = 0; i < 3; i++) CHECK(piece_sect_start(&pl_v, i) == bars[i] * 384, "start %u", i);
    CHECK(piece_sect_end(&pl_v, 2) == 8 * 384 && piece_sect_end(&pl_v, 0) == 2 * 384, "ends");
    player_stop(); play_frames(CTL);
}

static void test_23_3_a_queued_section_starts_at_the_next_bar_line(void)
{
    const uint32_t bars[2] = {0, 4};
    int32_t k;
    reset();
    sect_piece(2, bars, 120);                   /* (bar 2's last note sounds over the bar line) */
    player_load(pb); player_play();
    play_frames(frame_of(500, 1200));           /* in bar 2 */
    CHECK(player_section() == 0, "section 1 playing: %u", player_section());
    player_launch(1);
    CHECK(player_queued() == 1, "queued");
    play_frames(44100 * 2);
    k = first_on(44);
    CHECK(k >= 0 && near_frame(rec[k].frame, frame_of(768, 1200)), "bar 5's note at bar 3's line: frame %d want %u",
          k >= 0 ? (int)rec[k].frame : -1, frame_of(768, 1200));
    CHECK(first_on(42) < 0, "bar 3 never played");
    CHECK(k >= 1 && rec[k - 1].on == 0 && rec[k - 1].pitch == 41 && rec[k - 1].frame == rec[k].frame, "bar 2's note let go at the jump");
    CHECK(player_section() == 1 && player_queued() == PL_NONE && player_pos() > 4 * 384, "now in section 2 at %u", player_pos());
    player_stop(); play_frames(CTL);
    CHECK(ons() == offs(), "every note let go: %u on, %u off", ons(), offs());
}

static void test_23_3_loop_repeats_the_section_without_gap_or_stuck_note(void)
{
    const uint32_t bars[3] = {0, 4, 6};
    uint32_t i, k = 0, held = 0;
    reset();
    sect_piece(3, bars, 90);
    sb[31].dur = 300;                           /* the section's last note crosses its end */
    { const tn_t *nl[1] = {sb}; uint32_t cnt[1] = {32}; build(1200, 8 * 384, 1, nl, cnt); }
    player_load(pb);
    player_section_loop(1);
    player_launch(2);                           /* stopped: PLAY plays it from its start */
    player_play();
    play_frames(frame_of(3 * 768, 1200) + 1000);   /* three passes of 2 bars */
    for (i = 0; i < nrec; i++) {
        uint32_t beat = k % 8, pass = k / 8;
        if (!rec[i].on) continue;
        CHECK(rec[i].pitch == 46 + beat / 4 && near_frame(rec[i].frame, frame_of(pass * 768 + beat * 96, 1200)),
              "note %u: pitch %u frame %u", k, rec[i].pitch, rec[i].frame);
        k++;
    }
    CHECK(k == 25, "3 passes x 8 notes, then the 4th starts: %u", k);
    for (i = 0; i < nrec; i++)                  /* the crossing note let go at each jump back */
        if (!rec[i].on && rec[i].pitch == 47 && rec[i].frame > frame_of(767, 1200) && rec[i].frame <= frame_of(768, 1200)) held++;
    CHECK(held == 1, "the long note ends at the first loop point: %u", held);
    CHECK(player_section() == 2, "still section 3");
    player_section_loop(0);                     /* loop off: it plays on into the next pass's end, then wraps to the start */
    play_frames(frame_of(768 + 384, 1200));
    CHECK(player_section() == 0 || player_section() == PL_NONE || player_pos() < 384, "past the end, from the start: %u", player_pos());
    player_stop(); play_frames(CTL);
    CHECK(ons() == offs() && synth_active() == 0, "nothing stuck: %u on, %u off", ons(), offs());
}

static void test_23_3_a_jump_back_is_o1_cursor_snapshots(void)
{
    static tn_t d[8][500];
    const tn_t *nl[8];
    uint32_t cnt[8], ch, i, n, worst = 0, jumps = 0, s;
    for (ch = 0; ch < 8; ch++) {
        for (i = 0; i < 500; i++) d[ch][i] = (tn_t){i * 24, 12, (uint8_t)(30 + ch * 8 + i % 8), 90};
        nl[ch] = d[ch]; cnt[ch] = 500;
    }
    reset();
    bs_n = 4; bs_t[0] = 0; bs_t[1] = 384 * 8; bs_t[2] = 384 * 16; bs_t[3] = 384 * 30;
    n = build(3000, 500 * 24, 8, nl, cnt);
    CHECK(n <= PIECE_MAX_BYTES && piece_check(pb, n) == PE_OK, "dense v2 piece");
    player_load(pb);
    for (s = 0; s < 4; s++)                     /* each snapshot: the first note at or after the start */
        for (ch = 0; ch < 8; ch++) {
            piece_cur_t c = pl_snap[s][ch];
            piece_note_t no;
            CHECK(piece_next(&c, &no) && no.tick == bs_t[s] && no.pitch == 30 + ch * 8 + (bs_t[s] / 24) % 8, "snap %u ch %u", s, ch);
        }
    player_section_loop(1);
    player_launch(3);
    player_play();
    for (i = 0; i < 44100 * 4 / CTL; i++) {     /* 4 s at 300 BPM: section 4 (480 ticks) loops 4 times */
        uint32_t before = steps, p0 = pl_tick;
        play_frames(CTL);
        if (steps - before > worst) worst = steps - before;
        if (pl_tick < p0) jumps++;
    }
    CHECK(jumps >= 1 && worst == 8, "a jump back: one cursor copy per channel (%u jumps, worst %u steps)", jumps, worst);
    CHECK(ons() > 500, "it played: %u", ons());
    player_stop(); play_frames(CTL);
    steps = 0;
    player_play_from(384 * 29);                 /* (the counter sees a scan: play from a tick walks the events) */
    play_frames(CTL);
    CHECK(steps > 1000, "a scan counts: %u", steps);
    player_stop(); play_frames(CTL);
}

static void test_23_5_hostile_sections(void)
{
    const uint32_t bars[2] = {0, 4};
    uint32_t n, i, before;
    static const uint32_t bad[][4] = {           /* {count, start1, start2, what}: PE_SECT each */
        {2, 0, 8 * 384, 0}, {2, 384, 384, 0}, {2, 768, 384, 0}, {25, 0, 0, 0}, {0, 0, 0, 0}};
    reset();                                    /* 0 sections (v1): launching does nothing */
    sect_piece(0, bars, 90);
    player_load(pb); player_launch(0); player_play(); play_frames(44100);
    CHECK(player_queued() == PL_NONE && player_section() == PL_NONE && ons() >= 2, "v1 plays on");
    player_stop(); play_frames(CTL);
    for (i = 0; i < 5; i++) {                   /* past the end, a 0-length (same start), unsorted, 25, 0 */
        uint32_t k;
        reset();
        sect_piece(0, bars, 90);
        bs_n = bad[i][0] ? bad[i][0] : 1;
        for (k = 0; k < bs_n; k++) bs_t[k] = k * 10;
        if (bad[i][0] == 2) { bs_t[0] = bad[i][1]; bs_t[1] = bad[i][2]; }
        { const tn_t *nl[1] = {sb}; uint32_t cnt[1] = {32}; n = build(1200, 8 * 384, 1, nl, cnt); }
        if (!bad[i][0]) { pb[PIECE_HDR + PIECE_CHREC] = 0; put32(pb + n - 4, piece_crc32(pb, n - 4)); }
        CHECK(piece_check(pb, n) == PE_SECT, "bad table %u: %d", i, piece_check(pb, n));
    }
    {                                           /* reserved bytes, an empty name, a control character, short */
        static const uint32_t at[4] = {1, 8, 9, 0};
        for (i = 0; i < 4; i++) {
            reset();
            n = sect_piece(2, bars, 90);
            if (i == 3) { put32(pb + 40, PIECE_HDR + PIECE_CHREC + 8); n = PIECE_HDR + PIECE_CHREC + 8; }
            else pb[PIECE_HDR + PIECE_CHREC + at[i]] = i == 1 ? 0 : 7;
            put32(pb + n - 4, piece_crc32(pb, n - 4));
            CHECK(piece_check(pb, n) == PE_SECT, "bad byte %u: %d", i, piece_check(pb, n));
        }
    }
    reset();                                    /* 24 sections, one a bar... the piece is 8 bars: 24 at 128 ticks */
    bs_n = 24;
    for (i = 0; i < 24; i++) bs_t[i] = i * 128;
    { const tn_t *nl[1] = {sb}; uint32_t cnt[1] = {32}; for (i = 0; i < 32; i++) sb[i] = (tn_t){i * 96, 90, (uint8_t)(40 + i / 4), 100};
      n = build(1200, 8 * 384, 1, nl, cnt); }
    CHECK(piece_check(pb, n) == PE_OK, "24 sections");
    player_load(pb);
    player_launch(24); player_launch(200);      /* out of range: ignored */
    CHECK(player_queued() == PL_NONE, "no such section");
    player_launch(23); player_play(); play_frames(CTL);   /* launch while stopped: from its start at once */
    play_frames(frame_of(40, 1200));
    CHECK(player_section() == 23 && first_on(47) == 0, "from section 24's start (%u)", player_section());
    for (i = 0; i < 20; i++) player_launch(i);  /* 20 launches before its end: the last wins, one jump */
    before = pl_tick;
    for (n = 0, i = 0; i < frame_of(200, 1200) / CTL; i++) { uint32_t t0 = pl_tick; play_frames(CTL); n += pl_tick < t0; }
    CHECK(n == 1 && player_queued() == PL_NONE && before >= 2944, "one jump: %u", n);
    CHECK(first_on(46) >= 0, "section 20 played (bar 7)");
    player_stop(); play_frames(CTL);
    CHECK(ons() == offs() && synth_active() == 0, "nothing stuck: %u on, %u off", ons(), offs());
    player_launch(5); player_stop(); play_frames(CTL);  /* launched, then stopped: stays queued for PLAY */
    player_play(); play_frames(CTL);
    CHECK(player_section() == 5, "PLAY after a launch while stopped: %u", player_section());
    player_launch(9); player_stop(); play_frames(CTL);   /* a launch pending at the stop is forgotten */
    player_play(); play_frames(CTL);
    CHECK(player_section() == 0 && player_queued() == PL_NONE, "stop forgets a launch: %u", player_section());
    player_stop(); play_frames(CTL);
}

int main(void)
{
    test_23_1_v1_and_v2_play_the_same_without_launches();
    test_23_3_a_queued_section_starts_at_the_next_bar_line();
    test_23_3_loop_repeats_the_section_without_gap_or_stuck_note();
    test_23_3_a_jump_back_is_o1_cursor_snapshots();
    test_23_5_hostile_sections();
    test_11_5_play_from_a_tick();
    test_5_1_timing_without_drift();
    test_5_2_tempo_change_keeps_position_and_loop_wraps();
    test_5_2_loop_wraps_on_time_8_channels();
    test_5_3_mute_and_solo();
    test_5_4_hostile_pieces_and_stop();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
