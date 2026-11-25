/* SPDX-License-Identifier: GPL-3.0-only */
/* uart_sysex_test (US-12, US-33): SysEx frames out of the TRS jack's byte stream, through the jack's real parser
 * (midi_uart.c's um_byte, which also still turns notes into packets); up to UX_Q (4) finished frames wait, in order */
#include <stdint.h>
#include <stdio.h>
#include <string.h>
static uint32_t midi_in_q[64], mi_w, mi_r;
#define MQ 64u
#define RING_PUBLISH() do { } while (0)
static volatile uint8_t midi_in_lost;           /* (the app's: composer.c) */
#include "midi_uart.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { fails++; printf("FAIL %s:%d ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); } } while (0)

static void feed(const uint8_t *b, uint32_t n) { uint32_t i; for (i = 0; i < n; i++) um_byte(b[i]); }
static uint32_t got(uint8_t *out)               /* the waiting frame (copied, then taken), or 0 */
{
    const uint8_t *p;
    uint32_t n;
    if (!uart_sysex_get(&p, &n)) return 0;
    memcpy(out, p, n);
    uart_sysex_done();
    return n;
}

static void test_12_1_a_frame_byte_identical_and_notes_still_parsed(void)
{
    static const uint8_t in[] = {0x90, 60, 100, 0xF0, 0x7D, 0x46, 0x31, 0x01, 0x05, 0xF7, 0x80, 60, 0};
    uint8_t out[UX_MAX];
    uint32_t n;
    mi_w = mi_r = 0;
    feed(in, sizeof in);
    n = got(out);
    CHECK(n == 5 && !memcmp(out, in + 4, 5), "frame of %u", n);
    CHECK(mi_w == 2 && (midi_in_q[0] >> 8 & 0xFFu) == 0x90 && (midi_in_q[1] >> 8 & 0xFFu) == 0x80, "notes: %u packets", mi_w);
    CHECK(!got(out), "only one frame");
}

static void test_12_3_hostile_frames(void)
{
    static uint8_t big[UX_MAX + 10];
    uint8_t out[UX_MAX];
    uint32_t d0 = ux_drops, i;
    for (i = 0; i < sizeof big; i++) big[i] = (uint8_t)(i & 0x7Fu);
    um_byte(0xF0); feed(big, sizeof big); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 1, "too long: dropped, counted");
    um_byte(0xF7);                              /* F7 alone: nothing */
    CHECK(!got(out), "lone F7");
    um_byte(0xF0); um_byte(1); um_byte(2); um_byte(0xF0); um_byte(3); um_byte(0xF7);
    CHECK(got(out) == 1 && out[0] == 3 && ux_drops == d0 + 2, "a new F0 restarts");
    um_byte(0xF0); um_byte(1); um_byte(0xF8); um_byte(0xFE); um_byte(2); um_byte(0xF7);
    CHECK(got(out) == 2 && out[0] == 1 && out[1] == 2, "realtime inside SysEx passed over");
    um_byte(0xF0); um_byte(1); um_byte(0x90); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 3, "cut by a status byte");
    um_byte(0xF0); um_byte(9); um_byte(0xF7);
    um_byte(0xF0); um_byte(8); um_byte(0xF7);    /* a second frame while the first waits: it waits too (US-33) */
    CHECK(got(out) == 1 && out[0] == 9 && ux_drops == d0 + 3, "33.1: the first frame, the second not dropped");
    CHECK(got(out) == 1 && out[0] == 8, "33.1: then the second");
    um_byte(0xF0); um_byte(7); um_byte(0xF7);
    CHECK(got(out) == 1 && out[0] == 7, "the next frame works");
    um_byte(0xF0); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 4, "an empty frame is dropped");
    um_byte(0xF0); feed(big, UX_MAX); um_byte(0xF7);
    CHECK(got(out) == UX_MAX, "exactly UX_MAX fits");
}

static void test_12_1_clock_running_status_and_a_full_queue(void)
{
    static const uint8_t in[] = {0xF8, 0x90, 60, 0xF8, 100, 62, 90};
    uint32_t i, d0;
    mi_w = mi_r = 0;
    feed(in, sizeof in);
    CHECK(mi_w == 4 && midi_in_q[0] == (0x1Fu | 0xF8u << 8) && midi_in_q[1] == midi_in_q[0], "clock packets: %u", mi_w);
    CHECK(midi_in_q[2] == (0x19u | 0x90u << 8 | 60u << 16 | 100u << 24), "a note across a clock byte: %08X", midi_in_q[2]);
    CHECK(midi_in_q[3] == (0x19u | 0x90u << 8 | 62u << 16 | 90u << 24), "running status");
    mi_w = mi_r = 0;
    d0 = um.drops;
    for (i = 0; i < MQ + 1u; i++) { um_byte(0x90); um_byte(60); um_byte(1); }
    CHECK(mi_w == MQ && um.drops == d0 + 1, "a full queue drops (and counts) the next: %u %u", mi_w, um.drops - d0);
}

