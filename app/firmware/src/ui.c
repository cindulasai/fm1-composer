/* SPDX-License-Identifier: GPL-3.0-only */
/* ui.c: COMPOSER's screens, drawn only where something changed (240 x 240, FONT_S rows).
 *   LIBRARY  the pieces, newest first; SELECT moves, PLAY opens one, playing it
 *   PLAYER   the piece: name, tempo, bar:beat (in a section: its name and the bar:beat in it), then a strip per channel
 *            (role, sound, M / S); a piece with sections: the sections strip under them (playing amber, queued white)
 *   EDIT     one channel as a piano roll: 16 grid steps across, the 27 note keys' pitches down, the cursor's step lit
 *   STEPS    the 8 channels x 16 steps of a page (a note starts amber, sounds on grey, the playing step blue, the cursor
 *            framed white), then the cursor's step: its notes, length, velocity, the channel's loop and swing */
#define C_ACC RGB(255, 165, 0)                  /* amber */
#define C_REC RGB(255, 48, 48)                  /* REC's red */
#define C_GREY RGB(120, 120, 120)
#define C_MUTE RGB(70, 70, 70)
#define LIB_ROWS 9u
enum { D_ALL = 1, D_LIST = 2, D_HEAD = 4, D_POS = 8, D_FOOT = 16, D_STRIPS = 32, D_ROLL = 64, D_SECT = 128 };
#define C_ROW RGB(24, 24, 24)                   /* the roll: black-key rows, beat lines, the cursor's step */
#define C_BEAT RGB(48, 48, 48)
#define C_CUR RGB(60, 50, 20)
#define ROLL_X 24u
#define ROLL_Y 26u
#define ROLL_W 13u                              /* a step */
#define ROLL_H 6u                               /* a pitch */
#define ROLL_STEPS 16u
#define ROLL_KEYS 27u                           /* F3 .. G5, as the note keys */

/* the formats the screens need, %s %u %c (the device has no printf) */
static void ui_fmt(char *b, uint32_t max, const char *f, ...)
{
    __builtin_va_list ap;
    uint32_t o = 0;
    __builtin_va_start(ap, f);
    for (; *f && o + 1u < max; f++) {
        if (*f != '%' || !f[1]) { b[o++] = *f; continue; }
        f++;
        if (*f == 's') {
            const char *s = __builtin_va_arg(ap, const char *);
            while (*s && o + 1u < max) b[o++] = *s++;
        } else if (*f == 'u') {
            char d[10];
            uint32_t v = __builtin_va_arg(ap, uint32_t), k = 0;
            do { d[k++] = (char)('0' + v % 10u); v /= 10u; } while (v);
            while (k && o + 1u < max) b[o++] = d[--k];
        } else if (*f == 'c') {
            b[o++] = (char)__builtin_va_arg(ap, int);
        } else {
            b[o++] = *f;
        }
    }
    b[o] = 0;
    __builtin_va_end(ap);
}

static const char *const ROLE_NAME[ROLE_COUNT] = {"LEAD", "COUNTER", "CHORDS", "PAD", "BASS", "DRUMS", "PERC", "ARP", "DRONE", "FX", "OTHER"};

/* every line of text goes through here; host builds log it, so tests read the screen as text */
static void ui_text(uint32_t x, uint32_t y, uint32_t w, const felucca_font_t *f, const char *s, uint16_t c, int align)
{
    draw_text_box(x, y, w, f, s, c, align);
#ifdef DINGHY_HOST
    printf("%6u ms  SCREEN %u %u %04X %s\n", fm1_ms, y, x, c, s);
#endif
}

