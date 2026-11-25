/* SPDX-License-Identifier: GPL-3.0-only */
/* edit.c: changes a checked piece (piece.h) in place, in a buffer of PIECE_MAX_BYTES. A note is named by its channel,
 * start tick and pitch. After a call that returns ED_OK the piece passes piece_check() and is byte for byte what
 * fm1c's encode() makes of the same notes (in a gap, rests of 65535 ticks first; notes in tick, pitch order);
 * after an error the bytes are untouched. Each change is logged in the current group (edit_mark() starts one: a
 * key, a knob turn); edit_undo() takes back the last group, of the last ED_UNDO changes kept. edit_chan() sets a
 * channel's loop length or swing (US-25), or its voice mode or glide (US-30), logged the same way. A velocity may carry
 * PIECE_SLIDE (US-30). A recording pass (US-26, live.c) can be longer than
 * the history: edit_pass_insert / edit_pass_delete log into a pass log of their own (ED_PASSN changes, then ED_PASS),
 * and one history entry stands for the whole pass; only the last pass is kept (a new one drops the one before, and
 * what is older than it, from the history). edit_tool (US-31, the part tools) rewrites a channel's notes; a hold of
 * its button is one history entry, its snapshot in the pass log's room (a hold and a pass: only the last kept).
 * Edit only while the player is stopped (the bytes move), then player_refresh(). */
#define ED_UNDO 64u
#define ED_GAP 65535u
#define ED_PASSN 640u                           /* a recording pass's changes (8 B each) */
#ifndef POOL
#define POOL
#endif
enum { ED_OK = 0, ED_ARG = 1, ED_FULL = 2, ED_EXISTS = 3, ED_NONE = 4, ED_PASS = 6 };   /* (5: steps.c's) */
enum { EO_INS = 1, EO_DEL = 2, EO_SET = 3, EO_CHAN = 4, EO_PASS = 5, EO_SNAP = 6 };   /* EO_CHAN: tick = the record byte, pitch = its
                                                 * value; EO_SNAP (a tools hold): tick = the events kept, pitch = the loop */

typedef struct { uint32_t tick, grp; uint16_t dur; uint8_t op, ch, pitch, vel; } ed_op_t;
static ed_op_t ed_log[ED_UNDO];
static uint32_t ed_top, ed_n, ed_grp;           /* a ring: ed_n entries ending at ed_top; the group now */
typedef struct { uint32_t tick; uint16_t dur; uint8_t pitch, vel; } ed_pn_t;   /* vel 0: inserted, else deleted */
static ed_pn_t ed_pass[ED_PASSN] POOL;
static uint32_t ed_pn, ed_pnew;                 /* the pass log's entries; the next pass change starts a pass */
static uint32_t ed_hold, ed_hch, ed_hl;         /* a tools hold under way (US-31): its channel, its part's length */

static void ed_w16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void ed_w32(uint8_t *p, uint32_t v) { ed_w16(p, v & 0xFFFFu); ed_w16(p + 2, v >> 16); }
static uint8_t *ed_rec(uint8_t *b, uint32_t ch) { return b + PIECE_HDR + ch * PIECE_CHREC; }
static void ed_ev(uint8_t *e, uint32_t delta, uint32_t dur, uint32_t pitch, uint32_t vel)
{
    ed_w16(e, delta); ed_w16(e + 2, dur); e[4] = (uint8_t)pitch; e[5] = (uint8_t)vel;
}
static void ed_seal(uint8_t *b) { uint32_t n = pc_u32(b + 40); ed_w32(b + n - 4u, piece_crc32(b, n - 4u)); }

static void edit_forget(void) { ed_n = ed_pn = ed_hold = 0; }   /* a new piece: nothing to undo */
static void edit_mark(void) { ed_grp++; }       /* what follows is one gesture: undone together */
static uint32_t edit_undoable(void) { return ed_n; }

/* d > 0: open room for d events at byte at; d < 0: drop the -d events from at. Fixes channel ch's count, the
 * later channels' offsets and the length (the caller seals) */
