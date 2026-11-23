/* SPDX-License-Identifier: GPL-3.0-only */
/* sloop_glue.c: COMPOSER's glue around SLOOP's engines (SLOOP 2.8.5 @ bac9c05, copied unchanged into engines/:
 * firmware/SLOOP-PROVENANCE.md). Included by synth.c after the Felucca engines; nothing here is SLOOP's code.
 *
 * The engine pool: SLOOP keeps an engine's big per-part state (FM6: six notes and two buses, 1.7 KB; PHYS: its
 * voices' strings and modal banks, 8.5 KB) in a per-part arena (engines.c eng_arena_of). Here the blocks
 * (ENG_POOL_N, each the size of the largest state) are handed out on demand: a channel that picks a sound of such an
 * engine claims one (eng_claim, from synth_sound), gives it back when it picks another sound, and is refused when none
 * is free or its engine has its share (FM6: ENG_FM6_MAX channels, US-19's measured three); synth_sound then plays the
 * channel's role default, never a pool engine (tests 19.5, 20.3). eng_arena_of answers SLOOP's engines with the
 * channel's block, or 0 (they then stay silent and end their voices).
 *
 * FM6 (engines/eng_fm6.c + fm6_core.c): what eng_fm6.c expects of SLOOP's core.h, given here; SLOOP's ENG_FM6
 * (SLOOP's engine_t layout) is compiled as it is under another name and COMPOSER's ENG_FM6 points at the same
 * functions and presets. Not built: fm6_bank.c (SLOOP's user patch bank in SLOOP's flash objects; COMPOSER has no
 * bank: PTCH B1..B27 play the init voice, as an empty SLOOP bank does).
 *
 * PHYS, DIGITAL, VOICE (US-20: engines/eng_phys.c + phys_dsp.c + phys_symp.c, eng_digital.c, eng_formant.c): compiled
 * as they are with SLOOP's engine_t and preset_t layouts under other names; their presets are copied into COMPOSER's
 * preset_t at start (sloop_presets_init: name, EDIT values, ADSR, filter amount, MONO). Not played: a preset's extras
 * (XP: transpose, LFO, glide; COMPOSER's channels have none), its sends and pattern hint (COMPOSER's effects are the
 * piece's) and SLOOP's editor descriptors. PHYS keeps its voice slots in the channel's block (SYMP two voices, the
 * other models three: synth_sound caps the channel); every channel may play it at once (ENG_POOL_N = NPART blocks). */
#ifndef POOL
#define POOL
#endif
#ifndef ENG_POOL_N
#define ENG_POOL_N NPART                        /* every channel can hold a block (8 x PHYS: test 20.3) */
#endif
#define ENG_FM6_MAX 3u                          /* channels on FM6 at most (US-19: its device timing, test_19_6) */
#define ENG_POOL_BUDGET (72u * 1024u)           /* the engines' share of the 344 KB pool section (RAM) */
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
    const void *presets;                        /* (FM6's are in COMPOSER's preset_t, PHYS DIGITAL VOICE's in SLOOP's) */
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

/* ------------------------------------------ what PHYS, DIGITAL, VOICE need --- */
#define ENGI_PHYS 10u                           /* SLOOP core.h: PHYS's engine index (an arena owner id here) */
enum { P_LRATE = 9, P_LFADE = 12, P_LD_PIT = 13, P_LD_AMP = 16, P_TRANS = 28, P_GLIDE = 38 };   /* SLOOP core.h: the
                                                 * track parameters the presets' extras (XP) name; not played here */
typedef struct {                                /* SLOOP core.h preset_t, the layout the three engines' presets are in */
    const char *name;
    int8_t e[8];
    uint8_t env[4];
    int8_t fenv;
    uint8_t mono;
    uint8_t fx[4];
    uint8_t arp[4];
    uint8_t pat;
    int8_t x[8];
} sloop_preset_t;
#define XP(...) .x = {__VA_ARGS__}
static uint32_t sloop_rng_state = 0x1234567u;   /* SLOOP libc.c rng() (PHYS seeds a voice's noise from it): our own */
static uint32_t sloop_rng(void)
{
    sloop_rng_state ^= sloop_rng_state << 13;
    sloop_rng_state ^= sloop_rng_state >> 17;
    sloop_rng_state ^= sloop_rng_state << 5;
    return sloop_rng_state;
}

#include "sloop_tables.h"                       /* tools/gen_sloop_tables.py: ENV_LIN, ENV_EXP (SLOOP's gen_tables.py) */
#define engine_t sloop_engine_t
#define preset_t sloop_preset_t
#define rng sloop_rng
#define ENG_PHYS ENG_PHYS_SLOOP
#define ENG_DIGITAL ENG_DIGITAL_SLOOP
#define ENG_FORMANT ENG_FORMANT_SLOOP
#include "eng_phys.c"                           /* includes phys_dsp.c and phys_symp.c */
#include "eng_digital.c"
#include "eng_formant.c"
#undef ENG_PHYS
#undef ENG_DIGITAL
#undef ENG_FORMANT
#undef rng
#undef preset_t
#undef engine_t