static void frame(uint8_t tag, uint32_t n)      /* F0, n bytes (tag, then a count), F7 */
{
    uint32_t i;
    um_byte(0xF0);
    for (i = 0; i < n; i++) um_byte(i ? (uint8_t)((tag + i) & 0x7Fu) : tag);
    um_byte(0xF7);
}

static int is_frame(const uint8_t *p, uint32_t n, uint8_t tag, uint32_t want)
{
    uint32_t i;
    if (n != want) return 0;
    for (i = 0; i < n; i++) if (p[i] != (i ? (uint8_t)((tag + i) & 0x7Fu) : tag)) return 0;
    return 1;
}

static void test_33_1_frames_back_to_back_all_handed_over_in_order(void)
{
    static uint8_t out[UX_MAX];
    uint32_t d0 = ux_drops, k, n;
    for (k = 0; k < UX_Q; k++) frame((uint8_t)(10 + k), k == 1 ? UX_MAX : 300u + k);     /* before the app looks */
    CHECK(ux_drops == d0, "33.1: 4 back to back, none dropped (%u)", ux_drops - d0);
    for (k = 0; k < UX_Q; k++) {
        n = got(out);
        CHECK(is_frame(out, n, (uint8_t)(10 + k), k == 1 ? UX_MAX : 300u + k), "33.1: frame %u in order, intact (%u B)", k, n);
    }
    CHECK(!got(out), "33.1: then none");
    for (k = 0; k < 40u; k++) {                 /* the slots run round: taken one by one, 2 ahead */
        frame((uint8_t)k, 1u + k * 17u);
        if (k >= 2u) { n = got(out); CHECK(is_frame(out, n, (uint8_t)(k - 2u), 1u + (k - 2u) * 17u), "33.1: round %u", k); }
    }
    while (got(out)) { }
    CHECK(ux_drops == d0, "33.1: rounds, none dropped");
}

static void test_33_1_a_5th_while_4_wait_is_dropped_and_counted(void)
{
    static uint8_t out[UX_MAX];
    const uint8_t *p;
    uint32_t d0 = ux_drops, k, n;
    for (k = 0; k < UX_Q; k++) frame((uint8_t)(20 + k), 5u + k);
    CHECK(uart_sysex_get(&p, &n) && is_frame(p, n, 20, 5), "33.1: the oldest waits, looked at, not taken");
    frame(99, 7);                               /* a 5th while 4 wait (the app is reading the oldest) */
    CHECK(ux_drops == d0 + 1, "33.1: the 5th dropped, counted (%u)", ux_drops - d0);
    CHECK(is_frame(p, n, 20, 5), "33.1: the frame being read is untouched");
    for (k = 0; k < UX_Q; k++) { n = got(out); CHECK(is_frame(out, n, (uint8_t)(20 + k), 5u + k), "33.1: the 4 intact: %u", k); }
    CHECK(!got(out), "33.1: the 5th never shows");
    frame(30, 3);
    CHECK(got(out) == 3 && out[0] == 30 && ux_drops == d0 + 1, "33.1: a free slot takes the next");
}

static void test_33_2_hostile_frames(void)
{
    static uint8_t big[UX_MAX + 1], out[UX_MAX];
    uint32_t d0 = ux_drops, i;
    for (i = 0; i < sizeof big; i++) big[i] = (uint8_t)(i * 7u & 0x7Fu);
    frame(5, 1);
    CHECK(got(out) == 1 && out[0] == 5, "33.2: a 1-byte frame");
    um_byte(0xF0); feed(big, UX_MAX); um_byte(0xF7);
    CHECK(got(out) == UX_MAX && !memcmp(out, big, UX_MAX), "33.2: a UX_MAX frame");
    um_byte(0xF0); feed(big, UX_MAX + 1u); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 1, "33.2: UX_MAX+1 dropped, counted");
    um_byte(0xF0); um_byte(1); um_byte(0xB0); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 2, "33.2: cut by a status byte: dropped");
    um_byte(0xF0); um_byte(1); um_byte(0xF0); um_byte(2); um_byte(0xF7);
    CHECK(got(out) == 1 && out[0] == 2 && ux_drops == d0 + 3, "33.2: cut by a new F0: the first dropped");
    um_byte(0xF0); um_byte(1); um_byte(0xF8); um_byte(0xFA); um_byte(0xFE); um_byte(0xFF); um_byte(2); um_byte(0xF7);
    CHECK(got(out) == 2 && out[0] == 1 && out[1] == 2 && ux_drops == d0 + 3, "33.2: realtime inside passed over");
}

/* 33.2: the reference model: a frame is the whole run of data bytes after F0 up to the next status byte (realtime
 * bytes not counted); kept if that status is F7, it has 1..UX_MAX bytes and fewer than 4 wait; else, if a frame was
 * open, counted as dropped. Frames are kept as offsets into the stream (no buffers of its own). */