static void ed_shift(uint8_t *b, uint32_t ch, uint32_t at, int32_t d)
{
    uint32_t len = pc_u32(b + 40), i, n = (uint32_t)(d < 0 ? -d : d) * PIECE_EV;
    if (d > 0) for (i = len; i-- > at;) b[i + n] = b[i];
    else for (i = at; i + n < len; i++) b[i] = b[i + n];
    ed_w32(ed_rec(b, ch) + 8, (uint32_t)((int32_t)pc_u32(ed_rec(b, ch) + 8) + d));
    for (i = ch + 1u; i < b[5]; i++) ed_w32(ed_rec(b, i) + 12, (uint32_t)((int32_t)pc_u32(ed_rec(b, i) + 12) + d * (int32_t)PIECE_EV));
    ed_w32(b + 40, (uint32_t)((int32_t)len + d * (int32_t)PIECE_EV));
}

/* rewrite channel ch's rests as encode() does (never more events than there were: it packs in place) */
static void ed_pack(uint8_t *b, uint32_t ch)
{
    uint32_t n = pc_u32(ed_rec(b, ch) + 8), off = pc_u32(ed_rec(b, ch) + 12), r, w = off, gap = 0;
    for (r = 0; r < n; r++) {
        uint8_t e[PIECE_EV], k;
        for (k = 0; k < PIECE_EV; k++) e[k] = b[off + r * PIECE_EV + k];
        gap += pc_u16(e);
        if (e[4] == PIECE_REST) continue;
        for (; gap > ED_GAP; gap -= ED_GAP, w += PIECE_EV) ed_ev(b + w, ED_GAP, 1, PIECE_REST, 1);
        ed_ev(b + w, gap, pc_u16(e + 2), e[4], e[5]);
        w += PIECE_EV;
        gap = 0;
    }
    if (w < off + n * PIECE_EV) ed_shift(b, ch, w, -(int32_t)((off + n * PIECE_EV - w) / PIECE_EV));
}

/* the byte offset of note (tick, pitch) in channel ch, or 0 */
static uint32_t ed_find(const uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch)
{
    uint32_t n = pc_u32(b + PIECE_HDR + ch * PIECE_CHREC + 8), off = pc_u32(b + PIECE_HDR + ch * PIECE_CHREC + 12), j, t = 0;
    for (j = 0; j < n; j++, off += PIECE_EV) {
        t += pc_u16(b + off);
        if (t > tick) break;
        if (t == tick && b[off + 4] == pitch) return off;
    }
    return 0;
}

static void ed_push(uint32_t op, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel)
{
    ed_op_t *o;
    ed_hold = 0;                                /* any change logged ends a tools hold */
    ed_top = (ed_top + 1u) % ED_UNDO;
    if (ed_n < ED_UNDO) ed_n++;
    o = &ed_log[ed_top];
    o->grp = ed_grp;
    o->op = (uint8_t)op; o->ch = (uint8_t)ch; o->tick = tick; o->pitch = (uint8_t)pitch; o->dur = (uint16_t)dur; o->vel = (uint8_t)vel;
}

static int ed_ok_note(uint32_t dur, uint32_t vel) { return dur >= 1u && dur <= 65535u && (vel & 127u) >= 1u && vel <= 255u; }

static uint32_t ed_rests(uint32_t gap) { return gap > ED_GAP ? (gap - 1u) / ED_GAP : 0u; }

/* the run of rests around the new note is rewritten whole: from after the last note before it to the first note
 * after it, so the room needed is exact (a gap the note splits may need fewer rests than before) */
