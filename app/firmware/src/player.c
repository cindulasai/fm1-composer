/* SPDX-License-Identifier: GPL-3.0-only */
/* player.c: plays a checked piece (piece.h) on the synth (synth.c), in the audio interrupt.
 *
 * Time: an integer accumulator, so there is no drift: each frame adds tempo x10 * PPQ, and a tick passes every
 * 60 s * 10 * 44100 units. player_tick() runs once a 32-frame block: a note sounds within one block (0.73 ms)
 * of its exact time, at any tempo, for the whole piece.
 *
 * The main loop asks; the interrupt does. player_play/stop/tempo/mute/solo only write single volatile words
 * (one writer each), which player_tick() applies; player_load() works only while stopped.
 *
 * Sections (piece v2): player_launch() queues one; it starts at the next bar line (or where the playing section
 * ends, if sooner). With section loop on, the playing section repeats. A jump costs O(1) in the interrupt: at
 * player_load() (main loop) each channel's cursor is saved at each section's start, so a jump only copies them. */
#ifndef PL_NOTE_ON
#define PL_NOTE_ON synth_on
#define PL_NOTE_OFF synth_off
#define PL_CH_OFF synth_ch_off
#define PL_HUSH synth_hush
#endif

#define PL_TICK_UNITS (60u * 10u * 44100u)      /* accumulator units a tick (tempo x10, per frame) */
#define PL_MAXOFF 128u                          /* notes held at once, all channels */
#define PL_BAR (4u * PIECE_PPQ)
#define PL_NONE 255u                            /* no section */
#ifndef POOL
#define POOL
#endif
#ifndef PL_STEP
#define PL_STEP()                               /* (tests count the cursor steps an interrupt takes) */
#endif

typedef struct { uint32_t off; uint8_t ch, pitch; } pl_off_t;

static piece_view_t pl_v;
static piece_cur_t pl_cur[PIECE_MAX_CH];
static piece_note_t pl_next[PIECE_MAX_CH];
static uint8_t pl_has[PIECE_MAX_CH];
static pl_off_t pl_offs[PL_MAXOFF];
static uint32_t pl_noff, pl_tick, pl_acc, pl_dropped;
static uint8_t pl_loaded, pl_running;
static const uint8_t *pl_piece;                /* the bytes player_load() was given */
static uint32_t pl_mute_on, pl_solo_on;         /* what the interrupt last applied */
/* the main loop writes these; the interrupt reads them */
static volatile uint32_t pl_want_run, pl_tempo, pl_mute, pl_solo, pl_loop = 1, pl_start;
static volatile uint32_t pl_pos;                /* the interrupt writes: the current tick */
static volatile uint32_t pl_state;              /* the interrupt writes: 1 playing, 0 stopped */
/* sections: the cursors at each one's start (24 x 8 x 12 B = 2.3 KB on the chip), the one playing */
static piece_cur_t pl_snap[PIECE_MAX_SECT][PIECE_MAX_CH] POOL;
static uint32_t pl_sec = PL_NONE, pl_qdone;
static volatile uint32_t pl_q, pl_sloop;        /* the main loop writes: (launch count << 8) | section; loop on */
static volatile uint32_t pl_secv = PL_NONE;     /* the interrupt writes: the section playing */

static uint32_t pl_audible(uint32_t ch, uint32_t mute, uint32_t solo)
{
    return solo ? (solo >> ch & 1u) : !(mute >> ch & 1u);
}

static void pl_release_all(void)
{
    uint32_t i;
    for (i = 0; i < pl_noff; i++) PL_NOTE_OFF(pl_offs[i].ch, pl_offs[i].pitch);
    pl_noff = 0;
}

static void pl_rewind(void)
{
    uint32_t ch;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) {
        pl_has[ch] = 0;
        if (ch < pl_v.nch) {
            piece_cursor(&pl_v.ch[ch], &pl_cur[ch]);
            pl_has[ch] = (uint8_t)piece_next(&pl_cur[ch], &pl_next[ch]);
        }
    }
    pl_tick = 0;
    pl_acc = 0;
    pl_pos = 0;
    pl_secv = pl_sec = pl_v.nsect && piece_sect_start(&pl_v, 0) == 0u ? 0u : PL_NONE;
}

