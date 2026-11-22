/* SPDX-License-Identifier: GPL-3.0-only */
/* ui_test: the screens' formatter (the device has no printf): every format, truncation never past the buffer */
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
#define PIECE_PPQ 96u
#define PIECE_MAX_CH 8u
enum { ROLE_LEAD, ROLE_COUNTER, ROLE_CHORDS, ROLE_PAD, ROLE_BASS, ROLE_DRUMS, ROLE_PERC, ROLE_ARP, ROLE_DRONE, ROLE_FX, ROLE_OTHER, ROLE_COUNT };
typedef struct { uint32_t id, seq, len, first, span; char name[25]; } lib_entry_t;
static void draw_text_box(uint32_t x, uint32_t y, uint32_t w, const felucca_font_t *f, const char *s, uint16_t c, int a) { (void)x; (void)y; (void)w; (void)f; (void)s; (void)c; (void)a; }
static void lcd_fill(uint32_t x, uint32_t y, uint32_t w, uint32_t h, uint16_t c) { (void)x; (void)y; (void)w; (void)h; (void)c; }
static uint32_t lib_count(void) { return 0; }
static const lib_entry_t *lib_entry(uint32_t i) { (void)i; return 0; }
static uint32_t lib_free(void) { return 0; }
static const char *sound_name(uint32_t id) { (void)id; return ""; }

#include "ui.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; printf("FAIL %s:%d ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

int main(void)
{
    char b[24];
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
