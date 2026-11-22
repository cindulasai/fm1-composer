/* SPDX-License-Identifier: GPL-3.0-only */
/* edit.c: changes a checked piece (piece.h) in place, in a buffer of PIECE_MAX_BYTES. A note is named by its channel,
 * start tick and pitch. After a call that returns ED_OK the piece passes piece_check() and is byte for byte what
 * fm1c's encode() makes of the same notes (in a gap, rests of 65535 ticks first; notes in tick, pitch order);
 * after an error the bytes are untouched. Each change is logged in the current group (edit_mark() starts one: a
 * key, a knob turn); edit_undo() takes back the last group, of the last ED_UNDO changes kept.
 * Edit only while the player is stopped (the bytes move), then player_refresh(). */
#define ED_UNDO 64u
#define ED_GAP 65535u
enum { ED_OK = 0, ED_ARG = 1, ED_FULL = 2, ED_EXISTS = 3, ED_NONE = 4 };
enum { EO_INS = 1, EO_DEL = 2, EO_SET = 3 };

typedef struct { uint32_t tick, grp; uint16_t dur; uint8_t op, ch, pitch, vel; } ed_op_t;
static ed_op_t ed_log[ED_UNDO];
static uint32_t ed_top, ed_n, ed_grp;           /* a ring: ed_n entries ending at ed_top; the group now */

static void ed_w16(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); }
static void ed_w32(uint8_t *p, uint32_t v) { ed_w16(p, v & 0xFFFFu); ed_w16(p + 2, v >> 16); }
static uint8_t *ed_rec(uint8_t *b, uint32_t ch) { return b + PIECE_HDR + ch * PIECE_CHREC; }
static void ed_ev(uint8_t *e, uint32_t delta, uint32_t dur, uint32_t pitch, uint32_t vel)
{
    ed_w16(e, delta); ed_w16(e + 2, dur); e[4] = (uint8_t)pitch; e[5] = (uint8_t)vel;
}
static void ed_seal(uint8_t *b) { uint32_t n = pc_u32(b + 40); ed_w32(b + n - 4u, piece_crc32(b, n - 4u)); }

static void edit_forget(void) { ed_n = 0; }     /* a new piece: nothing to undo */
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
    ed_top = (ed_top + 1u) % ED_UNDO;
    if (ed_n < ED_UNDO) ed_n++;
    o = &ed_log[ed_top];
    o->grp = ed_grp;
    o->op = (uint8_t)op; o->ch = (uint8_t)ch; o->tick = tick; o->pitch = (uint8_t)pitch; o->dur = (uint16_t)dur; o->vel = (uint8_t)vel;
}

static int ed_ok_note(uint32_t dur, uint32_t vel) { return dur >= 1u && dur <= 65535u && vel >= 1u && vel <= 127u; }

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

static int edit_insert(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel) { return ed_insert(b, ch, tick, pitch, dur, vel, 1); }
static int edit_delete(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch) { return ed_delete(b, ch, tick, pitch, 1); }
static int edit_set(uint8_t *b, uint32_t ch, uint32_t tick, uint32_t pitch, uint32_t dur, uint32_t vel) { return ed_set(b, ch, tick, pitch, dur, vel, 1); }

static int ed_replay(uint8_t *b, const ed_op_t *o)
{
    if (o->op == EO_DEL) return ed_delete(b, o->ch, o->tick, o->pitch, 0);
    if (o->op == EO_INS) return ed_insert(b, o->ch, o->tick, o->pitch, o->dur, o->vel, 0);
    return ed_set(b, o->ch, o->tick, o->pitch, o->dur, o->vel, 0);
}

/* take back the last group of changes; ED_NONE when there is none. An entry that cannot be replayed stays logged
 * (and its error is returned). *ch / *tick say where the last one taken back was (for the screen) */
static int edit_undo(uint8_t *b, uint32_t *ch, uint32_t *tick)
{
    uint32_t g;
    if (!ed_n) return ED_NONE;
    g = ed_log[ed_top].grp;
    while (ed_n && ed_log[ed_top].grp == g) {
        const ed_op_t *o = &ed_log[ed_top];
        int r = ed_replay(b, o);
        if (r) return r;
        *ch = o->ch;
        *tick = o->tick;
        ed_top = (ed_top + ED_UNDO - 1u) % ED_UNDO;
        ed_n--;
    }
    return ED_OK;
}
