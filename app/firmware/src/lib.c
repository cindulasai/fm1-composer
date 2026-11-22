/* SPDX-License-Identifier: GPL-3.0-only */
/* lib.c: the library of pieces in flash, LIB_SECT sectors from LIB_AT (store.h). Survives a power cut at any
 * erase or write: an entry counts only once its header is written, and the header goes last.
 *
 * Each entry is a run of whole sectors: a 64-byte header, then the piece. Header: magic "LIB1", a CRC of the
 * header, id, seq, length, the data's CRC, name[24], and a "dead" word: 0xFFFFFFFF live, anything else deleted
 * (programmed in place, no erase: a delete is one short write). Mount reads every sector: a valid header with a
 * matching data CRC is an entry, an erased sector is free, anything else is "dirty" and erased when a put needs
 * it. A foreign store (another firmware's data) is all dirty: an empty library, nothing parsed, nothing erased
 * until a put.
 *
 * Interface: lib_mount, lib_count, lib_entry, lib_read, lib_pread, lib_put, lib_del, lib_find, lib_free, lib_room. Entries
 * are listed newest first. The flash calls are LIB_READ / LIB_ERASE / LIB_WRITE (the host tests' flash sim). */
#ifndef LIB_READ
#define LIB_READ(off, p, n) st_read(off, p, n)
#define LIB_ERASE(off) lib_erase_dev(off)
#define LIB_WRITE(off, p, n) fl_write(off, (const uint8_t *)(p), n)
static int lib_erase_dev(uint32_t off) { uint32_t took; fm1_wdt_feed(); return fl_erase4k(off, &took); }
#endif

#define LIB_SECTOR 4096u
#define LIB_HDR 64u
#define LIB_MAGIC 0x3142494Cu                   /* "LIB1" */
#define LIB_MAX PIECE_MAX_BYTES
#define LIB_SPAN_MAX ((LIB_HDR + LIB_MAX + LIB_SECTOR - 1u) / LIB_SECTOR)
enum { LIB_OK = 0, LIB_FULL = -1, LIB_EIO = -2, LIB_BAD = -3, LIB_NONE = -4 };
enum { LS_FREE, LS_DIRTY, LS_USED };

typedef struct {
    uint32_t id, seq, len;
    uint8_t first, span;
    char name[PIECE_NAME + 1];
} lib_entry_t;

typedef struct {                                /* the header as stored (little-endian words) */
    uint32_t magic, hcrc, id, seq, len, dcrc;
    char name[PIECE_NAME];
    uint32_t dead, rsv[3];
} lib_hdr_t;
_Static_assert(sizeof(lib_hdr_t) == LIB_HDR, "lib header is 64 bytes");

static lib_entry_t lib_ent[LIB_SECT];
static uint8_t lib_st[LIB_SECT];
static uint32_t lib_n, lib_seq;

static uint32_t lib_crc(uint32_t c, const uint8_t *p, uint32_t n)   /* start at 0; CRC-32 as piece.h, 4 bits a step */
{
    static const uint32_t T[16] = {0x00000000u, 0x1DB71064u, 0x3B6E20C8u, 0x26D930ACu, 0x76DC4190u, 0x6B6B51F4u,
                                   0x4DB26158u, 0x5005713Cu, 0xEDB88320u, 0xF00F9344u, 0xD6D6A3E8u, 0xCB61B38Cu,
                                   0x9B64C2B0u, 0x86D3D2D4u, 0xA00AE278u, 0xBDBDF21Cu};
    c = ~c;
    while (n--) {
        c ^= *p++;
        c = (c >> 4) ^ T[c & 15u];
        c = (c >> 4) ^ T[c & 15u];
    }
    return ~c;
}

static uint32_t lib_hcrc(const lib_hdr_t *h) { return lib_crc(0, (const uint8_t *)&h->id, (uint32_t)((const uint8_t *)&h->dead - (const uint8_t *)&h->id)); }
static uint32_t lib_at(uint32_t s) { return LIB_AT + s * LIB_SECTOR; }

/* the CRC of n bytes of flash from off; -1 on a read error */
static int lib_flash_crc(uint32_t off, uint32_t n, uint32_t *crc)
{
    uint8_t b[256];
    uint32_t c = 0;
    while (n) {
        uint32_t k = n > sizeof b ? (uint32_t)sizeof b : n;
        if (LIB_READ(off, b, k)) return -1;
        c = lib_crc(c, b, k);
        off += k;
        n -= k;
    }
    *crc = c;
    return 0;
}

