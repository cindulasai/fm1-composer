/* SPDX-License-Identifier: GPL-3.0-only */
/* US-7: xfer.c through its interface (frames in, frames out) with the real library on a RAM flash. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "piece.h"
#include "store.h"

static uint8_t fl[LIB_SECT * 4096u];
static int broken;                              /* 1: erases and writes fail */
static int sim_read(uint32_t off, void *p, uint32_t n) { if (off < LIB_AT || off - LIB_AT + n > sizeof fl) return -1; memcpy(p, fl + (off - LIB_AT), n); return 0; }
static int sim_erase(uint32_t off) { if (broken || off < LIB_AT || off - LIB_AT >= sizeof fl) return -1; memset(fl + (off - LIB_AT), 0xFF, 4096); return 0; }
static int sim_write(uint32_t off, const void *p, uint32_t n) { uint32_t i; if (broken || off < LIB_AT || off - LIB_AT + n > sizeof fl) return -1; for (i = 0; i < n; i++) fl[(off - LIB_AT) + i] &= ((const uint8_t *)p)[i]; return 0; }
#define LIB_READ(off, p, n) sim_read(off, p, n)
#define LIB_ERASE(off) sim_erase(off)
#define LIB_WRITE(off, p, n) sim_write(off, p, n)
#include "lib.c"

static uint8_t sent[400][400];
static uint32_t sent_n[400], nsent;
static void cap(const uint8_t *p, uint32_t n) { if (nsent < 400u && n <= 400u) { memcpy(sent[nsent], p, n); sent_n[nsent] = n; } nsent++; }
#define XFER_SEND(p, n) cap(p, n)
#define XF_POOL
#include "xfer.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

static uint32_t now;
static int playing;
static uint8_t rp[300];                         /* the last reply: cmd, payload */
static uint32_t rcmd, rlen;

static int send_raw(const uint8_t *f, uint32_t n)
{
    uint32_t before = nsent;
    int ours = xfer_frame(f, n, now, playing);
    rcmd = 0; rlen = 0;
    if (nsent > before) {
        const uint8_t *w = sent[nsent - 1u];
        uint32_t wn = sent_n[nsent - 1u];
        int32_t d;
        CHECK(w[0] == 0xF0 && w[wn - 1] == 0xF7 && w[1] == 0x7D && w[2] == 0x46 && w[3] == 0x31, "reply framing");
        d = xf_unpack7(w + 5, wn - 6u, rp, sizeof rp);
        CHECK(d >= 4 && lib_crc(0, rp, (uint32_t)d - 4u) == xf_rd32(rp + d - 4), "reply CRC");
        rcmd = w[4]; rlen = (uint32_t)d - 4u;
    }
    return ours;
}
static void send(uint32_t cmd, const uint8_t *pay, uint32_t n)
{
    uint8_t raw[300], f[400];
    uint32_t w;
    if (n) memcpy(raw, pay, n);
    xf_wr32(raw + n, lib_crc(0, raw, n));
    f[0] = 0x7D; f[1] = 0x46; f[2] = 0x31; f[3] = (uint8_t)cmd;
    w = 4u + xf_pack7(raw, n + 4u, f + 4);
    send_raw(f, w);
}
static void send32(uint32_t cmd, uint32_t v) { uint8_t p[4]; xf_wr32(p, v); send(cmd, p, 4); }
static int nak(uint32_t code) { return rcmd == 0x7F && rlen == 2 && rp[1] == code; }