static int ed_insert(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel, int log)
{
    uint32_t n, off, j, t = 0, rs = 0, p0 = 0, nx, tn, old, now, at, len = pc_u32(b + 40);
    uint8_t nd[4] = {0, 0, 0, 0};
    int32_t d;
    if (ch >= b[5] || tick >= pc_u32(b + 12) || pitch > 127u || !ed_ok_note(dur, vel)) return ED_ARG;
    if (ed_find(b, ch, tick, pitch)) return ED_EXISTS;
    n = pc_u32(ed_rec(b, ch) + 8);
    off = pc_u32(ed_rec(b, ch) + 12);
    for (j = 0; j < n; j++) {                   /* before the first event later than the note (tick, then pitch) */
        const uint8_t *e = b + off + j * PIECE_EV;
        uint32_t t2 = t + pc_u16(e);
        if (t2 > tick || (t2 == tick && e[4] != PIECE_REST && e[4] > pitch)) break;
        t = t2;
        if (e[4] != PIECE_REST) { rs = j + 1u; p0 = t; }
    }
    for (nx = j, tn = t; nx < n; nx++) {         /* the next note, past any rests */
        const uint8_t *e = b + off + nx * PIECE_EV;
        tn += pc_u16(e);
        if (e[4] != PIECE_REST) { nd[0] = e[2]; nd[1] = e[3]; nd[2] = e[4]; nd[3] = e[5]; break; }
    }
    old = nx - rs + (nx < n ? 1u : 0u);
    now = ed_rests(tick - p0) + 1u + (nx < n ? ed_rests(tn - tick) + 1u : 0u);
    d = (int32_t)now - (int32_t)old;
    if (d > 0 && len + (uint32_t)d * PIECE_EV > PIECE_MAX_BYTES) return ED_FULL;
    at = off + rs * PIECE_EV;
    if (d) ed_shift(b, ch, at, d);
    for (t = tick - p0; t > ED_GAP; t -= ED_GAP, at += PIECE_EV) ed_ev(b + at, ED_GAP, 1, PIECE_REST, 1);
    ed_ev(b + at, t, dur, pitch, vel);
    at += PIECE_EV;
    if (nx < n) {
        for (t = tn - tick; t > ED_GAP; t -= ED_GAP, at += PIECE_EV) ed_ev(b + at, ED_GAP, 1, PIECE_REST, 1);
        ed_ev(b + at, t, pc_u16(nd), nd[2], nd[3]);
    }
    ed_seal(b);
    if (log) ed_push(EO_DEL, ch, tick, pitch, 0, 0);
    return ED_OK;
}

static int ed_delete(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, int log)
{
    uint32_t at;
    uint8_t *e;
    if (ch >= b[5]) return ED_ARG;
    if (pitch > 127u || !(at = ed_find(b, ch, tick, pitch))) return ED_NONE;
    e = b + at;
    if (log) ed_push(EO_INS, ch, tick, pitch, pc_u16(e + 2), e[5]);
    ed_ev(e, pc_u16(e), 1, PIECE_REST, 1);      /* a rest keeps the time; ed_pack folds it into what follows */
    ed_pack(b, ch);
    ed_seal(b);
    return ED_OK;
}

static int ed_set(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel, int log)
{
    uint32_t at;
    if (ch >= b[5] || !ed_ok_note(dur, vel)) return ED_ARG;
    if (pitch > 127u || !(at = ed_find(b, ch, tick, pitch))) return ED_NONE;
    if (log) ed_push(EO_SET, ch, tick, pitch, pc_u16(b + at + 2), b[at + 5]);
    ed_w16(b + at + 2, dur);
    b[at + 5] = (uint8_t)vel;
    ed_seal(b);
    return ED_OK;
}

/* a channel record byte: PIECE_CH_STEPS (0..PIECE_MAX_STEPS), PIECE_CH_SWING (0..PIECE_MAX_SWING), PIECE_CH_GLIDE
 * (0..PIECE_MAX_GLIDE); or PIECE_CH_MODE, the channel's nibble of the header's voice modes (VM_*) */
static int ed_chan(uint8_t *b, uint32_t ch, uint32_t at, uint32_t v, int log)
{
    uint32_t sh = 4u * ch, m;
    if (ch >= b[5]) return ED_ARG;
    if (at == PIECE_CH_MODE) {
        if (v >= VM_N) return ED_ARG;
        m = pc_u32(b + PIECE_VOICES);
        if (log) ed_push(EO_CHAN, ch, at, piece_mode(b, ch), 0, 0);   /* (a corrupt one: undone to POLY, as it read) */
        ed_w32(b + PIECE_VOICES, (m & ~(15u << sh)) | v << sh);
        ed_seal(b);
        return ED_OK;
    }
    if (at != PIECE_CH_STEPS && at != PIECE_CH_SWING && at != PIECE_CH_GLIDE) return ED_ARG;
    if (v > (at == PIECE_CH_STEPS ? PIECE_MAX_STEPS : at == PIECE_CH_SWING ? PIECE_MAX_SWING : PIECE_MAX_GLIDE)) return ED_ARG;
    if (log) ed_push(EO_CHAN, ch, at, ed_rec(b, ch)[at], 0, 0);
    ed_rec(b, ch)[at] = (uint8_t)v;
    ed_seal(b);
    return ED_OK;
}

