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
static uint32_t nswitch, switch_frame, switch_steps;  /* live send: switches seen (PL_SWITCH), the last one's frame */
#define PL_SWITCH() (nswitch++, switch_frame = now, switch_steps = steps)
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
    bs_n = 0; pl_sloop = 0; steps = 0; nswitch = 0; pl_nx = 0;
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
        static const uint32_t at[4] = {3, 8, 9, 0};   /* (US-24: +1 and +2 are the key; +3 is still reserved) */
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

/* ---- US-30: voice modes ---- */
static uint32_t crc_upd(uint32_t c, const void *p, size_t n)
{
    const uint8_t *b = p;
    uint32_t k;
    c = ~c;
    while (n--) { c ^= *b++; for (k = 0; k < 8u; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u))); }
    return ~c;
}

static uint32_t first_of(const engine_t *e)
{
    uint32_t i;
    for (i = 0; i < sound_count && sounds[i].e != e; i++) {}
    return i;
}

/* 8 channels of overlapping notes and chords on mono and poly sounds (SAW LEAD, SQR BASS, FM BASS and ROBOT are mono
 * presets, the last two with SLOOP glide in their extras): 4 s through the player and synth, as a CRC */
static uint32_t v8_render(void)
{
    static tn_t ns[8][200];
    const tn_t *nl[8];
    uint32_t cnt[8], ch, i, crc = 0, f, id[8];
    int32_t out[2 * CTL], peak = 0;
    reset();
    for (ch = 0; ch < 8; ch++) {
        uint32_t k = 0;
        for (i = 0; i < 64; i++) {
            ns[ch][k++] = (tn_t){i * 48, 72, (uint8_t)(40 + ch * 5 + i % 7), 100};
            if (i % 4 == 0) ns[ch][k++] = (tn_t){i * 48, 40, (uint8_t)(47 + ch * 5 + i % 7), 90};
        }
        nl[ch] = ns[ch]; cnt[ch] = k;
    }
    build(1200, 8 * 384, 8, nl, cnt);
    player_load(pb);
    id[0] = sound_find("ANALOG", "SAW LEAD"); id[1] = sound_find("ANALOG", "SQR BASS"); id[2] = sound_find("ANALOG", "SOFT PAD");
    id[3] = sound_find("DIGITAL", "FM BASS"); id[4] = sound_find("VOICE", "ROBOT"); id[5] = sound_find("PHYS", "PLUCK BASS");
    id[6] = first_of(&ENG_FM6); id[7] = sound_find("TRIO", "SYNC LEAD");
    for (ch = 0; ch < 8; ch++) { CHECK(id[ch] < sound_count, "sound %u", ch); synth_sound(ch, id[ch]); }
    player_play();
    for (f = 0; f < 4u * 44100u; f += CTL) {
        player_tick(CTL);
        memset(out, 0, sizeof out);
        synth_apply();
        synth_render(out, CTL);
        crc = crc_upd(crc, out, sizeof out);
        for (i = 0; i < 2 * CTL; i++) if (out[i] > peak) peak = out[i];
    }
    CHECK(peak > 3000, "it sounds: peak %d", peak);
    player_stop(); play_frames(CTL);
    return crc;
}

#define V8_CRC 0x0001CCB7u                      /* recorded with v8's synth (before US-30) */
static void test_30_1_a_v8_piece_renders_bit_identical(void)
{
    uint32_t crc = v8_render();
    printf("30.1 v8 piece render %08X\n", crc);
    CHECK(crc == V8_CRC, "a piece without mode bytes renders as v8: %08X, recorded %08X", crc, V8_CRC);
    for (crc = 0; crc < 8; crc++) CHECK(pl_v.ch[crc].mode == VM_POLY && pl_v.ch[crc].glide == 0, "v8 channel %u: POLY", crc);
}

static void blocks(uint32_t n)                  /* the synth alone, n blocks */
{
    int32_t out[2 * CTL];
    while (n--) { memset(out, 0, sizeof out); synth_apply(); synth_render(out, CTL); }
}

static uint32_t sine_key(void) { return sound_find("ANALOG", "SINE KEY"); }

