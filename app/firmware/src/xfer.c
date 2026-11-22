/* SPDX-License-Identifier: GPL-3.0-only */
/* xfer.c: the transfer protocol between fm1c (the Mac) and the library, over USB-MIDI SysEx.
 *
 * A frame: F0 7D 46 31 <cmd> <7-bit packed: payload, CRC-32 of the payload (LE)> F7. 7D is the manufacturer ID for
 * non-commercial use; "F1" marks ours, so the updater's frames (and anyone else's) pass by untouched.
 *
 *   cmd  request              reply
 *   01   INFO                 41  ver, count, free u32, room u32, flags (1 playing, 2 staged)
 *   02   LIST index           42  index, id u32, len u32, name[24]
 *   03   BEGIN len u32        43  len u32
 *   04   DATA off u32, bytes  44  off + n u32           (in order, up to 256 bytes a frame)
 *   05   END                  45  SAVED id u32  |  4F STAGED (playing: saved at the next stop, then 45 unasked)
 *   06   DEL id u32           46  id u32
 *   07   GET id u32, off u32  47  off u32, bytes        (up to 256)
 *   08   ABORT                48
 * (a reply is 0x40 | its request)
 *   any  -                    7F  NAK: request cmd, code (XF_E*)
 *
 * The piece goes into a RAM buffer first, is checked whole (piece_check), and only then is written to flash: never
 * while music plays (an erase silences the audio ~50 ms a sector), so a push during playback is staged. A put
 * idle for 5 s is dropped. Interface: xfer_frame (one received frame), xfer_poll (main loop), xfer_changed. */
#ifndef XFER_SEND
#define XFER_SEND(p, n) ota_wire_send(p, n)
#endif
#ifndef XF_POOL
#define XF_POOL __attribute__((section(".pool")))
#endif

#define XF_CHUNK 256u
#define XF_TIMEOUT_MS 5000u
enum { XF_EFRAME = 1, XF_ESEQ, XF_ESIZE, XF_ENOPUT, XF_ESHORT, XF_EPIECE, XF_EFULL, XF_EIO, XF_EBUSY, XF_ENONE, XF_ECMD, XF_ETIMEOUT };
enum { XS_IDLE, XS_RECV, XS_STAGED };

static uint8_t xf_buf[LIB_MAX] XF_POOL;         /* the piece on its way in */
static uint32_t xf_len, xf_got, xf_prev, xf_last_ms;   /* xf_prev: where the last chunk began */
static uint8_t xf_state, xf_expired;
static volatile uint8_t xf_changed;             /* the library changed: the UI re-reads it (and clears this) */

static uint32_t xf_rd32(const uint8_t *p) { return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24; }
static void xf_wr32(uint8_t *p, uint32_t v) { p[0] = (uint8_t)v; p[1] = (uint8_t)(v >> 8); p[2] = (uint8_t)(v >> 16); p[3] = (uint8_t)(v >> 24); }

/* 8-bit bytes -> groups of a high-bits byte and up to 7 low-7-bit bytes */
static uint32_t xf_pack7(const uint8_t *in, uint32_t n, uint8_t *out)
{
    uint32_t i, k, o = 0;
    for (i = 0; i < n; i += 7u) {
        uint32_t m = n - i < 7u ? n - i : 7u;
        uint8_t hi = 0;
        for (k = 0; k < m; k++) hi |= (uint8_t)((in[i + k] >> 7) << k);
        out[o++] = hi;
        for (k = 0; k < m; k++) out[o++] = in[i + k] & 0x7Fu;
    }
    return o;
}

/* the reverse; the decoded length, or -1 (a byte with bit 7 set, a group with no data, too long) */
static int32_t xf_unpack7(const uint8_t *in, uint32_t n, uint8_t *out, uint32_t max)
{
    uint32_t i = 0, k, o = 0;
    while (i < n) {
        uint32_t m = n - i - 1u < 7u ? n - i - 1u : 7u;
        uint8_t hi = in[i++];
        if (m == 0u || (hi & 0x80u) || o + m > max) return -1;
        for (k = 0; k < m; k++, i++) {
            if (in[i] & 0x80u) return -1;
            out[o++] = (uint8_t)(in[i] | ((hi >> k & 1u) << 7));
        }
    }
    return (int32_t)o;
}