static int edit_insert(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel) { return ed_insert(b, ch, tick, pitch, dur, vel, 1); }
static int edit_delete(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch) { return ed_delete(b, ch, tick, pitch, 1); }
static int edit_set(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel) { return ed_set(b, ch, tick, pitch, dur, vel, 1); }
static int edit_chan(uint8_t *b, uint32_t ch, uint32_t at, uint32_t v) { return ed_chan(b, ch, at, v, 1); }

/* ---- a recording pass: one undo ---- */
static void edit_pass_begin(void) { ed_pnew = 1; }

static void ed_pass_add(uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel)
{
    uint32_t i;
    if (ed_pnew) {                              /* the pass's first change: the last pass leaves the history */
        for (i = 0; i < ed_n; i++)
            if (ed_log[(ed_top + ED_UNDO - i) % ED_UNDO].op >= EO_PASS) { ed_n = i; break; }   /* (a hold's too: same room) */
        ed_pnew = ed_pn = 0;
        edit_mark();
        ed_push(EO_PASS, ch, tick, 0, 0, 0);
    }
    ed_pass[ed_pn].tick = tick; ed_pass[ed_pn].dur = (uint16_t)dur; ed_pass[ed_pn].pitch = (uint8_t)pitch; ed_pass[ed_pn].vel = (uint8_t)vel;
    ed_pn++;
}

static int edit_pass_insert(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel)
{
    int r;
    if (!ed_pnew && ed_pn >= ED_PASSN) return ED_PASS;
    r = ed_insert(b, ch, tick, pitch, dur, vel, 0);
    if (r == ED_OK) ed_pass_add(ch, tick, pitch, 0, 0);
    return r;
}

static int edit_pass_delete(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch)
{
    uint32_t at, dur, vel;
    if (!ed_pnew && ed_pn >= ED_PASSN) return ED_PASS;
    if (ch >= b[5] || pitch > 127u || !(at = ed_find(b, ch, tick, pitch))) return ED_NONE;
    dur = pc_u16(b + at + 2);
    vel = b[at + 5];
    ed_delete(b, ch, tick, pitch, 0);
    ed_pass_add(ch, tick, pitch, dur, vel);
    return ED_OK;
}

static int ed_pass_undo(uint8_t *b, uint32_t ch)   /* the pass taken back, last change first */
{
    while (ed_pn) {
        const ed_pn_t *e = &ed_pass[ed_pn - 1u];
        int r = e->vel ? ed_insert(b, ch, e->tick, e->pitch, e->dur, e->vel, 0) : ed_delete(b, ch, e->tick, e->pitch, 0);
        if (r) return r;
        ed_pn--;
    }
    return ED_OK;
}

static int ed_snap_back(uint8_t *b, uint32_t ch, uint32_t n, uint32_t steps);
static int ed_replay(uint8_t *b, const ed_op_t *o)
{
    if (o->op == EO_SNAP) return ed_snap_back(b, o->ch, o->tick, o->pitch);
    if (o->op == EO_PASS) return ed_pass_undo(b, o->ch);
    if (o->op == EO_DEL) return ed_delete(b, o->ch, o->tick, o->pitch, 0);
    if (o->op == EO_INS) return ed_insert(b, o->ch, o->tick, o->pitch, o->dur, o->vel, 0);
    if (o->op == EO_CHAN) return ed_chan(b, o->ch, o->tick, o->pitch, 0);
    return ed_set(b, o->ch, o->tick, o->pitch, o->dur, o->vel, 0);
}

/* take back the last group of changes; ED_NONE when there is none. An entry that cannot be replayed stays logged
 * (and its error is returned). *ch / *tick say where the last one taken back was (for the screen) */
static int edit_undo(uint8_t *b, uint32_t *ch, uint32_t *tick)
{
    uint32_t g;
    ed_hold = 0;
    if (!ed_n) return ED_NONE;
    g = ed_log[ed_top].grp;
    while (ed_n && ed_log[ed_top].grp == g) {
        const ed_op_t *o = &ed_log[ed_top];
        int r = ed_replay(b, o);
        if (r) return r;
        *ch = o->ch;
        *tick = o->op == EO_SNAP ? PIECE_MAX_TICKS : o->tick;   /* (a hold: no one place) */
        ed_top = (ed_top + ED_UNDO - 1u) % ED_UNDO;
        ed_n--;
    }
    return ED_OK;
}