/* every channel's cursor at each section's start: one pass over the events (main loop, stopped) */
static void pl_snapshots(void)
{
    uint32_t ch, s;
    for (ch = 0; ch < pl_v.nch; ch++) {
        piece_cur_t c, at;
        piece_note_t n;
        int has;
        piece_cursor(&pl_v.ch[ch], &c);
        at = c;
        has = piece_next(&c, &n);
        for (s = 0; s < pl_v.nsect; s++) {
            uint32_t t = piece_sect_start(&pl_v, s);
            while (has && n.tick < t) { at = c; has = piece_next(&c, &n); }
            pl_snap[s][ch] = at;                /* (its next note is the first at or after t) */
        }
    }
}

/* the interrupt: play section s from its start, now (what sounds lets go) */
static void pl_jump(uint32_t s)
{
    uint32_t ch;
    pl_release_all();
    for (ch = 0; ch < pl_v.nch; ch++) {
        PL_STEP();
        pl_cur[ch] = pl_snap[s][ch];
        pl_has[ch] = (uint8_t)piece_next(&pl_cur[ch], &pl_next[ch]);
    }
    pl_tick = piece_sect_start(&pl_v, s);
    pl_secv = pl_sec = s;
}

/* load a checked piece (stopped only): 0, or -1 while playing */
static int player_load(const uint8_t *piece)
{
    if (pl_state || pl_want_run) return -1;
    pl_piece = piece;
    piece_view(piece, &pl_v);
    pl_tempo = pl_v.tempo_x10;
    pl_mute = pl_solo = 0;
    pl_mute_on = pl_solo_on = 0;
    pl_noff = 0;
    pl_dropped = 0;
    pl_qdone = pl_q >> 8;                       /* (a launch for the last piece is forgotten) */
    pl_rewind();
    pl_snapshots();
    pl_loaded = 1;
    return 0;
}

static void player_play(void) { if (pl_loaded) { pl_start = 0; pl_want_run = 1; } }
/* play from a tick (inside the piece; else from the start); notes begun before it stay silent */
static void player_play_from(uint32_t tick) { if (pl_loaded) { pl_start = tick < pl_v.len_ticks ? tick : 0u; pl_want_run = 1; } }
/* the piece's bytes changed (edit.c; stopped only): read them again, keeping tempo, mutes and solos */
static void player_refresh(void) { if (pl_loaded && !pl_state && !pl_want_run) { piece_view(pl_piece, &pl_v); pl_rewind(); pl_snapshots(); } }
static void player_stop(void) { pl_want_run = 0; }
static void player_tempo(uint32_t x10) { pl_tempo = x10 < 300u ? 300u : x10 > 3000u ? 3000u : x10; }
static void player_mute(uint32_t ch, int on) { if (ch < PIECE_MAX_CH) pl_mute = on ? pl_mute | 1u << ch : pl_mute & ~(1u << ch); }
static void player_solo(uint32_t ch, int on) { if (ch < PIECE_MAX_CH) pl_solo = on ? pl_solo | 1u << ch : pl_solo & ~(1u << ch); }
static uint32_t player_pos(void) { return pl_pos; }
static int player_playing(void) { return pl_state != 0; }
/* sections: queue section s (the last launch wins; stopped: it plays from its start at PLAY); loop the playing one */
static void player_launch(uint32_t s) { if (pl_loaded && s < pl_v.nsect) pl_q = (((pl_q >> 8) + 1u) << 8) | s; }
static void player_section_loop(int on) { pl_sloop = on != 0; }
static uint32_t player_section(void) { return pl_secv; }       /* the section playing, or PL_NONE */
static uint32_t player_queued(void) { return (pl_q >> 8) != pl_qdone ? (pl_q & 255u) : PL_NONE; }

/* the notes due at pl_tick: note-offs first, then note-ons of audible channels */
static void pl_seek(uint32_t t)                 /* (just started, at tick 0) skip to tick t */
{
    uint32_t ch;
    for (ch = 0; ch < pl_v.nch; ch++)
        while (pl_has[ch] && pl_next[ch].tick < t) { PL_STEP(); pl_has[ch] = (uint8_t)piece_next(&pl_cur[ch], &pl_next[ch]); }
    pl_tick = pl_pos = t;
    for (ch = 0; ch < pl_v.nsect && piece_sect_start(&pl_v, ch) <= t; ch++) pl_sec = ch;   /* (24 at most) */
    pl_secv = pl_sec;
}