static void test_30_2_mono_retriggers_legato_glides(void)
{
    uint32_t age, n, want, step;
    reset();
    synth_sound(0, sine_key());
    synth_voice(0, VM_MONO, 0);
    CHECK(trk[0].p[P_VOICE] == V_MONO && chs[0].cap == 1, "MONO: one voice");
    synth_on(0, 60, 100); blocks(20); age = trk[0].v[0].age;
    synth_on(0, 64, 100);
    CHECK(synth_count(0) == 1 && trk[0].v[0].note == 64 && trk[0].v[0].stage == 1 && trk[0].v[0].age != age,
          "MONO: the new note retriggers the one voice");
    synth_on(0, 67, 100);
    synth_off(0, 67);
    CHECK(trk[0].v[0].note == 64 && trk[0].v[0].gate && synth_count(0) == 1, "last-note priority: 67 let go, 64 (held) again");
    synth_off(0, 60); synth_off(0, 64);
    CHECK(!trk[0].v[0].gate && trk[0].mono_note == 0, "all let go: released");
    blocks(3000);
    CHECK(synth_active() == 0, "silent");

    synth_voice(0, VM_LEGATO, 0);
    synth_on(0, 60, 100); blocks(40); age = trk[0].v[0].age;
    CHECK(trk[0].v[0].stage == 2, "held: decay / sustain");
    synth_on(0, 64, 100);
    CHECK(trk[0].v[0].note == 64 && trk[0].v[0].stage == 2 && trk[0].v[0].age == age && trk[0].v[0].pitch_cur == 64 * 16,
          "LEGATO: an overlapping note changes the pitch, no retrigger (glide 0: at once)");
    synth_off(0, 60); synth_off(0, 64); blocks(3000);

    synth_voice(0, VM_LEGATO, 86);              /* an octave in ~511 ms: RATE, SLOOP voice.c */
    synth_on(0, 60, 100); blocks(10);
    synth_on(0, 64, 100);
    step = ENV_LIN[86] * 3u >> 10;
    want = (4u * 4096u + step - 1u) / step;     /* blocks to glide 4 semitones */
    for (n = 0; n < 4000 && trk[0].v[0].pitch_cur != 64 * 16; n++) {
        blocks(1);
        if (n == want / 2u) CHECK(trk[0].v[0].pitch_cur > 61 * 16 && trk[0].v[0].pitch_cur < 63 * 16, "half way: %d", trk[0].v[0].pitch_cur);
    }
    CHECK(n + 1u >= want * 9u / 10u && n + 1u <= want * 11u / 10u, "the glide takes %u blocks, want %u (+-10%%)", n + 1u, want);
    synth_off(0, 60); synth_off(0, 64); blocks(3000);
    CHECK(synth_active() == 0, "silent");
    printf("30.2 MONO retriggers, LEGATO changes pitch, glide 86: 4 semitones in %u blocks (%u)\n", n + 1u, want);
}

static void test_30_3_the_sound_brings_its_voice_an_override_wins(void)
{
    uint32_t fm = sound_find("DIGITAL", "FM BASS"), rb = sound_find("VOICE", "ROBOT"), tb = sound_find("VOICE", "TALKBOX");
    reset();
    synth_voice(0, VM_AUTO, 0);
    synth_sound(0, fm);
    CHECK(trk[0].p[P_VOICE] == V_LEGATO && trk[0].p[P_GLIDE] == 36 && chs[0].cap == 1, "FM BASS: MONO preset, GLIDE 36 (its XP)");
    synth_sound(0, rb);
    CHECK(trk[0].p[P_VOICE] == V_LEGATO && trk[0].p[P_GLIDE] == 20, "ROBOT: GLIDE 20");
    synth_sound(0, sound_find("ANALOG", "SAW LEAD"));
    CHECK(trk[0].p[P_VOICE] == V_LEGATO && trk[0].p[P_GLIDE] == 0, "SAW LEAD (Felucca, MONO): LEGATO, no glide");
    synth_sound(0, sound_find("ANALOG", "SOFT PAD"));
    CHECK(trk[0].p[P_VOICE] == V_POLY && chs[0].cap == NVOICE, "SOFT PAD: POLY");
    synth_sound(0, sound_default(ROLE_DRUMS));
    CHECK(trk[0].p[P_VOICE] == V_POLY && trk[0].p[P_GLIDE] == 0, "a kit: POLY");
    synth_sound(0, tb);
    synth_on(0, 57, 100); blocks(5); synth_on(0, 64, 100); blocks(2);
    CHECK(trk[0].p[P_GLIDE] == 60 && trk[0].v[0].pitch_cur > 57 * 16 && trk[0].v[0].pitch_cur < 64 * 16, "TALKBOX glides (60): %d",
          trk[0].v[0].pitch_cur);
    synth_all_off(); blocks(3000);
    synth_voice(0, VM_POLY, 0);
    CHECK(trk[0].p[P_VOICE] == V_POLY && trk[0].p[P_GLIDE] == 0, "POLY wins over TALKBOX's own");
    synth_on(0, 57, 100); blocks(5); synth_on(0, 64, 100); blocks(2);
    CHECK(trk[0].v[0].pitch_cur == 64 * 16, "POLY: no glide");
    synth_all_off(); blocks(3000);
    synth_voice(0, VM_MONO, 100);
    synth_sound(0, fm);                         /* (another sound keeps the channel's override) */
    CHECK(trk[0].p[P_VOICE] == V_MONO && trk[0].p[P_GLIDE] == 100, "MONO 100 wins over FM BASS's LEGATO 36");
    printf("30.3 AUTO: FM BASS LEGATO 36, ROBOT 20, TALKBOX 60; POLY / MONO overrides win\n");
}

