/* SPDX-License-Identifier: GPL-3.0-only */
/* ui.c: COMPOSER's screens, drawn only where something changed (240 x 240, FONT_S rows).
 *   LIBRARY  the pieces, newest first; SELECT moves, PLAY opens one and plays it
 *   PLAYER   the piece: name, tempo, bar:beat, then a strip per channel (role, sound, M / S) */
#define C_ACC RGB(255, 165, 0)                  /* amber */
#define C_GREY RGB(120, 120, 120)
#define C_MUTE RGB(70, 70, 70)
#define LIB_ROWS 9u
enum { D_ALL = 1, D_LIST = 2, D_HEAD = 4, D_POS = 8, D_FOOT = 16, D_STRIPS = 32 };

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
    printf("%6u ms  SCREEN %u %u %s\n", fm1_ms, y, x, s);
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
            ui_text(26, y, 70, &FONT_S, ROLE_NAME[p->role[ch] < ROLE_COUNT ? p->role[ch] : ROLE_OTHER], c, 0);
            ui_text(98, y, 108, &FONT_S, sound_name(p->sound[ch]), c, 0);
            ui_text(206, y, 30, &FONT_S, p->solo >> ch & 1u ? "S" : p->mute >> ch & 1u ? "M" : "", c, 2);
        }
    }
    if (dirty & (D_ALL | D_FOOT)) ui_text(0, 222, 240, &FONT_S, p->status, C_GREY, 1);
}
