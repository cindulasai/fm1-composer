/* SPDX-License-Identifier: GPL-3.0-only */
/* steps.c (US-25): the STEPS screen's model. A channel's notes seen as a grid of 1/16 steps (24 ticks) from a start
 * tick (the section's, or the piece's), 16 a page, up to 64. There is no second note store (SPEC decision 38): every
 * change goes through edit.c, so the bytes are what fm1c's encode() makes of the same notes and each change undoes.
 * A step holds the notes that start inside it: a key adds one (up to ST_CHORD) or takes it away; its length in
 * steps is its duration / 24 (a tie is a longer note, sounding to the end of its last step); velocity 1..127; SLIDE
 * (US-30, the velocity's PIECE_SLIDE bit): the step's notes glide into the next step's (player.c holds them into it).
 * A channel's loop length (0: none, 1..64 steps; polymeter) and swing (0..100) are bytes of its record (piece.h,
 * edit_chan); player.c plays them. */
#define ST_TICKS 24u                            /* a 1/16 step at 96 PPQ */
#define ST_PAGE 16u
#define ST_CHORD 4u
enum { ED_CHORD = 5 };                          /* a 5th note in a step */

/* the notes of channel ch starting in the step at t0: how many; the first ST_CHORD in out (tick, pitch order) */
static uint32_t steps_cell(const uint8_t *b, uint32_t ch, uint32_t t0, piece_note_t *out)
{
    piece_view_t v;
    piece_cur_t c;
    piece_note_t n;
    uint32_t k = 0;
    piece_view(b, &v);
    piece_cursor(&v.ch[ch % PIECE_MAX_CH], &c);
    while (piece_next(&c, &n) && n.tick < t0 + ST_TICKS)
        if (n.tick >= t0) { if (k < ST_CHORD) out[k] = n; k++; }
    return k;
}

/* a page of channel ch from t0: cell[i] = 2 a note starts in step i, 1 an earlier one sounds through it, 0 none */
static void steps_row(const piece_view_t *v, uint32_t ch, uint32_t t0, uint8_t *cell)
{
    piece_cur_t c;
    piece_note_t n;
    uint32_t i, end = t0 + ST_PAGE * ST_TICKS;
    for (i = 0; i < ST_PAGE; i++) cell[i] = 0;
    piece_cursor(&v->ch[ch % PIECE_MAX_CH], &c);
    while (piece_next(&c, &n) && n.tick < end) {
        uint32_t s0, s1;
        if (n.tick + n.dur <= t0) continue;
        s0 = n.tick > t0 ? (n.tick - t0) / ST_TICKS : 0u;
        s1 = (n.tick + n.dur - 1u - t0) / ST_TICKS;
        if (n.tick >= t0) cell[s0] = 2;
        for (i = s0; i <= s1 && i < ST_PAGE; i++) if (!cell[i]) cell[i] = 1;
    }
}

/* a key in the step at t0: its note there goes; else it starts there, len steps long (1..64), if the step has room */
static int steps_key(uint8_t *b, uint32_t ch, uint32_t t0, uint32_t pitch, uint32_t len, uint32_t vel)
{
    piece_view_t v;
    piece_cur_t c;
    piece_note_t n;
    uint32_t k = 0;
    piece_view(b, &v);
    piece_cursor(&v.ch[ch % PIECE_MAX_CH], &c);
    while (piece_next(&c, &n) && n.tick < t0 + ST_TICKS) {
        if (n.tick < t0) continue;
        if (n.pitch == pitch) return edit_delete(b, ch, n.tick, pitch);
        k++;
    }
    if (k >= ST_CHORD) return ED_CHORD;
    if (len < 1u || len > PIECE_MAX_STEPS) return ED_ARG;
    return edit_insert(b, ch, t0, pitch, len * ST_TICKS, vel);
}

/* every note starting in the step at t0: len steps (1..64), velocity vel; ED_NONE when the step is empty */
static int steps_set(uint8_t *b, uint32_t ch, uint32_t t0, uint32_t len, uint32_t vel)
{
    piece_view_t v;
    piece_cur_t c;
    piece_note_t n;
    int r = ED_NONE;
    if (len < 1u || len > PIECE_MAX_STEPS) return ED_ARG;
    piece_view(b, &v);
    piece_cursor(&v.ch[ch % PIECE_MAX_CH], &c);
    while (piece_next(&c, &n) && n.tick < t0 + ST_TICKS) {   /* (edit_set changes bytes in place: the cursor holds) */
        if (n.tick < t0) continue;
        r = edit_set(b, ch, n.tick, n.pitch, len * ST_TICKS, vel | (n.slide ? PIECE_SLIDE : 0u));   /* (SLIDE kept) */
        if (r) break;
    }
    return r;
}

/* SLIDE on the step at t0: on for every note starting in it, or off when all have it; ED_NONE when it is empty */
static int steps_slide(uint8_t *b, uint32_t ch, uint32_t t0)
{
    piece_note_t cn[ST_CHORD];
    uint32_t n = steps_cell(b, ch, t0, cn), i, on = 0;
    int r = ED_NONE;
    for (i = 0; i < n && i < ST_CHORD; i++) on |= !cn[i].slide;
    for (i = 0; i < n && i < ST_CHORD; i++) {   /* (edit_set changes bytes in place: the notes stay where they were) */
        r = edit_set(b, ch, cn[i].tick, cn[i].pitch, cn[i].dur, cn[i].vel | (on ? PIECE_SLIDE : 0u));
        if (r) break;
    }
    return r;
}
