/* SPDX-License-Identifier: GPL-3.0-only */
/* uart_sysex.c: SysEx frames out of a MIDI byte stream (the TRS jack), so pieces can arrive there as over USB.
 * ux_byte() takes every byte (the jack's parser calls it in its interrupt; realtime bytes are passed over, as
 * MIDI lets them sit inside SysEx). Whole frames (the bytes between F0 and F7) wait for the app, in order, in UX_Q
 * slots: uart_sysex_get() (the oldest) / uart_sysex_done(). Studio keeps up to 2000 bytes in the jack, so a slow
 * main loop finds several frames finished (US-33; one slot dropped one, decision 51). Dropped, and counted in
 * ux_drops: a frame finished while UX_Q wait, one longer than UX_MAX, one cut by another status byte or a new F0.
 * One writer (ux_byte, the interrupt: ux_w) and one reader (the main loop: ux_r); each only moves its own index. */
#define UX_MAX 700u
#define UX_Q 4u                                 /* (a power of two: the indexes run on and wrap) */

static uint8_t ux_buf[UX_MAX], ux_out[UX_Q][UX_MAX];
static uint32_t ux_n, ux_drops, ux_len[UX_Q];
static uint8_t ux_on, ux_over;
static volatile uint32_t ux_w, ux_r;            /* frames finished / taken: ux_w - ux_r wait */

static void ux_byte(uint32_t b)
{
    uint32_t i, s;
    if (b >= 0xF8u) return;
    if (b & 0x80u) {
        if (ux_on && b == 0xF7u && !ux_over && ux_n && ux_w - ux_r < UX_Q) {
            s = ux_w % UX_Q;
            for (i = 0; i < ux_n; i++) ux_out[s][i] = ux_buf[i];
            ux_len[s] = ux_n;
            __asm__ volatile("" ::: "memory");  /* (the bytes first, then the index the app looks at) */
            ux_w++;
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
    uint32_t r = ux_r, s = r % UX_Q;
    if (ux_w == r) return 0;
    __asm__ volatile("" ::: "memory");          /* (the index first, then the bytes it stands for) */
    *p = ux_out[s];
    *n = ux_len[s];
    return 1;
}

static void uart_sysex_done(void)
{
    __asm__ volatile("" ::: "memory");          /* (the app is through with the bytes before the slot is freed) */
    if (ux_w != ux_r) ux_r++;
}
