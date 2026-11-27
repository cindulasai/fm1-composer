/* SPDX-License-Identifier: GPL-3.0-only */
/* ui_test: the screens' formatter (the device has no printf): every format, truncation never past the buffer; the
 * sections strip and the EDIT roll at their edges (what they draw, recorded) */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
typedef struct { int h; } felucca_font_t;
static const felucca_font_t FONT_S, FONT_L;
#define RGB(r, g, b) ((uint16_t)(((r) >> 3) << 11 | ((g) >> 2) << 5 | (b) >> 3))
#define C_BLACK 0
#define C_WHITE 0xFFFF
#define APP_NAME "COMPOSER"
#include "piece.h"
typedef struct { uint32_t id, seq, len, first, span; char name[25]; } lib_entry_t;
static char drawn[64][32];                      /* the text drawn, the widest fill */
static uint32_t ndrawn, widest;
static void draw_text_box(uint32_t x, uint32_t y, uint32_t w, const felucca_font_t *f, const char *s, uint16_t c, int a)
{
    (void)x; (void)y; (void)w; (void)f; (void)c; (void)a;
    if (ndrawn < 64u) { strncpy(drawn[ndrawn], s, 31); drawn[ndrawn++][31] = 0; }
}
static uint16_t fb[240 * 240];                  /* the screen the fills paint (clipped to 240 x 240) */
static void lcd_fill(uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint16_t c)
{
    uint32_t i, j;
    if (x + w > widest) widest = x + w;
    for (j = y; j < y + h && j < 240u; j++)
        for (i = x; i < x + w && i < 240u; i++) fb[j * 240u + i] = c;
}
static uint32_t lib_count(void) { return 0; }
static const lib_entry_t *lib_entry(uint32_t i) { (void)i; return 0; }
static uint32_t lib_free(void) { return 0; }
static const char *sound_name(uint32_t id) { (void)id; return ""; }
static int32_t text_w(const felucca_font_t *f, const char *s) { (void)f; return (int32_t)strlen(s) * 8; }
#define PL_NONE 255u
#include "edit.c"
#include "steps.c"                              /* (STEPS draws its rows) */

#include "ui.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; printf("FAIL %s:%d ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

static int was_drawn(const char *s) { uint32_t i; for (i = 0; i < ndrawn; i++) if (!strcmp(drawn[i], s)) return 1; return 0; }

static void strip_and_roll_edges(void)
{
    static uint8_t sect[6 * PIECE_SECT], pc[PIECE_HDR + PIECE_CHREC + 6 + 4];
    piece_view_t v;
    ui_player_t p;
    ui_edit_t e;
    uint32_t i;
    memset(&v, 0, sizeof v);
    memset(&p, 0, sizeof p);
    for (i = 0; i < 6u; i++) memcpy(sect + i * PIECE_SECT + 4, "AAA", 3);   /* "1 AAA" .. "5 AAA": 40 px, 10 between */
    v.nsect = 6;
    v.sect = sect;
    p.v = &v;
    p.sec = p.queued = PL_NONE;
    ndrawn = 0;
    ui_sections(&p, 202);
    CHECK(was_drawn("1 AAA") && was_drawn("5 AAA") && !was_drawn("6 AAA"), "the 5th ends at 240: it fits; the 6th does not");
    memset(pc, 0, sizeof pc);                   /* one note, 0..384: it ends where the roll's 2nd page starts */
    memcpy(pc, "F1P1", 4); pc[4] = 1; pc[5] = 1; pc[8] = 0xB0; pc[9] = 4; pc[10] = 96; pc[13] = 3;   /* 1200, 96 PPQ, 768 */
    pc[PIECE_HDR] = ROLE_LEAD; pc[PIECE_HDR + 2] = 100; pc[PIECE_HDR + 3] = 64; pc[PIECE_HDR + 8] = 1; pc[PIECE_HDR + 12] = PIECE_HDR + PIECE_CHREC;
    pc[PIECE_HDR + PIECE_CHREC + 3] = 1; pc[PIECE_HDR + PIECE_CHREC + 4] = 60; pc[PIECE_HDR + PIECE_CHREC + 5] = 100;   /* dur 256+128 */
    pc[PIECE_HDR + PIECE_CHREC + 2] = 128;
    pc[40] = sizeof pc;
    memset(&e, 0, sizeof e);
    e.status = ""; e.piece = pc; e.tick = 384; e.grid = 24; e.gi = 1; e.low = 53; e.len = 24; e.vel = 100;
    widest = 0;
    ui_edit(D_ALL, &e);
    CHECK(widest <= 240u, "a note ending at the page's start is not drawn on it: %u", widest);
}