static void ui_library(uint32_t dirty, uint32_t cur, uint32_t *top)
{
    char b[40];
    uint32_t i, n = lib_count();
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD | D_LIST)) {
        ui_text(8, 4, 120, &FONT_S, APP_NAME, C_ACC, 0);
        ui_fmt(b, sizeof b, "%u PIECE%s", n, n == 1u ? "" : "S");
        ui_text(128, 4, 104, &FONT_S, n ? b : "", C_GREY, 2);
    }
    if (dirty & (D_ALL | D_LIST)) {
        if (cur < *top) *top = cur;
        if (cur >= *top + LIB_ROWS) *top = cur - LIB_ROWS + 1u;
        if (!n) {
            lcd_fill(0, 28, 240, LIB_ROWS * 20u, C_BLACK);
            ui_text(0, 112, 240, &FONT_S, "LIBRARY EMPTY", C_GREY, 1);
            ui_text(0, 136, 240, &FONT_S, "push pieces with fm1c", C_GREY, 1);
        } else {
            for (i = 0; i < LIB_ROWS; i++) {
                const lib_entry_t *e = lib_entry(*top + i);
                uint32_t y = 28u + i * 20u;
                if (!e) { lcd_fill(0, y, 240, 20, C_BLACK); continue; }
                ui_fmt(b, sizeof b, "%s%s", *top + i == cur ? "> " : "  ", e->name[0] ? e->name : "untitled");
                ui_text(4, y, 190, &FONT_S, b, *top + i == cur ? C_ACC : C_WHITE, 0);
                ui_fmt(b, sizeof b, "%uK", (e->len + 1023u) / 1024u);
                ui_text(194, y, 40, &FONT_S, b, C_GREY, 2);
            }
        }
    }
    if (dirty & (D_ALL | D_LIST | D_FOOT)) {
        ui_fmt(b, sizeof b, "%u KB FREE", lib_free() / 1024u);
        ui_text(0, 220, 240, &FONT_S, b, C_GREY, 1);
    }
}

typedef struct {
    const char *name, *status;
    uint32_t nch, sel, mute, solo, tempo_x10, pos;
    const uint8_t *role, *sound;
    const piece_view_t *v;                      /* its sections */
    uint32_t sec, queued, sloop;                /* playing, queued (PL_NONE: none), section loop on */
    uint32_t rec;                               /* REC: 0 off, 1 counting in, 2 recording */
} ui_player_t;

/* the sections strip: from the one before the playing one, as many as fit: "N NAME"; LOOP first when it is on */
static void ui_sections(const ui_player_t *p, uint32_t y)
{
    char b[16], n[PIECE_SNAME + 1];
    uint32_t i = p->sec != PL_NONE && p->sec ? p->sec - 1u : 0u, x = 0;
    lcd_fill(0, y, 240, 18, C_BLACK);
    if (p->sloop) { ui_text(0, y, 44, &FONT_S, "LOOP", C_ACC, 0); x = 48; }
    for (; i < p->v->nsect; i++) {
        uint32_t w;
        piece_sect_name(p->v, i, n);
        ui_fmt(b, sizeof b, "%u %s", i + 1u, n);
        w = (uint32_t)text_w(&FONT_S, b);
        if (x + w > 240u) break;
        ui_text(x, y, w, &FONT_S, b, i == p->sec ? C_ACC : i == p->queued ? C_WHITE : C_GREY, 0);
        x += w + 10u;
    }
}

static void ui_player(uint32_t dirty, const ui_player_t *p)
{
    char b[40];
    uint32_t ch, pitch = p->v->nsect ? 17u : 19u;   /* (with sections, the strips make room for theirs) */
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD)) {
        ui_text(8, 4, 150, &FONT_S, p->name, C_ACC, 0);
        if (p->tempo_x10 % 10u) ui_fmt(b, sizeof b, "%u.%u BPM", p->tempo_x10 / 10u, p->tempo_x10 % 10u);
        else ui_fmt(b, sizeof b, "%u BPM", p->tempo_x10 / 10u);
        ui_text(158, 4, 76, &FONT_S, b, C_WHITE, 2);
    }
    if (dirty & (D_ALL | D_POS | D_SECT)) {
        if (p->sec != PL_NONE && p->sec < p->v->nsect) {
            char n[PIECE_SNAME + 1];
            uint32_t t = p->pos - piece_sect_start(p->v, p->sec);
            piece_sect_name(p->v, p->sec, n);
            ui_fmt(b, sizeof b, "%s %u:%u", n, t / (4u * PIECE_PPQ) + 1u, t / PIECE_PPQ % 4u + 1u);
        } else {
            ui_fmt(b, sizeof b, "%u:%u", p->pos / (4u * PIECE_PPQ) + 1u, p->pos / PIECE_PPQ % 4u + 1u);
        }
        ui_text(0, 26, 240, &FONT_L, b, C_WHITE, 1);
        if (p->rec) ui_text(4, 30, 36, &FONT_S, p->rec == 1u ? "IN" : "REC", C_REC, 0);
    }
    if (dirty & (D_ALL | D_STRIPS)) {
        for (ch = 0; ch < PIECE_MAX_CH; ch++) {
            uint32_t y = 64u + ch * pitch;
            int heard = p->solo ? (int)(p->solo >> ch & 1u) : !(p->mute >> ch & 1u);
            uint16_t c = ch == p->sel ? C_ACC : heard ? C_WHITE : C_MUTE;
            if (ch >= p->nch) { lcd_fill(0, y, 240, pitch, C_BLACK); continue; }
            ui_fmt(b, sizeof b, "%s%u", ch == p->sel ? ">" : " ", ch + 1u);
            ui_text(0, y, 24, &FONT_S, b, c, 0);
            ui_text(26, y, 70, &FONT_S, ROLE_NAME[p->role[ch]], c, 0);
            ui_text(98, y, 108, &FONT_S, sound_name(p->sound[ch]), c, 0);
            ui_text(206, y, 30, &FONT_S, p->solo >> ch & 1u ? "S" : p->mute >> ch & 1u ? "M" : "", c, 2);
        }
    }
    if (p->v->nsect && (dirty & (D_ALL | D_SECT))) ui_sections(p, 202);
    if (dirty & (D_ALL | D_FOOT)) ui_text(0, 222, 240, &FONT_S, p->status, C_GREY, 1);
}

