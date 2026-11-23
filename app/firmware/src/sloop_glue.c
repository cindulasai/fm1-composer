/* SPDX-License-Identifier: GPL-3.0-only */
/* sloop_glue.c: COMPOSER's glue around SLOOP's engines (SLOOP 2.8.5 @ bac9c05, copied unchanged into engines/:
 * firmware/SLOOP-PROVENANCE.md). Included by synth.c after the Felucca engines; nothing here is SLOOP's code.
 *
 * The engine pool: SLOOP keeps an engine's big per-part state (FM6: six notes and two buses, 1.7 KB) in a
 * per-part arena (engines.c eng_arena_of). COMPOSER has eight channels and little use for eight arenas, so a few
 * blocks (ENG_POOL_N) are handed out on demand: a channel that picks a sound of such an engine claims one
 * (eng_claim, from synth_sound), gives it back when it picks another sound, and is refused when none is free
 * (synth_sound then plays the channel's role default, never a pool engine: test 19.3). eng_arena_of answers
 * SLOOP's engines with the channel's block, or 0 (they then stay silent and end their voices). A later engine
 * (PHYS, ...) adds its state to eng_block_t and a line to eng_id.
 *
 * FM6 (engines/eng_fm6.c + fm6_core.c): what eng_fm6.c expects of SLOOP's core.h, given here; SLOOP's ENG_FM6
 * (SLOOP's engine_t layout) is compiled as it is under another name and COMPOSER's ENG_FM6 points at the same
 * functions and presets. Not built: fm6_bank.c (SLOOP's user patch bank in SLOOP's flash objects; COMPOSER has no
 * bank: PTCH B1..B27 play the init voice, as an empty SLOOP bank does). */
#ifndef POOL
#define POOL
#endif
#ifndef ENG_POOL_N
#define ENG_POOL_N 3u
#endif
#define ENG_NONE 0xFFu
static void *eng_arena_of(const track_t *t, uint32_t eng);

/* ------------------------------------------------- what eng_fm6.c needs --- */
#define ENGI_FM6 9u                             /* SLOOP core.h: FM6's engine index (an arena owner id here) */
enum { V_POLY, V_MONO, V_LEGATO, V_UNISON };    /* SLOOP core.h, P_VOICE (COMPOSER: always POLY) */
#ifndef RING_PUBLISH                            /* (platform.c's on the device; host tests: none needed, the patch is set
                                                 * in the audio interrupt itself, synth_sound) */
#define RING_PUBLISH()
#endif
#ifndef memcpy
#define memcpy __builtin_memcpy
#endif
typedef struct {                                /* SLOOP core.h engine_t, the layout eng_fm6.c's ENG_FM6 is written for */
    const char *name;
    const char *page_title[2];
    param_desc_t edit[8];
    const preset_t *presets;
    uint8_t npresets;
    int8_t fil_page;
    void (*note_on)(struct track *t, voice_t *v);
    void (*render)(struct track *t, voice_t *v, int32_t *out, uint32_t n, const vmod_t *m);
    uint16_t color;
    uint8_t macro[4];
    uint8_t poly;
    int32_t (*amp)(struct track *t, voice_t *v, int32_t adsr);
    const param_desc_t *(*desc)(const struct track *t, uint32_t k);
    void (*block)(struct track *t);
    uint8_t ownenv;
    int (*done)(struct track *t, voice_t *v);
} sloop_engine_t;

#include "fm6_tables.h"                         /* tools/gen_fm6_tables.py (SLOOP's gen_tables.py fm6_tables) */
#define engine_t sloop_engine_t
#define ENG_FM6 ENG_FM6_SLOOP
#include "eng_fm6.c"                            /* includes fm6_core.c and felucca_fm6.h */
#undef ENG_FM6
#undef engine_t

/* EDIT k of FM6: SLOOP's descriptors (COMPOSER has no engine editor; kept for the record) */
static const param_desc_t *fm6_desc(const struct track *t, uint32_t k)
{
    (void)t;
    return k < 8u ? &ENG_FM6_SLOOP.edit[k] : 0;
}

static const engine_t ENG_FM6 = {
    .name = "FM6", .page_title = {"OPS", "PATCH"},
    .presets = FM6_PRESETS, .npresets = NELEM(FM6_PRESETS),
    .knob = {P_E2, P_E3, P_E4, P_E7}, .poly = FM6_POLY,
    .note_on = fm6_note_on, .render = fm6_render, .desc = fm6_desc, .block = fm6_block,
    .ownenv = 1, .done = fm6_done, .mem = sizeof(fm6_part_t),
};

/* ---------------------------------------------------------- the pool --- */
typedef union {
    fm6_part_t fm6;
} eng_block_t;
static eng_block_t eng_pool[ENG_POOL_N] POOL;
static uint8_t eng_pool_own[ENG_POOL_N];        /* the engine id + 1 whose state a block holds, 0 = free */
static uint8_t eng_blk[NPART];                  /* a channel's block + 1, 0 = none */
_Static_assert(ENG_POOL_N < 255u && NPART < 255, "pool indices fit a byte");

/* the pool engine id of e, ENG_NONE for an engine that needs no block */
static uint32_t eng_id(const engine_t *e) { return e == &ENG_FM6 ? ENGI_FM6 : ENG_NONE; }

static void eng_pool_reset(void)
{
    uint32_t i;
    for (i = 0; i < ENG_POOL_N; i++) eng_pool_own[i] = 0;
    for (i = 0; i < NPART; i++) eng_blk[i] = 0;
}

/* channel ch now plays engine eng: its block (zeroed) when eng needs one; 0 = done, -1 = the pool is full (the
 * channel then holds none) */
static int eng_claim(uint32_t ch, uint32_t eng)
{
    uint32_t i;
    if (ch >= NPART) return -1;
    if (eng_blk[ch]) { eng_pool_own[eng_blk[ch] - 1u] = 0; eng_blk[ch] = 0; }
    if (eng == ENG_NONE) return 0;
    for (i = 0; i < ENG_POOL_N && eng_pool_own[i]; i++) {}
    if (i == ENG_POOL_N) return -1;
    __builtin_memset(&eng_pool[i], 0, sizeof eng_pool[i]);
    eng_pool_own[i] = (uint8_t)(eng + 1u);
    eng_blk[ch] = (uint8_t)(i + 1u);
    return 0;
}

/* SLOOP's engines: the channel's block, 0 when it holds none for eng */
static void *eng_arena_of(const track_t *t, uint32_t eng)
{
    uint32_t ch = (uint32_t)(t - trk), b;
    if (t < &trk[0] || ch >= NPART) return 0;
    b = eng_blk[ch];
    return b && eng_pool_own[b - 1u] == eng + 1u ? (void *)&eng_pool[b - 1u] : 0;
}

/* a sound of engine e was put on t (synth_sound, after its parameters): SLOOP's load path (FM6: the PTCH slot's patch) */
static void eng_loaded(track_t *t, const engine_t *e)
{
    t->eng_req = (uint8_t)eng_id(e);
    t->pold = t->xf_on = t->xf = 0;             /* (COMPOSER switches at once: no fade under way) */
    if (t->eng_req == ENGI_FM6) fm6_track_loaded(t);
}