static voice_t *gated_newest(uint32_t ch, uint32_t *ngated)  /* the channel's newest held voice; how many are held */
{
    voice_t *v = 0;
    uint32_t k;
    *ngated = 0;
    for (k = 0; k < NVOICE; k++)
        if (trk[ch].v[k].active && trk[ch].v[k].gate) { (*ngated)++; if (!v || trk[ch].v[k].age > v->age) v = &trk[ch].v[k]; }
    return v;
}

/* (v9, SPEC decision 54) SLIDE on every voice mode: the slid note's voice glides into the next note, no second voice */
static void test_30_5_slide_glides_into_the_next_note(void)
{
    static const tn_t ns[4] = {{0, 24, 60, 100 | PIECE_SLIDE}, {24, 24, 64, 100}, {96, 24, 60, 100}, {120, 24, 67, 100}};
    static const uint32_t modes[3] = {VM_LEGATO, VM_MONO, VM_POLY};
    const tn_t *nl[1] = {ns};
    uint32_t cnt[1] = {4}, n, m, held = 0, held_mid = 9;
    voice_t *v;
    for (m = 0; m < 3u; m++) {
        int32_t mid = 0;
        reset();
        build(1200, 384, 1, nl, cnt);
        CHECK(piece_check(pb, pc_u32(pb + 40)) == PE_OK, "a SLIDE note: a valid piece");
        player_load(pb);
        synth_sound(0, sine_key());
        synth_voice(0, modes[m], 0);
        pl_loop = 0;
        player_play();
        for (n = 0; n < frame_of(30, 1200) / CTL; n++) {   /* tick 24 + a few blocks: 60 slid into 64 */
            play_frames(CTL);
            if (pl_tick == 25 && !mid) { v = gated_newest(0, &held_mid); mid = v ? v->pitch_cur : 0; }
        }
        v = gated_newest(0, &held);
        CHECK(mid > 60 * 16 && mid < 64 * 16 && v && v->note == 64 && held_mid == 1 && held == 1,
              "mode %u SLIDE: 60 glides into 64 on one voice (SLOOP's SLIDE_T, glide 0): %d, %u held", modes[m], mid, held_mid);
        play_frames(frame_of(122, 1200) - frame_of(30, 1200));
        v = gated_newest(0, &held);
        CHECK(v && v->note == 67 && v->pitch_cur == 67 * 16 && held == 1, "mode %u no SLIDE: 60 to 67 at once", modes[m]);
        play_frames(frame_of(180, 1200) - frame_of(122, 1200));
        v = gated_newest(0, &held);
        CHECK(pl_noff == 0 && trk[0].mono_note == 0 && !held, "every note let go before the end (the SLIDE's too)");
        play_frames(3u * 44100u);               /* (the piece ends, 2 s, and stops) */
        CHECK(synth_active() == 0 && pl_noff == 0 && trk[0].mono_note == 0, "nothing stuck: %u voices, %u held, note %u",
              synth_active(), pl_noff, trk[0].mono_note);
    }
    reset();                                    /* POLY: a chord's other note, due the SLIDE note's tick, keeps its own */
    synth_sound(0, sine_key());
    synth_voice(0, VM_POLY, 0);
    synth_on(0, 60, 100); synth_slide(0); synth_on(0, 67, 100); blocks(5);
    synth_on(0, 64, 100); blocks(8);
    v = gated_newest(0, &held);
    CHECK(held == 2 && synth_count(0) == 2 && v && v->note == 67, "POLY chord: 60 slid into 64, 67 held: %u held", held);
    CHECK(trk[0].v[0].note == 64 && trk[0].v[0].pitch_cur > 60 * 16 && trk[0].v[0].pitch_cur < 64 * 16, "60's voice glides");
    synth_off(0, 60);                           /* (the slid note's late off: 64 goes on) */
    CHECK(trk[0].v[0].gate && trk[0].v[0].note == 64, "the slid note's off leaves 64 sounding");
    synth_all_off(); blocks(3000);
    printf("30.5 SLIDE glides into the next note on LEGATO, MONO and POLY (one voice), then nothing held\n");
}