/* ---- part tools (US-31): SLOOP's EDIT knobs (ui_layers.c pattern_rotate / pattern_length / pattern_transpose) on a
 * channel's notes. The part: the channel's loop (US-25) when it has one, else the bars up to its last note's start
 * (kept for the hold); never past the piece's end. Each turn is all or nothing (the room is counted first). A hold
 * is one undo: its first change keeps the channel as it was in the pass log's room (only the last hold or pass is
 * kept, as passes) under one entry; the hold's later turns log nothing. Any other change ends the hold, as do
 * edit_tools_end() (the button let go) and edit_undo(). */
enum { ED_RANGE = 7, ED_DRUMS = 8, ED_EMPTY = 9, ED_LONG = 10, ED_SHORT = 11, ED_BIG = 12 };
enum { ET_SHIFT, ET_LEN, ET_TRANS };
#define ED_SNAPN (sizeof ed_pass / PIECE_EV)    /* the events a hold can keep (853) */
#define ED_STEP (PIECE_PPQ / 4u)                /* a loop step: 1/16 */
#define ED_BAR (PIECE_PPQ * 4u)

static void edit_tools_end(void) { ed_hold = 0; }

static void ed_mv(uint8_t *b, uint32_t to, uint32_t from, uint32_t n)   /* memmove, either way */
{
    uint32_t i;
    if (to > from) for (i = n; i-- > 0;) b[to + i] = b[from + i];
    else for (i = 0; i < n; i++) b[to + i] = b[from + i];
}

static void ed_rev(uint8_t *e, uint32_t n)      /* n events reversed in place */
{
    uint32_t i, k;
    for (i = 0; i < n / 2u; i++)
        for (k = 0; k < PIECE_EV; k++) {
            uint8_t x = e[i * PIECE_EV + k];
            e[i * PIECE_EV + k] = e[(n - 1u - i) * PIECE_EV + k];
            e[(n - 1u - i) * PIECE_EV + k] = x;
        }
}

/* the first of n events at or after tick cut; *tb = the tick before it */
static uint32_t ed_at(const uint8_t *e, uint32_t n, uint32_t cut, uint32_t *tb)
{
    uint32_t j, t = 0;
    for (j = 0; j < n && t + pc_u16(e + j * PIECE_EV) < cut; j++) t += pc_u16(e + j * PIECE_EV);
    *tb = t;
    return j;
}

/* the output as encode() writes it: rests of 65535 ticks first in a gap; w 0: only counted. lead: how far (events)
 * the writer gets past the first source event still needed (need) */
typedef struct { uint8_t *w; uint32_t n, last; int32_t lead; } ed_out_t;
static void ed_put(ed_out_t *o, uint32_t t, uint32_t dur, uint32_t pitch, uint32_t vel, uint32_t need)
{
    uint32_t gap = t - o->last;
    for (; gap > ED_GAP; gap -= ED_GAP, o->n++) if (o->w) ed_ev(o->w + o->n * PIECE_EV, ED_GAP, 1, PIECE_REST, 1);
    if (o->w) ed_ev(o->w + o->n * PIECE_EV, gap, dur, pitch, vel);
    o->n++;
    o->last = t;
    if ((int32_t)o->n - (int32_t)need > o->lead) o->lead = (int32_t)o->n - (int32_t)need;
}

/* a run of events [i, n) of p, tb the tick before i: its notes out at tick + add, those before end. ws: where its
 * first event lies in the source as written (the source events before it all read by then); fix: the run is read
 * from the output, the source needed from ws on throughout */
typedef struct { const uint8_t *p; uint32_t i, n, tb; int32_t add; uint32_t ws, fix; } ed_blk_t;
static void ed_emit(ed_out_t *o, const ed_blk_t *k, uint32_t end)
{
    uint32_t j, t = k->tb;
    for (j = k->i; j < k->n; j++) {
        const uint8_t *e = k->p + j * PIECE_EV;
        uint32_t dur = pc_u16(e + 2), pitch = e[4], vel = e[5], ot;   /* (read before the writer may reach it) */
        t += pc_u16(e);
        if (pitch == PIECE_REST) continue;
        ot = (uint32_t)((int32_t)t + k->add);
        if (ot < end) ed_put(o, ot, dur, pitch, vel, k->fix ? k->ws : k->ws + (j - k->i) + 1u);
    }
}

