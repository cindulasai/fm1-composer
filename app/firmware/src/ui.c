/* SPDX-License-Identifier: GPL-3.0-only */
/* ui.c: COMPOSER's screens, drawn only where something changed (240 x 240, FONT_S rows).
 *   LIBRARY  the pieces, newest first; SELECT moves, PLAY opens one, playing it
 *   PLAYER   the piece: name, tempo, bar:beat, then a strip per channel (role, sound, M / S)
 *   EDIT     one channel as a piano roll: 16 grid steps across, the 27 note keys' pitches down, the cursor's step lit */
#define C_ACC RGB(255, 165, 0)                  /* amber */
#define C_GREY RGB(120, 120, 120)
#define C_MUTE RGB(70, 70, 70)
#define LIB_ROWS 9u
enum { D_ALL = 1, D_LIST = 2, D_HEAD = 4, D_POS = 8, D_FOOT = 16, D_STRIPS = 32, D_ROLL = 64 };
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
} ui_player_t;

static void ui_player(uint32_t dirty, const ui_player_t *p)
{
    char b[40];
    uint32_t ch;
    if (dirty & D_ALL) lcd_fill(0, 0, 240, 240, C_BLACK);
    if (dirty & (D_ALL | D_HEAD)) {
        ui_text(8, 4, 150, &FONT_S, p->name, C_ACC, 0);
        if (p->tempo_x10 % 10u) ui_fmt(b, sizeof b, "%u.%u BPM", p->tempo_x10 / 10u, p->tempo_x10 % 10u);
        else ui_fmt(b, sizeof b, "%u BPM", p->tempo_x10 / 10u);
        ui_text(158, 4, 76, &FONT_S, b, C_WHITE, 2);
    }
    if (dirty & (D_ALL | D_POS)) {
        ui_fmt(b, sizeof b, "%u:%u", p->pos / (4u * PIECE_PPQ) + 1u, p->pos / PIECE_PPQ % 4u + 1u);
        ui_text(0, 26, 240, &FONT_L, b, C_WHITE, 1);
    }
    if (dirty & (D_ALL | D_STRIPS)) {
        for (ch = 0; ch < PIECE_MAX_CH; ch++) {
            uint32_t y = 64u + ch * 19u;
            int heard = p->solo ? (int)(p->solo >> ch & 1u) : !(p->mute >> ch & 1u);
            uint16_t c = ch == p->sel ? C_ACC : heard ? C_WHITE : C_MUTE;
            if (ch >= p->nch) { lcd_fill(0, y, 240, 19, C_BLACK); continue; }
            ui_fmt(b, sizeof b, "%s%u", ch == p->sel ? ">" : " ", ch + 1u);
            ui_text(0, y, 24, &FONT_S, b, c, 0);
            ui_text(26, y, 70, &FONT_S, ROLE_NAME[p->role[ch]], c, 0);
            ui_text(98, y, 108, &FONT_S, sound_name(p->sound[ch]), c, 0);
            ui_text(206, y, 30, &FONT_S, p->solo >> ch & 1u ? "S" : p->mute >> ch & 1u ? "M" : "", c, 2);
        }
    }
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