/* the presets in COMPOSER's layout (sloop_presets_init, from synth_init): what COMPOSER's synth reads of a preset (its
 * sends and pattern hint are not: COMPOSER's effects are the piece's, per channel) */
static preset_t PHYS_PRE[NELEM(PHYS_PRESETS)], DIGITAL_PRE[NELEM(DIGITAL_PRESETS)], FORMANT_PRE[NELEM(FORMANT_PRESETS)];

static void sloop_presets(preset_t *d, const sloop_preset_t *s, uint32_t n)
{
    uint32_t i, k;
    for (i = 0; i < n; i++) {
        d[i].name = s[i].name, d[i].fenv = s[i].fenv, d[i].mono = s[i].mono;
        for (k = 0; k < 8u; k++) d[i].e[k] = s[i].e[k];
        for (k = 0; k < 4u; k++) d[i].env[k] = s[i].env[k];
    }
}

static void sloop_presets_init(void)
{
    sloop_presets(PHYS_PRE, PHYS_PRESETS, NELEM(PHYS_PRESETS));
    sloop_presets(DIGITAL_PRE, DIGITAL_PRESETS, NELEM(DIGITAL_PRESETS));
    sloop_presets(FORMANT_PRE, FORMANT_PRESETS, NELEM(FORMANT_PRESETS));
}

static const engine_t ENG_PHYS = {
    .name = "PHYS", .page_title = {"BODY", "EXCT"},
    .presets = PHYS_PRE, .npresets = NELEM(PHYS_PRESETS),
    .knob = {P_E1, P_E2, P_E3, P_E4}, .poly = PHYS_POLY,
    .note_on = phys_note_on, .render = phys_render, .desc = phys_desc, .mem = PHYS_ARENA,
};
static const engine_t ENG_DIGITAL = {
    .name = "DIGITAL", .page_title = {"OPS", "MOD"},
    .presets = DIGITAL_PRE, .npresets = NELEM(DIGITAL_PRESETS), .knob = {P_E4, P_E5, P_E6, P_REL},
    .note_on = digital_note_on, .render = digital_render,
};
static const engine_t ENG_FORMANT = {
    .name = "VOICE", .page_title = {"VOWL", "TONE"},
    .presets = FORMANT_PRE, .npresets = NELEM(FORMANT_PRESETS),
    .knob = {P_E0, P_E4, P_E5, P_E2}, .poly = 4,
    .note_on = formant_note_on, .render = formant_render,
};

/* ---------------------------------------------------------- the pool --- */
typedef union {
    fm6_part_t fm6;
    uint8_t phys[PHYS_ARENA];                   /* (eng_phys.c phys_slot_of: two or three voice slots) */
} eng_block_t;
static eng_block_t eng_pool[ENG_POOL_N] POOL;
static uint8_t eng_pool_own[ENG_POOL_N];        /* the engine id + 1 whose state a block holds, 0 = free */
static uint8_t eng_blk[NPART];                  /* a channel's block + 1, 0 = none */
static uint32_t eng_pool_n = ENG_POOL_N;        /* the blocks handed out (host tests lower it to fill the pool) */
_Static_assert(ENG_POOL_N < 255u && NPART < 255, "pool indices fit a byte");
_Static_assert(sizeof eng_pool <= ENG_POOL_BUDGET, "the engine pool fits its RAM budget");

/* the pool engine id of e, ENG_NONE for an engine that needs no block */
static uint32_t eng_id(const engine_t *e) { return e == &ENG_FM6 ? ENGI_FM6 : e == &ENG_PHYS ? ENGI_PHYS : ENG_NONE; }

static void eng_pool_reset(void)
{
    uint32_t i;
    for (i = 0; i < ENG_POOL_N; i++) eng_pool_own[i] = 0;
    for (i = 0; i < NPART; i++) eng_blk[i] = 0;
    eng_pool_n = ENG_POOL_N;
    sloop_rng_state = 0x1234567u;               /* (a fresh start renders the same: PHYS's and VOICE's noise) */
    formant_nz = 0x2545F491;
}

/* bytes of the pool in use (test 20.3) */
static uint32_t eng_pool_used(void)
{
    uint32_t i, n = 0;
    for (i = 0; i < ENG_POOL_N; i++) n += eng_pool_own[i] ? (uint32_t)sizeof eng_pool[i] : 0u;
    return n;
}

/* channel ch now plays engine eng: its block (zeroed) when eng needs one; 0 = done, -1 = refused: the pool is full or
 * eng has its share (the channel then holds none) */
static int eng_claim(uint32_t ch, uint32_t eng)
{
    uint32_t i, n = 0;
    if (ch >= NPART) return -1;
    if (eng_blk[ch]) { eng_pool_own[eng_blk[ch] - 1u] = 0; eng_blk[ch] = 0; }
    if (eng == ENG_NONE) return 0;
    for (i = 0; i < ENG_POOL_N; i++) n += eng_pool_own[i] == eng + 1u;
    if (eng == ENGI_FM6 && n >= ENG_FM6_MAX) return -1;
    for (i = 0; i < eng_pool_n && eng_pool_own[i]; i++) {}
    if (i >= eng_pool_n) return -1;
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