static void test_30_6_hostile_voices(void)
{
    uint32_t k, ch;
    reset();
    synth_sound(0, sine_key());
    synth_voice(0, VM_LEGATO, 127);             /* glide 127: an octave in 10 s */
    synth_on(0, 40, 100); synth_on(0, 100, 100); blocks(50);
    k = (uint32_t)(trk[0].v[0].pitch_cur << 8) + (uint32_t)trk[0].v[0].pitch_frac;   /* (1/4096 semitone) */
    CHECK(k > 40u * 4096u && k < 41u * 4096u, "glide 127 crawls: %u", k);
    synth_voice(0, VM_POLY, 0);                 /* mode change while it sounds */
    synth_on(0, 50, 100);
    synth_off(0, 40); synth_off(0, 100); synth_off(0, 50);
    for (k = 0; k < NVOICE; k++) CHECK(!trk[0].v[k].gate, "POLY now: every note let go, voice %u", k);
    synth_on(0, 50, 100); synth_on(0, 54, 100); synth_on(0, 57, 100);
    synth_voice(0, VM_MONO, 0);                 /* and back, three POLY voices held */
    synth_on(0, 62, 100);
    synth_off(0, 50); synth_off(0, 54); synth_off(0, 57); synth_off(0, 62);
    for (k = 0; k < NVOICE; k++) CHECK(!trk[0].v[k].gate, "MONO now: every note let go, voice %u", k);
    blocks(3000);
    CHECK(synth_active() == 0, "none stuck");
    for (k = 0; k < 4; k++) synth_on(0, 60 + 3 * k, 100);   /* a 4-note chord step on MONO: the last note */
    CHECK(synth_count(0) == 1 && trk[0].v[0].note == 69, "MONO chord: one voice, the last note (%u)", trk[0].v[0].note);
    for (k = 0; k < 4; k++) synth_off(0, 60 + 3 * k);
    CHECK(!trk[0].v[0].gate && trk[0].nmono == 0, "chord let go: released");
    blocks(3000);
    synth_voice(0, VM_LEGATO, 90);              /* LEGATO, no overlap: retriggered, no glide */
    synth_on(0, 60, 100); blocks(10); synth_off(0, 60); blocks(2);
    synth_on(0, 64, 100);
    CHECK(trk[0].v[0].stage == 1 && trk[0].v[0].pitch_cur == 64 * 16, "LEGATO without overlap: a new note, no glide");
    synth_off(0, 64);
    synth_voice(0, VM_N + 3, 300);              /* out of range: POLY, glide 127 */
    CHECK(ch_vm[0] == VM_POLY && ch_vg[0] == 127 && trk[0].p[P_VOICE] == V_POLY, "bad mode: POLY");
    blocks(3000);
    synth_voice(0, VM_POLY, 0);
    for (ch = 1; ch < 3; ch++) { synth_sound(ch, sound_find("ANALOG", "SOFT PAD")); for (k = 0; k < 8; k++) synth_on(ch, 40 + k, 100); }
    CHECK(synth_active() == VOICE_BUDGET, "16 voices busy: %u", synth_active());
    synth_sound(3, sine_key());
    synth_voice(3, VM_MONO, 40);
    synth_on(3, 72, 100); synth_on(3, 76, 100);
    CHECK(synth_active() <= VOICE_BUDGET && synth_count(3) == 1 && trk[3].v[0].note == 76, "MONO takes a voice: %u at once",
          synth_active());
    synth_all_off(); blocks(1500);
    CHECK(synth_active() == 0, "all let go: silent (%u)", synth_active());
    printf("30.6 glide 0 / 127, mode changes while sounding, MONO chords, LEGATO without overlap, a full pool: clean\n");
}