/* a valid piece of exactly `size` bytes (filler notes; size >= 68 and (size - 68) % 6 == 0) */
static uint8_t pc[PIECE_MAX_BYTES];
static uint32_t make_piece(uint32_t size, uint32_t seed)
{
    uint32_t nev = (size - 68u) / 6u, i, off = 64;
    memset(pc, 0, sizeof pc);
    memcpy(pc, "F1P1", 4); pc[4] = 1; pc[5] = 1; pc[8] = 0xB0; pc[9] = 0x04; pc[10] = 96;
    xf_wr32(pc + 12, 96u * 4u * 999u);
    snprintf((char *)pc + 16, 24, "piece %u", seed);
    xf_wr32(pc + 40, size);
    pc[48] = ROLE_LEAD; pc[49] = 255; pc[50] = 100; pc[51] = 64;
    xf_wr32(pc + 56, nev); xf_wr32(pc + 60, 64);
    for (i = 0; i < nev; i++, off += 6u) { pc[off] = (uint8_t)(i ? 60 : 0); pc[off + 2] = 30; pc[off + 4] = (uint8_t)(36 + (i * 7u + seed) % 60u); pc[off + 5] = 100; }
    xf_wr32(pc + off, piece_crc32(pc, off));
    return size;
}

static int put(const uint8_t *b, uint32_t n)   /* BEGIN, DATA..., END; the END reply's cmd */
{
    uint32_t off = 0;
    send32(0x03, n);
    if (rcmd != 0x43) return -1;
    while (off < n) {
        uint8_t p[4 + XF_CHUNK];
        uint32_t c = n - off > XF_CHUNK ? XF_CHUNK : n - off;
        xf_wr32(p, off); memcpy(p + 4, b + off, c);
        send(0x04, p, 4 + c);
        if (rcmd != 0x44 || xf_rd32(rp) != off + c) return -2;
        off += c;
    }
    send(0x05, 0, 0);
    return (int)rcmd;
}

static void fresh(void) { memset(fl, 0xFF, sizeof fl); lib_mount(); xf_state = XS_IDLE; xf_expired = 0; now = 1000; playing = 0; }

static void test_7_1_put_32k_then_get(void)
{
    uint32_t n = make_piece(32768 - (32768 - 68) % 6, 1), id, off = 0;
    static uint8_t back[PIECE_MAX_BYTES];
    fresh();
    CHECK(n > 32700 && piece_check(pc, n) == PE_OK, "test piece");
    CHECK(put(pc, n) == 0x45 && rlen == 4, "saved: reply %02X", rcmd);
    id = xf_rd32(rp);
    CHECK(lib_count() == 1 && lib_entry(0)->id == id && !strcmp(lib_entry(0)->name, "piece 1") && xf_changed, "in the library");
    while (off < n) {
        uint8_t q[8];
        xf_wr32(q, id); xf_wr32(q + 4, off);
        send(0x07, q, 8);
        CHECK(rcmd == 0x47 && xf_rd32(rp) == off && rlen > 4, "get at %u", off);
        if (rcmd != 0x47) break;
        memcpy(back + off, rp + 4, rlen - 4);
        off += rlen - 4;
    }
    CHECK(off == n && !memcmp(back, pc, n), "get returns identical bytes");
    send(0x01, 0, 0);
    CHECK(rcmd == 0x41 && rp[0] == 1 && rp[1] == 1 && xf_rd32(rp + 2) == lib_free() && xf_rd32(rp + 6) == lib_room() && rp[10] == 0, "info");
    { uint8_t i0 = 0; send(0x02, &i0, 1); }
    CHECK(rcmd == 0x42 && rp[0] == 0 && xf_rd32(rp + 1) == id && xf_rd32(rp + 5) == n && !memcmp(rp + 9, "piece 1", 8), "list");
    send32(0x06, id);
    CHECK(rcmd == 0x46 && lib_count() == 0, "delete");
}