/* FX: one channel's effects, a row each; the picked row in amber */
typedef struct {
    const char *status;
    uint32_t ch, role, sound, row;
    const uint8_t *v;
} ui_fx_t;
#define UI_FX 6u                                /* rows: fx.c FX_N */
static const char *const FX_NAME[UI_FX] = {"REVERB", "DELAY", "CHORUS", "FILTER", "RESO", "DRIVE"};

static void ui_fx(uint32_t dirty, const ui_fx_t *f)
{
    char b[40];
    uint32_t k;
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD)) {
        ui_fmt(b, sizeof b, "FX %u %s", f->ch + 1u, ROLE_NAME[f->role < ROLE_COUNT ? f->role : ROLE_OTHER]);
        ui_text(8, 4, 120, &FONT_S, b, C_ACC, 0);
        ui_text(128, 4, 106, &FONT_S, sound_name(f->sound), C_WHITE, 2);
    }
    if (dirty & (D_ALL | D_STRIPS)) {
        for (k = 0; k < UI_FX; k++) {
            uint32_t y = 40u + k * 28u;
            uint16_t c = k == f->row ? C_ACC : C_WHITE;
            ui_fmt(b, sizeof b, "%s%s", k == f->row ? ">" : " ", FX_NAME[k]);
            ui_text(16, y, 130, &FONT_S, b, c, 0);
            ui_fmt(b, sizeof b, "%u", f->v[k]);
            ui_text(150, y, 74, &FONT_S, b, c, 2);
        }
    }
    if (dirty & (D_ALL | D_FOOT)) ui_text(0, 222, 240, &FONT_S, f->status, C_GREY, 1);
}

static const char *const NOTE_NAME[12] = {"C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"};
static const char *const GRID_NAME[4] = {"1/32", "1/16", "1/8", "1/4"};

typedef struct {
    const char *status;
    const uint8_t *piece;                       /* checked (piece_view) */
    uint32_t ch, role, tick, grid, gi, low;     /* cursor tick, grid ticks + its name's index, the lowest row's pitch */
    uint32_t len, vel, has, pitch;              /* the note under the cursor (has) or the next new one's length, velocity */
    uint32_t edited;
} ui_edit_t;

/* note length in grid names where it is one, else in ticks */
static void ui_len(char *b, uint32_t max, uint32_t len)
{
    static const uint16_t T[6] = {12, 24, 48, 96, 192, 384};
    static const char *const N[6] = {"1/32", "1/16", "1/8", "1/4", "1/2", "1/1"};
    uint32_t i;
    for (i = 0; i < 6u; i++) if (len == T[i]) { ui_fmt(b, max, "%s", N[i]); return; }
    ui_fmt(b, max, "%uT", len);
}

