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