/* 30 (STEPS grid): the cursor moved off step 16 leaves no white edge behind: the grid's clear spans its right border */
static void steps_cursor_leaves_no_trail(void)
{
    static uint8_t pc[PIECE_HDR + PIECE_CHREC + 6 + 4];
    static const uint8_t role[PIECE_MAX_CH] = {ROLE_LEAD};
    uint32_t at[PIECE_MAX_CH], x = ST_X + ST_PAGE * ST_W;   /* step 16's right edge: 232 */
    ui_steps_t u;
    memset(at, 0xFF, sizeof at);
    memset(pc, 0, sizeof pc);
    memcpy(pc, "F1P1", 4); pc[4] = 1; pc[5] = 1; pc[8] = 0xB0; pc[9] = 4; pc[10] = 96; pc[13] = 3;
    pc[PIECE_HDR] = ROLE_LEAD; pc[PIECE_HDR + 2] = 100; pc[PIECE_HDR + 3] = 64; pc[PIECE_HDR + 8] = 1; pc[PIECE_HDR + 12] = PIECE_HDR + PIECE_CHREC;
    pc[PIECE_HDR + PIECE_CHREC + 3] = 1; pc[PIECE_HDR + PIECE_CHREC + 4] = 60; pc[PIECE_HDR + PIECE_CHREC + 5] = 100;
    pc[PIECE_HDR + PIECE_CHREC + 2] = 128;
    pc[40] = sizeof pc;
    memset(&u, 0, sizeof u);
    u.status = ""; u.piece = pc; u.role = role; u.at = at; u.len = 24; u.vel = 100;
    u.step = 15;
    ui_steps(D_ALL, &u);
    CHECK(fb[ST_Y * 240u + x] == C_WHITE, "the cursor on step 16 draws its right edge at x %u", x);
    u.step = 14;
    fb[(ST_Y + PIECE_MAX_CH * ST_H + 2u) * 240u + ST_X + 5u] = 0x1234u;   /* below the 8 rows: the footer's */
    ui_steps(D_POS, &u);
    CHECK(fb[(ST_Y + PIECE_MAX_CH * ST_H + 2u) * 240u + ST_X + 5u] == 0x1234u, "the grid redraws its 8 rows, nothing below");
    CHECK(fb[ST_Y * 240u + x] == C_BLACK && fb[(ST_Y + ST_H / 2u) * 240u + x] == C_BLACK,
          "the cursor moved to step 15: step 16's right edge (x %u) is cleared", x);
    CHECK(fb[ST_Y * 240u + x - ST_W] == C_WHITE, "step 15's right edge is the cursor's now");
}

int main(void)
{
    char b[24];
    strip_and_roll_edges();
    steps_cursor_leaves_no_trail();
    memset(b, '#', sizeof b);
    ui_fmt(b, 8, "%s", "abcdefghijk");
    CHECK(!strcmp(b, "abcdefg") && b[8] == '#', "text cut at 7, nothing past the buffer");
    memset(b, '#', sizeof b);
    ui_fmt(b, 8, "abcdefghijk");
    CHECK(!strcmp(b, "abcdefg") && b[8] == '#', "literal cut at 7");
    memset(b, '#', sizeof b);
    ui_fmt(b, 4, "%u", 123456u);
    CHECK(!strcmp(b, "123") && b[4] == '#', "number cut at 3: %s", b);
    ui_fmt(b, sizeof b, "%u:%u %s%c%%", 0u, 4294967295u, "x", 'y');
    CHECK(!strcmp(b, "0:4294967295 xy%"), "formats: %s", b);
    ui_fmt(b, sizeof b, "end%");
    CHECK(!strcmp(b, "end%"), "a lone %% at the end: %s", b);
    ui_fmt(b, 1, "abc");
    CHECK(b[0] == 0, "room for nothing but the end");
    printf(fails ? "ui_test: %d FAILED\n" : "ui_test: ok\n", fails);
    return fails != 0;
}