/* ---- US-34: live send ---- */
static uint8_t lv_a[PIECE_MAX_BYTES], lv_b[PIECE_MAX_BYTES], lv_c[PIECE_MAX_BYTES];

/* A: 3 pad channels holding notes across every bar line (and a note that would start right at bar 2), 4 bars at 120;
 * B: 2 channels at 90, a note on its tick 0 and its tick 96; C: one channel at 150 */
static void lv_pieces(void)
{
    static tn_t a0[] = {{0, 1536, 60, 100}, {384, 50, 72, 100}}, a1[] = {{300, 1000, 40, 100}}, a2[] = {{0, 1536, 67, 100}};
    static tn_t b0[] = {{0, 300, 80, 100}, {96, 50, 81, 100}}, b1[] = {{0, 700, 50, 100}};
    static tn_t c0[] = {{0, 200, 90, 100}};
    const tn_t *na[3] = {a0, a1, a2}, *nb[2] = {b0, b1}, *nc[1] = {c0};
    uint32_t ca[3] = {2, 1, 1}, cb[2] = {2, 1}, cc[1] = {1}, n;
    n = build(1200, 4 * 384, 3, na, ca); memcpy(lv_a, pb, n);
    n = build(900, 2 * 384, 2, nb, cb); memcpy(lv_b, pb, n);
    n = build(1500, 384, 1, nc, cc); memcpy(lv_c, pb, n);
}

static void test_34_1_live_switch_at_the_next_bar_no_hanging_note_no_gap(void)
{
    uint32_t i, sounding[3][128], gap = 0, worst = 0, b_on = 0, a_after = 0, first_b = 0, second_b = 0;
    int32_t peaks[64];
    reset();
    lv_pieces();
    CHECK(player_load(lv_a) == 0, "load A");
    player_play();
    play_frames(frame_of(100, 1200));                   /* A plays, tick ~100: its notes sound */
    player_next(lv_b, 900, 0, 0);
    while (now + CTL < frame_of(384, 1200) - 8 * CTL) play_frames(CTL);
    CHECK(nswitch == 0 && player_pos() < 384, "nothing before the bar line (tick %u)", player_pos());
    for (i = 0; i < 64; i++) { play_frames(CTL); peaks[i] = blk_peak; }   /* across the bar line */
    CHECK(nswitch == 1 && player_swaps() == 1, "one switch: %u", nswitch);
    CHECK(switch_frame <= frame_of(384, 1200) && switch_frame + CTL > frame_of(384, 1200),
          "at A's bar line: frame %u, exact %u", switch_frame, frame_of(384, 1200));
    CHECK(switch_steps == steps || steps - switch_steps <= 2u, "the switch scans nothing (%u steps)", steps - switch_steps);
    for (i = 0; i < 64; i++) { if (peaks[i] == 0) { gap++; if (gap > worst) worst = gap; } else gap = 0; }
    CHECK(peaks[0] > 0 && peaks[63] > 0, "A sounds before, B after (%d %d)", peaks[0], peaks[63]);
    CHECK(worst <= 1, "no gap of more than a block at the switch: %u silent blocks", worst);
    memset(sounding, 0, sizeof sounding);               /* every A note sounding at the switch got its note-off there */
    for (i = 0; i < nrec; i++) {
        if (rec[i].frame < switch_frame) { if (rec[i].on) sounding[rec[i].ch][rec[i].pitch]++; else if (sounding[rec[i].ch][rec[i].pitch]) sounding[rec[i].ch][rec[i].pitch]--; continue; }
        if (rec[i].frame == switch_frame && !rec[i].on && sounding[rec[i].ch][rec[i].pitch]) sounding[rec[i].ch][rec[i].pitch]--;
        if (rec[i].on && (rec[i].pitch == 60 || rec[i].pitch == 72 || rec[i].pitch == 40 || rec[i].pitch == 67)) a_after++;
        if (rec[i].on && (rec[i].pitch == 80 || rec[i].pitch == 50)) { b_on++; if (!first_b) first_b = rec[i].frame; }
    }
    for (i = 0; i < 3 * 128; i++) CHECK(!sounding[i / 128][i % 128], "A's note ch %u pitch %u left hanging", i / 128, i % 128);
    CHECK(a_after == 0, "no note of A after the switch (its bar-2 note 72 neither): %u", a_after);
    CHECK(b_on == 2 && first_b == switch_frame, "B's bar 1 starts at the switch: %u notes, frame %u", b_on, first_b);
    CHECK(pl_v.nch == 2 && pl_tempo == 900 && pl_piece == lv_b, "B, at its own tempo");
    play_frames(frame_of(200, 900));
    for (i = 0; i < nrec; i++) if (rec[i].on && rec[i].pitch == 81) second_b = rec[i].frame;
    CHECK(second_b > switch_frame && second_b - switch_frame <= frame_of(96, 900) + CTL && second_b - switch_frame + CTL > frame_of(96, 900),
          "B's tick 96 at its 90 BPM from the switch: %u frames after", second_b - switch_frame);
    printf("34.1 switch at the next bar line, from bar 1 at its tempo; every note let go there; no audio gap; O(1)\n");
}