static void pl_events(void)
{
    uint32_t i, ch;
    for (i = 0; i < pl_noff;) {
        if (pl_offs[i].off <= pl_tick) {
            PL_NOTE_OFF(pl_offs[i].ch, pl_offs[i].pitch);
            pl_offs[i] = pl_offs[--pl_noff];
        } else {
            i++;
        }
    }
    for (ch = 0; ch < pl_v.nch; ch++) {
        while (pl_has[ch] && pl_next[ch].tick == pl_tick) {
            const piece_note_t *n = &pl_next[ch];
            if (pl_audible(ch, pl_mute_on, pl_solo_on)) {
                if (pl_noff < PL_MAXOFF) {
                    PL_NOTE_ON(ch, n->pitch, n->vel);
                    pl_offs[pl_noff].off = pl_tick + n->dur;
                    pl_offs[pl_noff].ch = (uint8_t)ch;
                    pl_offs[pl_noff].pitch = n->pitch;
                    pl_noff++;
                } else {
                    pl_dropped++;
                }
            }
            pl_has[ch] = (uint8_t)piece_next(&pl_cur[ch], &pl_next[ch]);
        }
    }
}

/* (the interrupt, a tick just passed, the piece has sections) a launch due at this bar line or this section's end
 * jumps; else at the playing section's end, loop on, it starts again; else the next section begins. 1 = jumped */
static uint32_t pl_section_edge(void)
{
    uint32_t q = player_queued(), end = pl_sec == PL_NONE ? piece_sect_start(&pl_v, 0) : piece_sect_end(&pl_v, pl_sec);
    uint32_t edge = pl_tick >= end;
    if (q != PL_NONE && (edge || pl_tick % PL_BAR == 0u)) {
        pl_qdone = pl_q >> 8;
        pl_jump(q);
        return 1;
    }
    if (!edge) return 0;
    if (pl_sloop && pl_sec != PL_NONE) { pl_jump(pl_sec); return 1; }
    if (pl_tick < pl_v.len_ticks) pl_secv = pl_sec = pl_sec == PL_NONE ? 0u : pl_sec + 1u;
    return 0;
}

/* the audio interrupt, once a block of nframes (32) */
static void player_tick(uint32_t nframes)
{
    uint32_t ch, mute = pl_mute, solo = pl_solo, want = pl_want_run;
    if (!pl_loaded) return;
    if (mute != pl_mute_on || solo != pl_solo_on) {         /* channels that fall silent let go of their notes */
        for (ch = 0; ch < PIECE_MAX_CH; ch++)
            if (pl_audible(ch, pl_mute_on, pl_solo_on) && !pl_audible(ch, mute, solo)) {
                uint32_t i;
                PL_CH_OFF(ch);
                for (i = 0; i < pl_noff;) {
                    if (pl_offs[i].ch == ch) pl_offs[i] = pl_offs[--pl_noff];
                    else i++;
                }
            }
        pl_mute_on = mute;
        pl_solo_on = solo;
    }
    if (!want) {
        if (pl_running) {                       /* stop: silence at once, back to the start; a launch is forgotten */
            pl_qdone = pl_q >> 8;
            pl_release_all();
            PL_HUSH();
            pl_rewind();
            pl_running = 0;
            pl_state = 0;
        }
        return;
    }
    if (!pl_running) {
        pl_running = 1;
        pl_state = 1;
        if (player_queued() != PL_NONE) { pl_qdone = pl_q >> 8; pl_jump(pl_q & 255u); }   /* launched while stopped */
        else if (pl_start) pl_seek(pl_start);
        pl_pos = pl_tick;
        pl_events();                            /* the first tick sounds at once */
    }
    pl_acc += pl_tempo * PIECE_PPQ * nframes;
    while (pl_acc > PL_TICK_UNITS) {            /* a tick due exactly at the next block's start waits for it */
        pl_acc -= PL_TICK_UNITS;
        pl_tick++;
        /* (unless a launch or a section loop jumped) the end: loop (keeping the part of a tick already counted), or stop */
        if (!(pl_v.nsect && pl_section_edge()) && pl_tick >= pl_v.len_ticks) {
            uint32_t acc = pl_acc;
            pl_release_all();
            pl_rewind();
            pl_acc = acc;
            if (!pl_loop) {
                PL_HUSH();
                pl_want_run = 0;
                pl_running = 0;
                pl_state = 0;
                return;
            }
        }
        pl_events();
    }
    pl_pos = pl_tick;
}