static int lib_erased(uint32_t s)
{
    uint32_t b[64], i, off;
    for (off = 0; off < LIB_SECTOR; off += sizeof b) {
        if (LIB_READ(lib_at(s) + off, b, sizeof b)) return 0;
        for (i = 0; i < 64u; i++)
            if (b[i] != 0xFFFFFFFFu) return 0;
    }
    return 1;
}

static void lib_sort(void)                      /* newest (highest seq) first; insertion sort (73 at most) */
{
    uint32_t i, j;
    for (i = 1; i < lib_n; i++) {
        lib_entry_t e = lib_ent[i];
        for (j = i; j > 0 && lib_ent[j - 1].seq < e.seq; j--) lib_ent[j] = lib_ent[j - 1];
        lib_ent[j] = e;
    }
}

/* read the library from flash */
static void lib_mount(void)
{
    uint32_t s = 0, k;
    lib_n = 0;
    lib_seq = 0;
    while (s < LIB_SECT) {
        lib_hdr_t h;
        uint32_t span = 0, dc;
        if (!LIB_READ(lib_at(s), &h, sizeof h) && h.magic == LIB_MAGIC && h.hcrc == lib_hcrc(&h) && h.len >= 1u &&
            h.len <= LIB_MAX) {
            span = (LIB_HDR + h.len + LIB_SECTOR - 1u) / LIB_SECTOR;
            if (s + span > LIB_SECT) span = 0;
        }
        if (span && h.seq >= lib_seq) lib_seq = h.seq + 1u;
        if (span && h.dead == 0xFFFFFFFFu && !lib_flash_crc(lib_at(s) + LIB_HDR, h.len, &dc) && dc == h.dcrc) {
            lib_entry_t *e = &lib_ent[lib_n++];
            e->id = h.id;
            e->seq = h.seq;
            e->len = h.len;
            e->first = (uint8_t)s;
            e->span = (uint8_t)span;
            for (k = 0; k < PIECE_NAME; k++) e->name[k] = h.name[k] >= 32 && h.name[k] < 127 ? h.name[k] : 0;
            e->name[PIECE_NAME] = 0;
            for (k = 0; k < span; k++) lib_st[s + k] = LS_USED;
            s += span;
        } else if (span) {                      /* a deleted (or damaged) entry: its sectors wait for an erase */
            for (k = 0; k < span; k++) lib_st[s + k] = LS_DIRTY;
            s += span;
        } else {
            lib_st[s] = lib_erased(s) ? LS_FREE : LS_DIRTY;
            s++;
        }
    }
    lib_sort();
}

static uint32_t lib_count(void) { return lib_n; }
static const lib_entry_t *lib_entry(uint32_t i) { return i < lib_n ? &lib_ent[i] : 0; }

static int lib_find(uint32_t id)                /* the index of an id; LIB_NONE if absent */
{
    uint32_t i;
    for (i = 0; i < lib_n; i++)
        if (lib_ent[i].id == id) return (int)i;
    return LIB_NONE;
}

/* copy entry i into dst (max bytes): its length, or LIB_NONE / LIB_BAD (too big) / LIB_EIO (flash went bad) */
static int lib_read(uint32_t i, uint8_t *dst, uint32_t max)
{
    const lib_entry_t *e = lib_entry(i);
    if (!e) return LIB_NONE;
    if (e->len > max) return LIB_BAD;
    if (LIB_READ(lib_at(e->first) + LIB_HDR, dst, e->len)) return LIB_EIO;
    {
        lib_hdr_t h;
        if (LIB_READ(lib_at(e->first), &h, sizeof h) || lib_crc(0, dst, e->len) != h.dcrc) return LIB_EIO;
    }
    return (int)e->len;
}

/* n bytes of entry i from offset off (no CRC check: the piece carries its own): 0, or LIB_NONE / LIB_BAD / LIB_EIO */
static int lib_pread(uint32_t i, uint32_t off, uint8_t *dst, uint32_t n)
{
    const lib_entry_t *e = lib_entry(i);
    if (!e) return LIB_NONE;
    if (off > e->len || n > e->len - off) return LIB_BAD;
    return LIB_READ(lib_at(e->first) + LIB_HDR + off, dst, n) ? LIB_EIO : LIB_OK;
}