static void test_34_4_newer_wins_bar_line_moved_bytes_dropped_channels(void)
{
    uint32_t i, on2 = 0;
    reset();
    lv_pieces();
    player_load(lv_a);
    player_play();
    player_mute(1, 1); player_mute(2, 1);
    play_frames(frame_of(50, 1200));
    player_next(lv_b, 900, 2u, 0);                      /* B, then C before the bar: C wins, one switch */
    play_frames(frame_of(50, 1200));
    player_next(lv_c, 1500, 0, 0);
    play_frames(frame_of(384, 1200));
    CHECK(nswitch == 1 && pl_piece == lv_c && pl_tempo == 1500 && pl_v.nch == 1, "the newer wins: %u switches", nswitch);
    CHECK(pl_mute == 0 && pl_mute_on == 0, "mutes as given");
    while (player_pos() != 0) play_frames(CTL);          /* C (1 bar) wraps to its start: armed exactly at a bar line */
    while (player_pos() == 0) play_frames(CTL);
    reset();
    player_load(lv_a);
    player_play();
    while (player_pos() < 384) play_frames(CTL);
    CHECK(player_pos() == 384 && nswitch == 0, "at the bar line itself (tick %u)", player_pos());
    player_next(lv_b, 900, 2u, 0);                      /* the bar line just passed: the next one */
    play_frames(CTL);
    CHECK(nswitch == 0, "not this bar line, it has played");
    nrec = 0;
    play_frames(frame_of(384, 1200));
    CHECK(nswitch == 1 && switch_frame <= frame_of(768, 1200) && switch_frame + CTL > frame_of(768, 1200), "the next one: frame %u", switch_frame);
    for (i = 0; i < nrec; i++) if (rec[i].on && rec[i].frame >= switch_frame && rec[i].ch == 1) on2++;
    CHECK(on2 == 0 && pl_mute == 2u, "B's channel 2 muted as given: %u notes", on2);
    for (i = 0; i < nrec; i++) CHECK(!(rec[i].on && rec[i].frame >= switch_frame && rec[i].ch == 2), "A's channel 3, not in B: dropped");
    memcpy(lv_c, lv_b, sizeof lv_c);                    /* the main loop moves the bytes (player_moved): no jump, no note again */
    nrec = 0;
    player_hold();
    player_moved(lv_c);
    play_frames(CTL * 4);
    for (i = 0; i < nrec; i++) CHECK(!rec[i].on, "moved: nothing replays (pitch %u)", rec[i].pitch);
    CHECK(pl_piece == lv_c && player_playing(), "plays on from the copy");
    player_next(lv_a, 1200, 0, 0);                      /* stopped while armed: no switch while stopped */
    player_stop();
    play_frames(CTL);
    player_next(0, 0, 0, 0);
    player_play();
    play_frames(frame_of(800, 900));
    CHECK(nswitch == 1 && pl_piece == lv_c, "disarmed: B plays on (%u)", nswitch);
    printf("34.4 the newer of two armed pieces wins, once; armed on a bar line: the next; dropped / muted channels; moved bytes\n");
}

int main(void)
{
    test_34_1_live_switch_at_the_next_bar_no_hanging_note_no_gap();
    test_34_4_newer_wins_bar_line_moved_bytes_dropped_channels();
    test_30_1_a_v8_piece_renders_bit_identical();
    test_30_2_mono_retriggers_legato_glides();
    test_30_3_the_sound_brings_its_voice_an_override_wins();
    test_30_5_slide_glides_into_the_next_note();
    test_30_6_hostile_voices();
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