static void test_7_2_staged_while_playing(void)
{
    uint32_t n = make_piece(68 + 6 * 100, 2);
    fresh();
    playing = 1;
    CHECK(put(pc, n) == 0x4F && lib_count() == 0, "staged, not yet in flash");
    send(0x01, 0, 0);
    CHECK(rp[10] == 3, "info: playing + staged");
    make_piece(68 + 6 * 10, 3);
    send32(0x03, 128);
    CHECK(nak(XF_EBUSY), "a second put while one is staged: busy");
    send(0x08, 0, 0);
    CHECK(rcmd == 0x48 && xf_state == XS_STAGED, "abort keeps the staged piece");
    nsent = 0;
    xfer_poll(now += 10, 1);
    CHECK(nsent == 0 && lib_count() == 0, "still playing: nothing written");
    xfer_poll(now += 10, 0);
    CHECK(nsent == 1 && lib_count() == 1 && !strcmp(lib_entry(0)->name, "piece 2"), "stopped: saved");
    { const uint8_t *w = sent[0]; int32_t d = xf_unpack7(w + 5, sent_n[0] - 6, rp, sizeof rp); CHECK(w[4] == 0x45 && d == 8 && xf_rd32(rp) == lib_entry(0)->id, "SAVED id sent unasked"); }
    playing = 0;
    make_piece(68 + 6 * 10, 4);
    { int r = put(pc, 68 + 6 * 10); CHECK(r == 0x45, "next put works: %d / %02X %u", r, rcmd, rlen ? rp[1] : 0); }
}