static uint32_t lib_free(void)                  /* bytes of sectors not in use */
{
    uint32_t s, n = 0;
    for (s = 0; s < LIB_SECT; s++) n += lib_st[s] != LS_USED;
    return n * LIB_SECTOR;
}

/* the start of a run of `span` sectors not in use: the run needing the fewest erases; -1 if none */
static int lib_fit(uint32_t span)
{
    uint32_t s, k;
    int best = -1;
    uint32_t best_dirty = 0;
    for (s = 0; s + span <= LIB_SECT; s++) {
        uint32_t dirty = 0;
        for (k = 0; k < span && lib_st[s + k] != LS_USED; k++) dirty += lib_st[s + k] == LS_DIRTY;
        if (k == span && (best < 0 || dirty < best_dirty)) { best = (int)s; best_dirty = dirty; }
    }
    return best;
}

static uint32_t lib_room(void)                  /* the biggest piece that fits now */
{
    uint32_t span;
    for (span = LIB_SPAN_MAX; span > 0; span--)
        if (lib_fit(span) >= 0) return span * LIB_SECTOR - LIB_HDR < LIB_MAX ? span * LIB_SECTOR - LIB_HDR : LIB_MAX;
    return 0;
}

/* store a piece: its new id (>= 0), or LIB_BAD (length), LIB_FULL (no room; nothing changed), LIB_EIO */
static int32_t lib_put(const char *name, const uint8_t *buf, uint32_t len)
{
    lib_hdr_t h;
    uint32_t span, k, dc;
    int s;
    if (len < 1u || len > LIB_MAX) return LIB_BAD;   /* (no more entries than sectors: lib_ent cannot overflow) */
    span = (LIB_HDR + len + LIB_SECTOR - 1u) / LIB_SECTOR;
    if ((s = lib_fit(span)) < 0) return LIB_FULL;
    for (k = 0; k < span; k++)
        if (lib_st[s + k] == LS_DIRTY) {
            if (LIB_ERASE(lib_at((uint32_t)s + k))) return LIB_EIO;
            lib_st[s + k] = LS_FREE;
        }
    for (k = 0; k < span; k++) lib_st[s + k] = LS_DIRTY;   /* written to from here on */
    __builtin_memset(&h, 0xFF, sizeof h);
    h.magic = LIB_MAGIC;
    h.id = lib_seq;
    h.seq = lib_seq;
    h.len = len;
    h.dcrc = lib_crc(0, buf, len);
    for (k = 0; k < PIECE_NAME; k++) h.name[k] = 0;
    for (k = 0; name && name[k] && k < PIECE_NAME; k++) h.name[k] = name[k] >= 32 && name[k] < 127 ? name[k] : '?';
    h.hcrc = lib_hcrc(&h);
    if (LIB_WRITE(lib_at((uint32_t)s) + LIB_HDR, buf, len)) return LIB_EIO;
    if (lib_flash_crc(lib_at((uint32_t)s) + LIB_HDR, len, &dc) || dc != h.dcrc) return LIB_EIO;   /* read back */
    if (LIB_WRITE(lib_at((uint32_t)s), &h, sizeof h)) return LIB_EIO;                            /* commit */
    lib_seq++;
    lib_mount();                                /* (re-read: the table is what flash says) */
    return lib_find(h.id) >= 0 ? (int32_t)h.id : LIB_EIO;
}

/* delete by id: LIB_OK, LIB_NONE, LIB_EIO */
static int lib_del(uint32_t id)
{
    static const uint32_t zero = 0;
    int i = lib_find(id);
    uint32_t k;
    if (i < 0) return LIB_NONE;
    if (LIB_WRITE(lib_at(lib_ent[i].first) + (uint32_t)__builtin_offsetof(lib_hdr_t, dead), &zero, 4u)) return LIB_EIO;
    for (k = 0; k < lib_ent[i].span; k++) lib_st[lib_ent[i].first + k] = LS_DIRTY;
    for (k = (uint32_t)i; k + 1u < lib_n; k++) lib_ent[k] = lib_ent[k + 1u];
    lib_n--;
    return LIB_OK;
}