static void xf_reply(uint32_t cmd, const uint8_t *pay, uint32_t n)
{
    static uint8_t raw[XF_CHUNK + 16u], wire[(XF_CHUNK + 16u) * 8u / 7u + 16u];
    uint32_t i, w;
    for (i = 0; i < n; i++) raw[i] = pay[i];
    xf_wr32(raw + n, lib_crc(0, raw, n));
    wire[0] = 0xF0; wire[1] = 0x7D; wire[2] = 0x46; wire[3] = 0x31; wire[4] = (uint8_t)cmd;
    w = 5u + xf_pack7(raw, n + 4u, wire + 5);
    wire[w++] = 0xF7;
    XFER_SEND(wire, w);
}

static void xf_nak(uint32_t cmd, uint32_t code)
{
    uint8_t p[2];
    p[0] = (uint8_t)cmd;
    p[1] = (uint8_t)code;
    xf_reply(0x7F, p, 2);
}

static void xf_commit(uint32_t cmd)             /* the whole piece is in xf_buf and checked: to flash */
{
    char name[PIECE_NAME + 1];
    uint32_t k;
    int32_t id;
    uint8_t p[4];
    for (k = 0; k < PIECE_NAME; k++) name[k] = (char)xf_buf[16u + k];
    name[PIECE_NAME] = 0;
    id = lib_put(name, xf_buf, xf_len);
    xf_state = XS_IDLE;
    if (id < 0) { xf_nak(cmd, id == LIB_FULL ? XF_EFULL : XF_EIO); return; }
    xf_changed = 1;
    xf_wr32(p, (uint32_t)id);
    xf_reply(0x45, p, 4);
}