static uint32_t rng = 0x33u;
static uint32_t rnd(uint32_t n) { rng ^= rng << 13; rng ^= rng >> 17; rng ^= rng << 5; return rng % n; }

static void test_33_2_random_streams_vs_a_model(void)
{
    static uint8_t s[8192], out[UX_MAX], body[UX_MAX + 8];
    static uint32_t mq_at[UX_Q], mq_n[UX_Q];
    uint32_t mw = 0, mr = 0, mdrops = 0, d0 = ux_drops, frames = 0, bad = 0, st;
    uint32_t len, i, k, n, open, start, cnt;
    open = 0; start = 0; cnt = 0;
    while (got(out)) { }
    um_byte(0xF7);                              /* (no frame open, as the model starts) */
    for (st = 0; st < 10000u && !bad; st++) {
        len = 0;
        while (len < sizeof s - UX_MAX - 16u && rnd(4)) {
            uint32_t r = rnd(16), j, fl;
            if (r < 9) {                        /* a frame: of a length near a boundary or anything */
                uint32_t m = rnd(5);
                fl = m == 0 ? rnd(4) : m == 1 ? UX_MAX - 2u + rnd(5) : m == 2 ? rnd(UX_MAX + 4u) : 1u + rnd(40);
                s[len++] = 0xF0;
                for (j = 0; j < fl; j++) {
                    if (!rnd(50)) s[len++] = (uint8_t)(0xF8u + rnd(8));
                    s[len++] = (uint8_t)rnd(128);
                }
                k = rnd(10);                    /* mostly F7; else cut by a status, a new F0, or left open */
                if (k < 7) s[len++] = 0xF7;
                else if (k == 7) s[len++] = (uint8_t)(0x80u + rnd(0x70));
                else if (k == 8) s[len++] = (uint8_t)(0xF0u + rnd(8));
            } else if (r < 14) {
                s[len++] = (uint8_t)rnd(256);    /* noise: any byte */
            } else {
                s[len++] = (uint8_t)(0x80u + rnd(0x70)); s[len++] = (uint8_t)rnd(128); s[len++] = (uint8_t)rnd(128);
            }
        }
        s[len++] = 0xF7;                        /* (each stream ends any open frame: the model reads s, rewritten next) */
        mi_w = mi_r = 0;
        for (i = 0; i < len; i++) {
            uint32_t b = s[i];
            if (!rnd(64)) {                     /* the app takes a frame now and then (both sides) */
                n = got(out);
                if (mw != mr) {
                    uint32_t q = mr % UX_Q, at = mq_at[q], c = 0, x;
                    for (x = at; c < mq_n[q]; x++) if ((s[x] & 0x80u) == 0) body[c++] = s[x];   /* data bytes */
                    if (n != mq_n[q] || memcmp(out, body, n)) bad++;
                    mr++; frames++;
                } else if (n) bad++;
            }
            um_byte(b);
            if (b >= 0xF8u) continue;
            if (b & 0x80u) {                    /* model */
                if (open) {
                    if (b == 0xF7u && cnt >= 1u && cnt <= UX_MAX && mw - mr < UX_Q) {
                        mq_at[mw % UX_Q] = start; mq_n[mw % UX_Q] = cnt; mw++;
                    } else mdrops++;
                }
                open = b == 0xF0u; start = i + 1u; cnt = 0;
            } else if (open) cnt++;
        }
        /* the end of a stream: everything waiting comes out the same, the drops counted the same */
        while (mw != mr) {
            uint32_t q = mr % UX_Q, at = mq_at[q], c = 0, x;
            for (x = at; c < mq_n[q]; x++) if ((s[x] & 0x80u) == 0) body[c++] = s[x];
            n = got(out);
            if (n != mq_n[q] || memcmp(out, body, n)) bad++;
            mr++; frames++;
        }
        if (got(out)) bad++;
        if (ux_drops - d0 != mdrops) bad++;
        CHECK(!bad, "33.2: stream %u: frames or drops differ from the model (%u vs %u drops)", st, ux_drops - d0, mdrops);
    }
    CHECK(st == 10000u && frames > 10000u && mdrops > 1000u, "33.2: 10000 streams, %u frames, %u drops", frames, mdrops);
    printf("33.2: %u streams, %u frames, %u drops: as the model\n", st, frames, mdrops);
}

int main(void)
{
    test_12_1_clock_running_status_and_a_full_queue();
    test_12_1_a_frame_byte_identical_and_notes_still_parsed();
    test_12_3_hostile_frames();
    test_33_1_frames_back_to_back_all_handed_over_in_order();
    test_33_1_a_5th_while_4_wait_is_dropped_and_counted();
    test_33_2_hostile_frames();
    test_33_2_random_streams_vs_a_model();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
