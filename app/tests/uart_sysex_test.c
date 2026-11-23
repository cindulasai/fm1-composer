/* SPDX-License-Identifier: GPL-3.0-only */
/* uart_sysex_test (US-12): SysEx frames out of the TRS jack's byte stream, through the jack's real parser
 * (midi_uart.c's um_byte, which also still turns notes into packets) */
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
    um_byte(0xF0); um_byte(8); um_byte(0xF7);    /* a second frame while the first waits */
    CHECK(got(out) == 1 && out[0] == 9 && ux_drops == d0 + 4, "one waits; the next is dropped");
    um_byte(0xF0); um_byte(7); um_byte(0xF7);
    CHECK(got(out) == 1 && out[0] == 7, "the next frame works");
    um_byte(0xF0); um_byte(0xF7);
    CHECK(!got(out) && ux_drops == d0 + 5, "an empty frame is dropped");
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

int main(void)
{
    test_12_1_clock_running_status_and_a_full_queue();
    test_12_1_a_frame_byte_identical_and_notes_still_parsed();
    test_12_3_hostile_frames();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