static void ui_edit(uint32_t dirty, const ui_edit_t *e)
{
    char b[48], l[12];
    uint32_t page = e->tick - e->tick % (ROLL_STEPS * e->grid), i;
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD | D_ROLL)) {
        ui_fmt(b, sizeof b, "EDIT %u %s%s", e->ch + 1u, ROLE_NAME[e->role], e->edited ? " *" : "");
        ui_text(8, 4, 130, &FONT_S, b, C_ACC, 0);
        ui_fmt(b, sizeof b, "BAR %u  %s", e->tick / (4u * PIECE_PPQ) + 1u, GRID_NAME[e->gi]);
        ui_text(138, 4, 96, &FONT_S, b, C_WHITE, 2);
    }
    if (dirty & (D_ALL | D_ROLL)) {
        piece_view_t v;
        piece_cur_t c;
        piece_note_t n;
        uint32_t end = page + ROLL_STEPS * e->grid;
        for (i = 0; i < ROLL_KEYS; i++) {       /* rows, top = highest */
            uint32_t p = e->low + ROLL_KEYS - 1u - i, k = p % 12u;
            int black = k == 1u || k == 3u || k == 6u || k == 8u || k == 10u;
            lcd_fill(ROLL_X, ROLL_Y + i * ROLL_H, ROLL_STEPS * ROLL_W, ROLL_H, black ? C_ROW : C_BLACK);
            lcd_fill(0, ROLL_Y + i * ROLL_H, ROLL_X, ROLL_H, k == 0u ? C_GREY : C_BLACK);   /* C rows marked at the left */
        }
        for (i = 0; i < ROLL_STEPS; i++) {
            uint32_t t = page + i * e->grid;
            if (t % PIECE_PPQ == 0u) lcd_fill(ROLL_X + i * ROLL_W, ROLL_Y, 1, ROLL_KEYS * ROLL_H, C_BEAT);
        }
        lcd_fill(ROLL_X + (e->tick - page) / e->grid * ROLL_W, ROLL_Y, ROLL_W, ROLL_KEYS * ROLL_H, C_CUR);
        piece_view(e->piece, &v);
        piece_cursor(&v.ch[e->ch], &c);
        while (piece_next(&c, &n) && n.tick < end) {
            uint32_t s0, s1;
            if (n.tick + n.dur <= page || n.pitch < e->low || n.pitch >= e->low + ROLL_KEYS) continue;
            s0 = n.tick > page ? (n.tick - page) / e->grid : 0u;
            s1 = (n.tick + n.dur - 1u < end ? n.tick + n.dur - 1u - page : end - 1u - page) / e->grid;
            lcd_fill(ROLL_X + s0 * ROLL_W + 1u, ROLL_Y + (e->low + ROLL_KEYS - 1u - n.pitch) * ROLL_H + 1u,
                     (s1 - s0 + 1u) * ROLL_W - 1u, ROLL_H - 1u, n.tick >= page ? C_ACC : C_GREY);
        }
    }
    if (dirty & (D_ALL | D_ROLL | D_FOOT)) {
        ui_len(l, sizeof l, e->len);
        if (e->has) ui_fmt(b, sizeof b, "%s%u  LEN %s  VEL %u", NOTE_NAME[e->pitch % 12u], e->pitch / 12u - 1u, l, e->vel);
        else ui_fmt(b, sizeof b, "NEW  LEN %s  VEL %u", l, e->vel);
        ui_text(0, 196, 240, &FONT_S, b, C_WHITE, 1);
        ui_text(0, 222, 240, &FONT_S, e->status, C_GREY, 1);
    }
}

#define ST_X 24u                                /* STEPS: the grid */
#define ST_Y 24u
#define ST_W 13u
#define ST_H 17u
#define C_PLAY RGB(80, 160, 255)

typedef struct {
    const char *status;
    const uint8_t *piece, *role;                /* checked (piece_view) */
    const uint32_t *at;                         /* each channel's own tick playing (0xFFFFFFFF: stopped) */
    const piece_note_t *notes;                  /* the cursor step's first notes (n of them, ST_CHORD at most shown) */
    uint32_t ch, base, step, n, len, vel, edited;
} ui_steps_t;

