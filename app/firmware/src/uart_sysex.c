/* SPDX-License-Identifier: GPL-3.0-only */
/* uart_sysex.c: SysEx frames out of a MIDI byte stream (the TRS jack), so pieces can arrive there as over USB.
 * ux_byte() takes every byte (the jack's parser calls it in its interrupt; realtime bytes are passed over, as
 * MIDI lets them sit inside SysEx). A whole frame (the bytes between F0 and F7) waits for the app in a second
 * buffer: uart_sysex_get() / uart_sysex_done(). One frame waits at a time. Dropped, and counted in ux_drops:
 * a frame finished while one waits, one longer than UX_MAX, one cut by another status byte or a new F0. */
#define UX_MAX 700u

static uint8_t ux_buf[UX_MAX], ux_out[UX_MAX];
static uint32_t ux_n, ux_drops;
static uint8_t ux_on, ux_over;
static volatile uint32_t ux_out_n;              /* the frame waiting for the app: its length, 0 = none */

static void ux_byte(uint32_t b)
{
    uint32_t i;
    if (b >= 0xF8u) return;
    if (b & 0x80u) {
        if (ux_on && b == 0xF7u && !ux_over && ux_n && !ux_out_n) {
            for (i = 0; i < ux_n; i++) ux_out[i] = ux_buf[i];
            __asm__ volatile("" ::: "memory");  /* (the bytes first, then the length the app looks at) */
            ux_out_n = ux_n;
        } else if (ux_on) {
            ux_drops++;
        }
        ux_on = b == 0xF0u;
        ux_n = 0;
        ux_over = 0;
        return;
    }
    if (!ux_on) return;
    if (ux_n < UX_MAX) ux_buf[ux_n++] = (uint8_t)b;
    else ux_over = 1;
}

static int uart_sysex_get(const uint8_t **p, uint32_t *n)
{
    uint32_t k = ux_out_n;
    if (!k) return 0;
    *p = ux_out;
    *n = k;
    return 1;
}

static void uart_sysex_done(void) { ux_out_n = 0; }