static void test_7_4_hostile(void)
{
    uint8_t f[64], p[300];
    uint32_t n = make_piece(68 + 6 * 200, 5);
    fresh();
    nsent = 0;
    { static const uint8_t upd[] = {0x00, 0x20, 0x59, 0x11, 0x00}, uboot[] = {0x22, 0x24, 0x35, 0x7D}, other[] = {0x7D, 0x00, 0x01, 0x02, 0x03};
      CHECK(!send_raw(upd, sizeof upd) && !send_raw(uboot, sizeof uboot) && !send_raw(other, sizeof other) && nsent == 0, "other SysEx: not ours, no reply"); }
    f[0] = 0x7D; f[1] = 0x46; f[2] = 0x31; f[3] = 0x01; f[4] = 0x00; f[5] = 0x10; f[6] = 0x20; f[7] = 0x30; f[8] = 0x40;
    CHECK(send_raw(f, 9) && nak(XF_EFRAME), "bad CRC");
    f[5] = 0x90;
    CHECK(send_raw(f, 9) && nak(XF_EFRAME), "a byte with bit 7");
    CHECK(send_raw(f, 4) && nak(XF_EFRAME), "no payload");
    send(0x55, 0, 0); CHECK(nak(XF_ECMD), "unknown command");
    send32(0x03, 32769); CHECK(nak(XF_ESIZE), "oversize");
    send32(0x03, 10); CHECK(nak(XF_ESIZE), "undersize");
    send(0x03, p, 2); CHECK(nak(XF_ESIZE), "short begin");
    xf_wr32(p, 0); memcpy(p + 4, pc, 8); send(0x04, p, 12); CHECK(nak(XF_ENOPUT), "data with no put");
    send(0x05, 0, 0); CHECK(nak(XF_ENOPUT), "end with no put");
    send32(0x03, n); CHECK(rcmd == 0x43, "begin");
    xf_wr32(p, 256); memcpy(p + 4, pc + 256, 256); send(0x04, p, 260); CHECK(nak(XF_ESEQ), "missing chunk");
    xf_wr32(p, 0); memcpy(p + 4, pc, 256); send(0x04, p, 260); CHECK(rcmd == 0x44, "first chunk");
    send(0x04, p, 260); CHECK(nak(XF_ESEQ), "same chunk twice");
    send(0x04, p, 4); CHECK(nak(XF_EFRAME), "a chunk of nothing");
    send(0x05, 0, 0); CHECK(nak(XF_ESHORT), "end before all data");
    xf_wr32(p, 256); memset(p + 4, 0, 256); send(0x04, p, 260); xf_wr32(p, 512); send(0x04, p, 260);
    xf_wr32(p, 768); send(0x04, p, 260); xf_wr32(p, 1024); send(0x04, p, 4 + n - 1024); CHECK(rcmd == 0x44 && xf_rd32(rp) == n, "rest");
    xf_wr32(p, n); send(0x04, p, 5); CHECK(nak(XF_ESIZE), "past the end");
    send(0x05, 0, 0); CHECK(nak(XF_EPIECE) && lib_count() == 0, "not a valid piece: nothing committed");
    send32(0x03, n); send(0x08, 0, 0); CHECK(rcmd == 0x48, "abort");
    xf_wr32(p, 0); send(0x04, p, 10); CHECK(nak(XF_ENOPUT), "data after abort");
    send32(0x03, n); xfer_poll(now + 4000, 0); xf_wr32(p, 0); memcpy(p + 4, pc, 256); now += 4000; send(0x04, p, 260);
    CHECK(rcmd == 0x44, "4 s apart is fine");
    xfer_poll(now + 5001, 0); now += 5001;
    send(0x04, p, 260); CHECK(nak(XF_ETIMEOUT), "idle 5 s: timed out");
    send(0x05, 0, 0); CHECK(nak(XF_ETIMEOUT), "end after timeout");
    CHECK(lib_count() == 0, "nothing committed");
    CHECK(put(pc, n) == 0x45 && lib_count() == 1, "the next put works");
    send32(0x06, 999); CHECK(nak(XF_ENONE), "delete unknown");
    send(0x06, p, 3); CHECK(nak(XF_ENONE), "delete, short id");
    { uint8_t q[8]; xf_wr32(q, 999); xf_wr32(q + 4, 0); send(0x07, q, 8); CHECK(nak(XF_ENONE), "get unknown");
      xf_wr32(q, lib_entry(0)->id); xf_wr32(q + 4, n + 1); send(0x07, q, 8); CHECK(nak(XF_ESIZE), "get past the end");
      xf_wr32(q + 4, n); send(0x07, q, 8); CHECK(rcmd == 0x47 && rlen == 4, "get at the end: empty");
      send(0x07, q, 4); CHECK(nak(XF_ENONE), "get, short"); }
    { uint8_t i1 = 1; send(0x02, &i1, 1); CHECK(nak(XF_ENONE), "list past the end"); send(0x02, p, 2); CHECK(nak(XF_EFRAME), "list, bad"); }
    broken = 1;                                 /* the flash refuses erases and writes */
    CHECK(put(pc, n) == 0x7F && nak(XF_EIO), "flash failing: EIO");
    send32(0x06, lib_entry(0)->id); CHECK(nak(XF_EIO), "delete on failing flash: EIO");
    broken = 0;
    { uint8_t big[600]; memset(big, 0x01, sizeof big); big[0] = 0x7D; big[1] = 0x46; big[2] = 0x31; big[3] = 0x04;
      CHECK(send_raw(big, sizeof big) && nak(XF_EFRAME), "too long a frame"); }
}

static void test_7_4_pack7_round_trip(void)
{
    uint8_t in[300], w[400], out[300];
    uint32_t n, i;
    for (n = 0; n < 300; n++) {
        for (i = 0; i < n; i++) in[i] = (uint8_t)(i * 37u + n);
        CHECK(xf_unpack7(w, xf_pack7(in, n, w), out, sizeof out) == (int32_t)n && !memcmp(in, out, n), "round trip %u", n);
        if (fails) break;
    }
    w[0] = 0; CHECK(xf_unpack7(w, 1, out, 10) == -1, "a group with no data");
    CHECK(xf_unpack7(w, xf_pack7(in, 20, w), out, 19) == -1, "longer than max");
}

int main(void)
{
    setvbuf(stdout, 0, _IONBF, 0);
    test_7_1_put_32k_then_get();
    test_7_2_staged_while_playing();
    test_7_4_hostile();
    test_7_4_pack7_round_trip();
    printf("%s\n", fails ? "FAILED" : "ok");
    return fails ? 1 : 0;
}
