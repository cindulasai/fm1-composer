/* SPDX-License-Identifier: GPL-3.0-only */
/* prefs.c: each piece's choices (a sound per channel, mutes, tempo) by piece id; 200 pieces' worth, one blob.
 * A piece saved when all 200 are taken replaces the one saved longest ago. */
#define PREF_MAGIC 0x31464550u                  /* "PEF1" */
#define NPREF 200u
typedef struct { uint32_t id, stamp; uint16_t tempo_x10; uint8_t sound[PIECE_MAX_CH], mute, used; } pref_t;
static struct { uint32_t magic, stamp; pref_t p[NPREF]; } prefs;

static void prefs_reset(void)
{
    __builtin_memset(&prefs, 0, sizeof prefs);
    prefs.magic = PREF_MAGIC;
}

static const pref_t *pref_find(uint32_t id)
{
    uint32_t i;
    for (i = 0; i < NPREF; i++)
        if (prefs.p[i].used && prefs.p[i].id == id) return &prefs.p[i];
    return 0;
}

/* keep this piece's choices: its slot, else a free one, else the one saved longest ago */
static void pref_put(uint32_t id, uint32_t tempo_x10, const uint8_t *sound, uint32_t mute)
{
    pref_t *p = (pref_t *)pref_find(id);
    uint32_t i, ch;
    if (!p) {
        p = &prefs.p[0];
        for (i = 0; i < NPREF; i++) {
            if (!prefs.p[i].used) { p = &prefs.p[i]; break; }
            if (prefs.p[i].stamp < p->stamp) p = &prefs.p[i];
        }
    }
    p->id = id;
    p->used = 1;
    p->stamp = ++prefs.stamp;
    p->tempo_x10 = (uint16_t)tempo_x10;
    p->mute = (uint8_t)mute;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) p->sound[ch] = sound[ch];
    prefs.magic = PREF_MAGIC;
}

/* each piece's effects (fx.c: REVERB .. DRIVE per channel), by piece id, in blob 1: more entries than the library can
 * hold pieces (73 of 4 KB at most); the one saved longest ago gives way. A value out of range reads as its maximum. */
#define FXP_MAGIC 0x31505846u                   /* "FXP1" */
#define NFXP 75u
#define PREF_FX 6u                               /* effects a channel (fx.c FX_N) */
typedef struct { uint32_t id; uint16_t stamp; uint8_t v[PIECE_MAX_CH][PREF_FX]; } __attribute__((packed)) fxpref_t;
static struct { uint32_t magic, stamp; fxpref_t p[NFXP]; } fxprefs;

static void fxprefs_reset(void)
{
    __builtin_memset(&fxprefs, 0, sizeof fxprefs);
    fxprefs.magic = FXP_MAGIC;
}

static const fxpref_t *fxpref_find(uint32_t id)
{
    uint32_t i;
    for (i = 0; i < NFXP; i++)
        if (fxprefs.p[i].stamp && fxprefs.p[i].id == id) return &fxprefs.p[i];
    return 0;
}

static void fxpref_put(uint32_t id, const uint8_t (*v)[PREF_FX])
{
    fxpref_t *p = (fxpref_t *)fxpref_find(id);
    uint32_t i;
    if (!p) {
        p = &fxprefs.p[0];
        for (i = 0; i < NFXP; i++) {
            if (!fxprefs.p[i].stamp) { p = &fxprefs.p[i]; break; }
            if (fxprefs.p[i].stamp < p->stamp) p = &fxprefs.p[i];
        }
    }
    if (fxprefs.stamp >= 0xFFFEu) {             /* (16-bit stamps: renumbered from 1, oldest first, before they wrap) */
        uint32_t lo = 0xFFFFu, k;
        for (k = 0; k < NFXP; k++) if (fxprefs.p[k].stamp && fxprefs.p[k].stamp < lo) lo = fxprefs.p[k].stamp;
        for (k = 0; k < NFXP; k++) if (fxprefs.p[k].stamp) fxprefs.p[k].stamp = (uint16_t)(fxprefs.p[k].stamp - lo + 1u);
        fxprefs.stamp = 0;
        for (k = 0; k < NFXP; k++) if (fxprefs.p[k].stamp > fxprefs.stamp) fxprefs.stamp = fxprefs.p[k].stamp;
    }
    p->id = id;
    p->stamp = (uint16_t)++fxprefs.stamp;
    __builtin_memcpy(p->v, v, sizeof p->v);
    fxprefs.magic = FXP_MAGIC;
}
