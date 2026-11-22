/* SPDX-License-Identifier: GPL-3.0-only */
/* piece.h: the .f1p piece format (v1), read-only. Written by fm1c (tools/fm1c/piece.py); this is the reader.
 * Little-endian, read byte by byte (the chip faults on misaligned 16/32-bit loads).
 *   0  "F1P1"   4  version (1)   5  nch 1..8   6  flags (0)   8  tempo x10 (300..3000)   10  ppq (96)
 *   12 length in ticks (1..PIECE_MAX_TICKS)   16 name[24] printable ASCII, NUL padded, not empty
 *   40 total length (whole file, CRC included)   44 reserved (0)
 *   48 channels, 16 B each: role, sound (255 = the role's default), volume 0..127, pan 0..127, source MIDI
 *      channel 0..15, 3 reserved, event count u32, event offset u32
 *   events, 6 B each, in time order per channel: delta ticks u16, duration u16 (>= 1), pitch, velocity 1..127;
 *      pitch 255 = a rest (only the delta counts: gaps longer than 65535 ticks)
 *   last 4 B: CRC-32 (IEEE) of everything before it
 * piece_check() proves all of that once; piece_view() and piece_next() then never fail. */
#ifndef PIECE_H
#define PIECE_H
#include <stdint.h>

#define PIECE_MAX_BYTES 32768u
#define PIECE_PPQ 96u
#define PIECE_MAX_CH 8u
#define PIECE_MAX_TICKS (96u * 4u * 999u)
#define PIECE_HDR 48u
#define PIECE_CHREC 16u
#define PIECE_EV 6u
#define PIECE_REST 255u
#define PIECE_NAME 24u

enum { PE_OK, PE_SHORT, PE_MAGIC, PE_VERSION, PE_CRC, PE_NCH, PE_HEADER, PE_TABLE, PE_EVENT, PE_SIZE };
enum { ROLE_LEAD, ROLE_COUNTER, ROLE_CHORDS, ROLE_PAD, ROLE_BASS, ROLE_DRUMS, ROLE_PERC, ROLE_ARP, ROLE_DRONE,
       ROLE_FX, ROLE_OTHER, ROLE_COUNT };

typedef struct { uint8_t role, sound, vol, pan, src; uint32_t nev; const uint8_t *ev; } piece_ch_t;
typedef struct { char name[PIECE_NAME + 1]; uint16_t tempo_x10; uint32_t len_ticks; uint8_t nch; piece_ch_t ch[PIECE_MAX_CH]; } piece_view_t;
typedef struct { const uint8_t *p; uint32_t left, tick; } piece_cur_t;
typedef struct { uint32_t tick; uint16_t dur; uint8_t pitch, vel; } piece_note_t;

static uint32_t pc_u16(const uint8_t *p) { return (uint32_t)p[0] | (uint32_t)p[1] << 8; }
static uint32_t pc_u32(const uint8_t *p) { return pc_u16(p) | pc_u16(p + 2) << 16; }

static uint32_t piece_crc32(const uint8_t *p, uint32_t n)
{
    uint32_t c = 0xFFFFFFFFu, k;
    while (n--) {
        c ^= *p++;
        for (k = 0; k < 8u; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1u)));
    }
    return ~c;
}

static int piece_check(const uint8_t *b, uint32_t len)
{
    uint32_t nch, i, k, end;
    if (len > PIECE_MAX_BYTES) return PE_SIZE;
    if (len < PIECE_HDR + PIECE_CHREC + 4u) return PE_SHORT;
    if (b[0] != 'F' || b[1] != '1' || b[2] != 'P' || b[3] != '1') return PE_MAGIC;
    if (b[4] != 1u) return PE_VERSION;
    if (pc_u32(b + 40) != len) return PE_SHORT;
    if (piece_crc32(b, len - 4u) != pc_u32(b + len - 4u)) return PE_CRC;
    nch = b[5];
    if (nch < 1u || nch > PIECE_MAX_CH) return PE_NCH;
    if (pc_u16(b + 6) || pc_u32(b + 44)) return PE_HEADER;
    if (pc_u16(b + 8) < 300u || pc_u16(b + 8) > 3000u || pc_u16(b + 10) != PIECE_PPQ) return PE_HEADER;
    if (pc_u32(b + 12) < 1u || pc_u32(b + 12) > PIECE_MAX_TICKS) return PE_HEADER;
    if (b[16] == 0u) return PE_HEADER;
    for (k = 0, i = 0; i < PIECE_NAME; i++) {     /* printable, then only NULs */
        uint8_t c = b[16 + i];
        if (c == 0u) k = 1;
        else if (k || c < 32u || c > 126u) return PE_HEADER;
    }
    end = PIECE_HDR + nch * PIECE_CHREC;
    if (end + 4u > len) return PE_SHORT;
    for (i = 0; i < nch; i++) {
        const uint8_t *c = b + PIECE_HDR + i * PIECE_CHREC;
        uint32_t nev = pc_u32(c + 8), off = pc_u32(c + 12), t = 0, j;
        if (c[0] >= ROLE_COUNT || c[2] > 127u || c[3] > 127u || c[4] > 15u || c[5] || c[6] || c[7]) return PE_TABLE;
        if (off != end || nev > (len - 4u - end) / PIECE_EV) return PE_TABLE;   /* packed in order, no gaps */
        for (j = 0; j < nev; j++) {
            const uint8_t *e = b + off + j * PIECE_EV;
            t += pc_u16(e);
            if (t >= pc_u32(b + 12)) return PE_EVENT;
            if (e[4] == PIECE_REST) continue;
            if (e[4] > 127u || e[5] < 1u || e[5] > 127u || pc_u16(e + 2) < 1u) return PE_EVENT;
        }
        end = off + nev * PIECE_EV;
    }
    return end + 4u == len ? PE_OK : PE_TABLE;
}

static void piece_view(const uint8_t *b, piece_view_t *v)       /* only after piece_check() == PE_OK */
{
    uint32_t i;
    for (i = 0; i < PIECE_NAME; i++) v->name[i] = (char)b[16 + i];
    v->name[PIECE_NAME] = 0;
    v->tempo_x10 = (uint16_t)pc_u16(b + 8);
    v->len_ticks = pc_u32(b + 12);
    v->nch = b[5];
    for (i = 0; i < PIECE_MAX_CH; i++) {
        const uint8_t *c = b + PIECE_HDR + i * PIECE_CHREC;
        piece_ch_t *d = &v->ch[i];
        if (i >= v->nch) { d->role = ROLE_OTHER; d->sound = 255u; d->vol = d->pan = d->src = 0; d->nev = 0; d->ev = 0; continue; }
        d->role = c[0]; d->sound = c[1]; d->vol = c[2]; d->pan = c[3]; d->src = c[4];
        d->nev = pc_u32(c + 8); d->ev = b + pc_u32(c + 12);
    }
}

static void piece_cursor(const piece_ch_t *c, piece_cur_t *cur) { cur->p = c->ev; cur->left = c->nev; cur->tick = 0; }

static int piece_next(piece_cur_t *cur, piece_note_t *n)        /* 1 = a note in *n, 0 = the end; rests skipped */
{
    while (cur->left) {
        const uint8_t *e = cur->p;
        cur->p += PIECE_EV; cur->left--;
        cur->tick += pc_u16(e);
        if (e[4] == PIECE_REST) continue;
        n->tick = cur->tick; n->dur = (uint16_t)pc_u16(e + 2); n->pitch = e[4]; n->vel = e[5];
        return 1;
    }
    return 0;
}
#endif