/* one received SysEx frame (the bytes between F0 and F7): 1 = ours (handled), 0 = someone else's */
static int xfer_frame(const uint8_t *f, uint32_t n, uint32_t now_ms, int playing)
{
    static uint8_t d[XF_CHUNK + 16u];
    uint8_t r[8 + PIECE_NAME + 2];
    uint32_t cmd, len;
    int32_t dn;
    if (n < 4u || f[0] != 0x7Du || f[1] != 0x46u || f[2] != 0x31u) return 0;
    cmd = f[3];
    dn = xf_unpack7(f + 4, n - 4u, d, sizeof d);
    if (dn < 4 || lib_crc(0, d, (uint32_t)dn - 4u) != xf_rd32(d + dn - 4)) { xf_nak(cmd, XF_EFRAME); return 1; }
    len = (uint32_t)dn - 4u;
    switch (cmd) {
    case 0x01: {
        r[0] = 1;
        r[1] = (uint8_t)lib_count();
        xf_wr32(r + 2, lib_free());
        xf_wr32(r + 6, lib_room());
        r[10] = (uint8_t)((playing ? 1u : 0u) | (xf_state == XS_STAGED ? 2u : 0u));
        xf_reply(0x41, r, 11);
        break;
    }
    case 0x02: {
        const lib_entry_t *e = len == 1u ? lib_entry(d[0]) : 0;
        uint32_t k;
        if (!e) { xf_nak(cmd, len == 1u ? XF_ENONE : XF_EFRAME); break; }
        r[0] = d[0];
        xf_wr32(r + 1, e->id);
        xf_wr32(r + 5, e->len);
        for (k = 0; k < PIECE_NAME; k++) r[9 + k] = (uint8_t)e->name[k];
        xf_reply(0x42, r, 9u + PIECE_NAME);
        break;
    }
    case 0x03: {
        uint32_t want = len == 4u ? xf_rd32(d) : 0;
        if (xf_state == XS_STAGED) { xf_nak(cmd, XF_EBUSY); break; }
        if (want < PIECE_HDR || want > LIB_MAX) { xf_nak(cmd, XF_ESIZE); break; }
        xf_state = XS_RECV;
        xf_len = want;
        xf_got = xf_prev = 0;
        xf_last_ms = now_ms;
        xf_expired = 0;
        xf_reply(0x43, d, 4);
        break;
    }
    case 0x04: {
        uint32_t off = len >= 5u ? xf_rd32(d) : 0, k, cnt = len - 4u;
        if (xf_state != XS_RECV) { xf_nak(cmd, xf_expired ? XF_ETIMEOUT : XF_ENOPUT); break; }
        if (len < 5u) { xf_nak(cmd, XF_EFRAME); break; }
        if (xf_got && off == xf_prev && cnt == xf_got - off) {    /* the last chunk again (its answer was lost)? */
            for (k = 0; k < cnt && xf_buf[off + k] == d[4 + k]; k++) {}
            if (k == cnt) { xf_last_ms = now_ms; xf_wr32(r, xf_got); xf_reply(0x44, r, 4); break; }
        }
        if (off != xf_got) { xf_nak(cmd, XF_ESEQ); break; }
        if (cnt > xf_len - xf_got) { xf_nak(cmd, XF_ESIZE); break; }
        for (k = 0; k < cnt; k++) xf_buf[off + k] = d[4 + k];
        xf_prev = off;
        xf_got += cnt;
        xf_last_ms = now_ms;
        xf_wr32(r, xf_got);
        xf_reply(0x44, r, 4);
        break;
    }
    case 0x05:
        if (xf_state != XS_RECV) { xf_nak(cmd, xf_expired ? XF_ETIMEOUT : XF_ENOPUT); break; }
        if (xf_got != xf_len) { xf_nak(cmd, XF_ESHORT); break; }
        if (piece_check(xf_buf, xf_len) != PE_OK) { xf_state = XS_IDLE; xf_nak(cmd, XF_EPIECE); break; }
        if (playing) { xf_state = XS_STAGED; xf_reply(0x4F, d, 0); break; }
        xf_commit(cmd);
        break;
    case 0x06: {
        int rc = len == 4u ? lib_del(xf_rd32(d)) : LIB_NONE;
        if (rc == LIB_OK) { xf_changed = 1; xf_reply(0x46, d, 4); }
        else xf_nak(cmd, rc == LIB_EIO ? XF_EIO : XF_ENONE);
        break;
    }
    case 0x07: {
        int i = len == 8u ? lib_find(xf_rd32(d)) : LIB_NONE;
        uint32_t off = len == 8u ? xf_rd32(d + 4) : 0, cnt;
        static uint8_t out[4u + XF_CHUNK];
        if (i < 0) { xf_nak(cmd, XF_ENONE); break; }
        if (off > lib_entry((uint32_t)i)->len) { xf_nak(cmd, XF_ESIZE); break; }
        cnt = lib_entry((uint32_t)i)->len - off;
        if (cnt > XF_CHUNK) cnt = XF_CHUNK;
        xf_wr32(out, off);
        if (lib_pread((uint32_t)i, off, out + 4, cnt)) { xf_nak(cmd, XF_EIO); break; }
        xf_reply(0x47, out, 4u + cnt);
        break;
    }
    case 0x08:
        if (xf_state == XS_RECV) xf_state = XS_IDLE;
        xf_reply(0x48, d, 0);
        break;
    default:
        xf_nak(cmd, XF_ECMD);
        break;
    }
    return 1;
}

/* the main loop: a staged piece is saved once playback stops; an idle put is dropped */
static void xfer_poll(uint32_t now_ms, int playing)
{
    if (xf_state == XS_STAGED && !playing) xf_commit(0x05);
    if (xf_state == XS_RECV && now_ms - xf_last_ms > XF_TIMEOUT_MS) { xf_state = XS_IDLE; xf_expired = 1; }
}