/* a hold's first change: the channel kept, one history entry (a group of its own) */
static int ed_hold_begin(uint8_t *b, uint32_t ch)
{
    const uint8_t *rec = ed_rec(b, ch);
    uint32_t n = pc_u32(rec + 8), off = pc_u32(rec + 12), i;
    uint8_t *s = (uint8_t *)ed_pass;
    if (ed_hold && ed_hch == ch) return ED_OK;
    if (n > ED_SNAPN) return ED_BIG;
    for (i = 0; i < ed_n; i++)                  /* the room is the pass log's: the last pass or hold leaves the history */
        if (ed_log[(ed_top + ED_UNDO - i) % ED_UNDO].op >= EO_PASS) { ed_n = i; break; }
    ed_pn = 0;
    for (i = 0; i < n * PIECE_EV; i++) s[i] = b[off + i];
    edit_mark();
    ed_push(EO_SNAP, ch, n, rec[PIECE_CH_STEPS], 0, 0);
    ed_hold = 1;
    ed_hch = ch;
    return ED_OK;
}

static int ed_snap_back(uint8_t *b, uint32_t ch, uint32_t n, uint32_t steps)
{
    const uint8_t *s = (const uint8_t *)ed_pass;
    uint32_t m, off, i;
    if (ch >= b[5] || n > ED_SNAPN) return ED_ARG;
    m = pc_u32(ed_rec(b, ch) + 8);
    off = pc_u32(ed_rec(b, ch) + 12);
    if (n > m && pc_u32(b + 40) + (n - m) * PIECE_EV > PIECE_MAX_BYTES) return ED_FULL;
    if (n != m) ed_shift(b, ch, off, (int32_t)n - (int32_t)m);
    for (i = 0; i < n * PIECE_EV; i++) b[off + i] = s[i];
    ed_rec(b, ch)[PIECE_CH_STEPS] = (uint8_t)steps;
    ed_seal(b);
    return ED_OK;
}

/* tool ET_SHIFT: every note of the part d grid steps (g ticks) later / earlier, wrapping in the part; ET_LEN: d > 0
 * x2 (the part copied after itself: a loop up to 64 steps, else up to the piece's end), d < 0 1/2 (the first half
 * kept); ET_TRANS: every note of the channel d semitones (none may leave 0..127; not drums: kit sounds). ED_OK, or
 * ED_EMPTY / ED_DRUMS / ED_RANGE / ED_LONG / ED_SHORT / ED_FULL / ED_BIG / ED_ARG with nothing changed */