static void ui_steps(uint32_t dirty, const ui_steps_t *s)
{
    char b[48];
    uint32_t page = s->step / ST_PAGE, t0 = s->base + page * ST_PAGE * ST_TICKS, ch, i, o;
    piece_view_t v;
    piece_view(s->piece, &v);
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD | D_ROLL)) {
        ui_fmt(b, sizeof b, "STEPS %u %s%s", s->ch + 1u, ROLE_NAME[s->role[s->ch]], s->edited ? " *" : "");
        ui_text(8, 4, 150, &FONT_S, b, C_ACC, 0);
        ui_fmt(b, sizeof b, "%u-%u", page * ST_PAGE + 1u, page * ST_PAGE + ST_PAGE);
        ui_text(158, 4, 76, &FONT_S, b, C_WHITE, 2);
        for (ch = 0; ch < PIECE_MAX_CH; ch++) {
            ui_fmt(b, sizeof b, "%u", ch + 1u);
            ui_text(0, ST_Y + ch * ST_H, 22, &FONT_S, b, ch == s->ch ? C_ACC : ch < v.nch ? C_WHITE : C_MUTE, 0);
        }
    }
    if (dirty & (D_ALL | D_ROLL | D_POS)) {
        uint32_t x = ST_X + s->step % ST_PAGE * ST_W, y = ST_Y + s->ch * ST_H;
        lcd_fill(ST_X, ST_Y, ST_PAGE * ST_W + 1u, PIECE_MAX_CH * ST_H + 1u, C_BLACK);
        for (ch = 0; ch < PIECE_MAX_CH; ch++) {
            uint8_t cell[ST_PAGE];
            steps_row(&v, ch, t0, cell);
            for (i = 0; i < ST_PAGE; i++) {
                uint32_t t = t0 + i * ST_TICKS;
                uint16_t c = ch >= v.nch || t >= v.len_ticks ? C_BLACK : cell[i] == 2u ? C_ACC : cell[i] ? C_GREY : i % 4u ? C_ROW : C_BEAT;
                if (c != C_BLACK && s->at[ch] >= t && s->at[ch] < t + ST_TICKS) c = C_PLAY;
                lcd_fill(ST_X + i * ST_W + 1u, ST_Y + ch * ST_H + 1u, ST_W - 1u, ST_H - 1u, c);
            }
        }
        lcd_fill(x, y, ST_W + 1u, 1, C_WHITE);  /* the cursor */
        lcd_fill(x, y + ST_H, ST_W + 1u, 1, C_WHITE);
        lcd_fill(x, y, 1, ST_H, C_WHITE);
        lcd_fill(x + ST_W, y, 1, ST_H, C_WHITE);
        if (s->at[s->ch] >= s->base && s->at[s->ch] < s->base + PIECE_MAX_STEPS * ST_TICKS)
            ui_fmt(b, sizeof b, "PLAY %u", (s->at[s->ch] - s->base) / ST_TICKS + 1u);
        else
            b[0] = 0;
        ui_text(160, 202, 76, &FONT_S, b, C_PLAY, 2);
    }
    if (dirty & (D_ALL | D_ROLL | D_FOOT)) {
        const uint8_t *rec = s->piece + PIECE_HDR + s->ch * PIECE_CHREC;
        ui_fmt(b, sizeof b, "STEP %u ", s->step + 1u);
        for (i = 0; i < s->n && i < ST_CHORD; i++) {
            for (o = 0; b[o]; o++) {}
            ui_fmt(b + o, sizeof b - o, " %s%u", NOTE_NAME[s->notes[i].pitch % 12u], s->notes[i].pitch / 12u - 1u);
        }
        for (o = 0; b[o]; o++) {}
        ui_fmt(b + o, sizeof b - o, "%s", !s->n ? " -" : s->n > ST_CHORD ? " +" : "");
        ui_text(0, 166, 240, &FONT_S, b, C_WHITE, 1);
        ui_fmt(b, sizeof b, "LEN %u  VEL %u", s->len, s->vel);
        ui_text(0, 184, 240, &FONT_S, b, C_WHITE, 1);
        if (rec[PIECE_CH_STEPS]) ui_fmt(b, sizeof b, "LOOP %u  SWING %u", rec[PIECE_CH_STEPS], rec[PIECE_CH_SWING]);
        else ui_fmt(b, sizeof b, "LOOP OFF  SWING %u", rec[PIECE_CH_SWING]);
        ui_text(0, 202, 156, &FONT_S, b, C_GREY, 0);
    }
    if (dirty & (D_ALL | D_FOOT)) ui_text(0, 222, 240, &FONT_S, s->status, C_GREY, 1);
}
