/* SPDX-License-Identifier: GPL-3.0-only */
/* COMPOSER: an FM-1 firmware that keeps a library of up to 8-channel pieces composed on a computer
 * (fm1c pushes them over USB-MIDI), plays them, and lets you pick each channel's sound.
 * Built on Dinghy (zvenson) / Felucca (Leo Kuroshita, Hugelton Instruments) / SLOOP (isod89): platform.c and below.
 * Sound engines: Felucca-Engines (MIT), app/engines. The chip's rules: docs/RULES.md. */
#define APP_NAME "COMPOSER"
static void app_idle(void);
#define LCD_IDLE app_idle
#include "platform.c"

#define C_ACC RGB(255, 165, 0)                  /* amber */
#define C_DIM RGB(120, 120, 120)

static volatile uint8_t idle_ok;

/* every line of text the UI draws goes through here; host builds log it, so tests read the screen as text */
static void ui_text(uint32_t x, uint32_t y, uint32_t w, const felucca_font_t *f, const char *s, uint16_t c, int align)
{
    draw_text_box(x, y, w, f, s, c, align);
#ifdef DINGHY_HOST
    printf("%6u ms  SCREEN %u %s\n", fm1_ms, y, s);
#endif
}

static void app_audio(int32_t *out, uint32_t n)
{
    (void)out; (void)n;                         /* nothing plays yet: the buffer stays zero */
}

static void midi_in(void)
{
    uint8_t st, d1, d2;
    while (midi_get(&st, &d1, &d2)) {}
}

static void app_idle(void)
{
    if (idle_ok) midi_in();
}

static void app_main(void)
{
    lcd_fill(0, 0, 240, 240, C_BLACK);
    ui_text(0, 18, 240, &FONT_L, APP_NAME, C_ACC, 1);
    ui_text(0, 112, 240, &FONT_S, "LIBRARY EMPTY", C_DIM, 1);
    ui_text(0, 136, 240, &FONT_S, "push pieces with fm1c", C_DIM, 1);
    for (;;) {
        platform_poll();
        midi_in();
    }
}