static int edit_tool(uint8_t *b, uint32_t ch, uint32_t tool, int32_t d, uint32_t g)
{
    uint8_t *rec, *src;
    uint32_t n, off, len, end, steps, nsteps, L, nl, i, t = 0, last = 0, notes = 0, j = 0, h = 0, m, tj = 0, th = 0, top;
    ed_blk_t bk[3];
    ed_out_t o = {0, 0, 0, 0};
    int r;
    if (ch >= b[5] || !d || tool > ET_TRANS || (tool == ET_SHIFT && !g)) return ED_ARG;
    rec = ed_rec(b, ch);
    n = pc_u32(rec + 8);
    off = pc_u32(rec + 12);
    len = pc_u32(b + 40);
    end = pc_u32(b + 12);
    steps = nsteps = rec[PIECE_CH_STEPS];
    for (i = 0; i < n; i++) {                   /* the last note's start */
        t += pc_u16(b + off + i * PIECE_EV);
        if (b[off + i * PIECE_EV + 4] != PIECE_REST) { last = t; notes++; }
    }
    if (!notes) return ED_EMPTY;
    L = steps ? steps * ED_STEP : ed_hold && ed_hch == ch ? ed_hl : (last / ED_BAR + 1u) * ED_BAR;
    if (L > end) L = end;
    nl = L;
    if (tool == ET_TRANS) {
        if (rec[0] == ROLE_DRUMS || rec[0] == ROLE_PERC) return ED_DRUMS;
        for (i = 0; i < n; i++) {
            int32_t p = b[off + i * PIECE_EV + 4];
            if (p != PIECE_REST && (p + d < 0 || p + d > 127)) return ED_RANGE;
        }
        if ((r = ed_hold_begin(b, ch))) return r;
        for (i = 0; i < n; i++)                 /* (all move alike: the order stays as encode() keeps it) */
            if (b[off + i * PIECE_EV + 4] != PIECE_REST) b[off + i * PIECE_EV + 4] = (uint8_t)(b[off + i * PIECE_EV + 4] + d);
        ed_hl = nl;
        ed_seal(b);
        return ED_OK;
    }
    src = b + off;
    if (tool == ET_SHIFT) {                     /* out: the notes from c on (wrapped to 0), those before c, the rest */
        uint32_t a = (uint32_t)(d < 0 ? -d : d) * g % L, s = d < 0 ? (L - a) % L : a, c = L - s;
        if (!s) return ED_OK;
        j = ed_at(src, n, c, &tj);
        h = ed_at(src, n, L, &th);
        bk[0] = (ed_blk_t){src, j, h, tj, -(int32_t)c, 0, 0};
        bk[1] = (ed_blk_t){src, 0, j, 0, (int32_t)s, h - j, 0};
        bk[2] = (ed_blk_t){src, h, n, th, 0, h, 0};
    } else if (d > 0) {                         /* x2: the part, its copy (before the end), what lies past twice it */
        if (steps ? steps * 2u > PIECE_MAX_STEPS : L >= end) return ED_LONG;
        h = ed_at(src, n, L, &th);
        m = ed_at(src, n, 2u * L, &th);
        bk[0] = (ed_blk_t){src, 0, h, 0, 0, 0, 0};
        bk[1] = (ed_blk_t){src, 0, h, 0, (int32_t)L, m, 1};
        bk[2] = (ed_blk_t){src, m, n, th, 0, m, 0};
        nsteps = steps * 2u;
        nl = 2u * L < end ? 2u * L : end;
    } else {                                    /* 1/2: the first half, what lies past the part */
        uint32_t half = steps ? steps / 2u * ED_STEP : L / 2u;
        if (steps ? steps < 2u : half < ED_STEP) return ED_SHORT;
        j = ed_at(src, n, half, &tj);
        h = ed_at(src, n, L, &th);
        bk[0] = (ed_blk_t){src, 0, j, 0, 0, 0, 0};
        bk[1] = (ed_blk_t){src, h, n, th, 0, h, 0};
        bk[2] = (ed_blk_t){src, n, n, 0, 0, n, 0};
        nsteps = steps / 2u;
        nl = half;
    }
    for (i = 0; i < 3u; i++) ed_emit(&o, &bk[i], end);   /* counted first: all or nothing */
    if (len + o.n * PIECE_EV > PIECE_MAX_BYTES + n * PIECE_EV || (o.lead > 0 && len + (uint32_t)o.lead * PIECE_EV > PIECE_MAX_BYTES))
        return ED_FULL;                         /* (the result, and the room the writer needs ahead of the reader) */
    if ((r = ed_hold_begin(b, ch))) return r;
    top = PIECE_MAX_BYTES - (len - off);        /* the channel and all after it to the top; written again from off */
    ed_mv(b, top, off, len - off);
    src = b + top;
    for (i = 0; i < 3u; i++) bk[i].p = src;
    if (tool == ET_SHIFT) {                     /* the source in the order it goes out (each run keeps its ticks) */
        ed_rev(src, j);
        ed_rev(src + j * PIECE_EV, h - j);
        ed_rev(src, h);
        bk[0].i = 0; bk[0].n = h - j;
        bk[1].i = h - j; bk[1].n = h;
    }
    o = (ed_out_t){b + off, 0, 0, 0};
    ed_emit(&o, &bk[0], end);
    if (tool == ET_LEN && d > 0) { bk[1].p = b + off; bk[1].n = o.n; }   /* the copy: read from what went out */
    ed_emit(&o, &bk[1], end);
    ed_emit(&o, &bk[2], end);
    ed_mv(b, off + o.n * PIECE_EV, top + n * PIECE_EV, len - off - n * PIECE_EV);
    ed_w32(rec + 8, o.n);
    for (i = ch + 1u; i < b[5]; i++) ed_w32(ed_rec(b, i) + 12, pc_u32(ed_rec(b, i) + 12) + o.n * PIECE_EV - n * PIECE_EV);
    ed_w32(b + 40, len + o.n * PIECE_EV - n * PIECE_EV);
    rec[PIECE_CH_STEPS] = (uint8_t)nsteps;
    ed_hl = nl;
    ed_seal(b);
    return ED_OK;
}
