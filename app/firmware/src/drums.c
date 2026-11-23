/* SPDX-License-Identifier: GPL-3.0-only */
/* drums.c: COMPOSER's drum kit, an engine in Felucca-Engines' shape (engine_t, oneshot). General MIDI drum notes
 * (35..81) map to eleven synthesised instruments: kick, snare, clap, closed / open hat, toms, rim, crash, ride,
 * cowbell and a shaker for the rest. Its own decays (the amp hook); note-offs do nothing. Integers only.
 * EDIT: E0 tune (64 = as designed; +-4 semitones), E1 decay (64 = as designed), E2 tone (noise brightness),
 * E3 drive. Three kits as presets. */
enum { DK_KICK, DK_SNARE, DK_CLAP, DK_CHH, DK_OHH, DK_TOM, DK_RIM, DK_CRASH, DK_RIDE, DK_BELL, DK_SHAKE };

static uint8_t dk_type(uint32_t note)
{
    switch (note) {
    case 35: case 36: return DK_KICK;
    case 37: return DK_RIM;
    case 38: case 40: return DK_SNARE;
    case 39: return DK_CLAP;
    case 42: case 44: return DK_CHH;
    case 46: return DK_OHH;
    case 41: case 43: case 45: case 47: case 48: case 50: return DK_TOM;
    case 49: case 52: case 55: case 57: return DK_CRASH;
    case 51: case 53: case 59: return DK_RIDE;
    case 56: return DK_BELL;
    default: return DK_SHAKE;
    }
}

/* per instrument: decay a block (Q16, -60 dB in its time), start tone (phase inc), tone sweep a block (Q16),
 * noise share (Q15) */
static const struct { uint16_t k; uint32_t inc; uint16_t sweep; int16_t noise; } DK[] = {
    [DK_KICK] = {64519, 14608732u, 60289, 0},      /* 150 Hz falling to ~50, 320 ms */
    [DK_SNARE] = {63634, 17530478u, 60289, 22000}, /* 180 Hz body with noise, 170 ms */
    [DK_CLAP] = {63234, 0, 65535, 32767},           /* noise, 140 ms */
    [DK_CHH] = {58635, 0, 65535, 32767},            /* bright noise, 45 ms */
    [DK_OHH] = {64519, 0, 65535, 32767},            /* bright noise, 320 ms */
    [DK_TOM] = {64286, 0, 62000, 2000},             /* tone by note, 260 ms */
    [DK_RIM] = {55462, 77913238u, 65535, 6000},     /* 800 Hz click, 30 ms */
    [DK_CRASH] = {65284, 0, 65535, 32767},          /* noise, 1.3 s */
    [DK_RIDE] = {65172, 97391548u, 65535, 24000},   /* noise with a 1 kHz ping, 0.9 s */
    [DK_BELL] = {63738, 52591436u, 65535, 0},       /* 540 Hz, 800 Hz squares, 180 ms */
    [DK_SHAKE] = {61012, 0, 65535, 32767},          /* noise, 70 ms */
};

/* s[0] amplitude Q15, s[1] decay Q16, s[2] tone inc, s[3] sweep Q16, s[4] noise state, s[5] high-pass memory,
 * s[6] instrument, s[7] noise share Q15 */
static void dk_note_on(struct track *t, voice_t *v)
{
    const int16_t *p = t->p;
    uint32_t ty = dk_type(v->note), inc = DK[ty].inc;
    int32_t dec = p[P_E1] - 64;                     /* -64..63: shorter .. longer */
    uint32_t k = DK[ty].k;
    if (ty == DK_TOM) inc = pitch_inc((uint32_t)v->note * 16u);   /* GM toms 41..50: 87..147 Hz */
    if (inc) {
        int32_t tune = p[P_E0] - 64;                /* 1/16 semitone steps, +-4 semitones */
        inc = (uint32_t)((int64_t)inc * (4096 + tune * 15) >> 12);
    }
    k = (uint32_t)clamp((int32_t)k + dec * (int32_t)(65535u - k) / 80, 30000, 65535);
    v->s[0] = 32767;
    v->s[1] = (int32_t)k;
    v->s[2] = (int32_t)inc;
    v->s[3] = DK[ty].sweep;
    v->s[4] = (int32_t)(0x9E3779B9u ^ v->age * 2654435761u) | 1;
    v->s[5] = 0;
    v->s[6] = (int32_t)ty;
    v->s[7] = DK[ty].noise;
    v->ph[0] = v->ph[1] = 0;
}

static int32_t dk_amp(struct track *t, voice_t *v, int32_t adsr)
{
    int32_t a = v->s[0];
    (void)t; (void)adsr;
    v->s[0] = (int32_t)((uint32_t)v->s[0] * (uint32_t)v->s[1] >> 16);
    if (v->s[3] < 65535) v->s[2] = (int32_t)((uint64_t)(uint32_t)v->s[2] * (uint32_t)v->s[3] >> 16);
    if (v->s[2] && (uint32_t)v->s[2] < 4869577u && v->s[6] == DK_KICK) v->s[2] = 4869577;   /* the kick bottoms at 50 Hz */
    if (a < 8) { v->active = 0; v->stage = 0; v->gate = 0; return 0; }
    return a;
}

static void dk_render(struct track *t, voice_t *v, int32_t *out, uint32_t n, const vmod_t *m)
{
    const int16_t *p = t->p;
    uint32_t i, ty = (uint32_t)v->s[6], inc = (uint32_t)v->s[2];
    int32_t tone_share = 32767 - v->s[7], bright = p[P_E2], drive = p[P_E3];
    for (i = 0; i < n; i++) {
        int32_t nz = ((int32_t)(noise32(&v->s[4]) >> 16) - 32768) * 3 / 4, x;   /* +-24576 */
        int32_t tone = 0;
        if (ty == DK_CHH || ty == DK_OHH || ty == DK_CRASH || ty == DK_RIDE || ty == DK_SHAKE) {
            int32_t hp = nz - v->s[5];                 /* first difference: brighter */
            v->s[5] = nz;
            nz = (hp * (64 + bright) + nz * (127 - bright) / 2) >> 7;
        }
        if (inc) {
            if (ty == DK_BELL)
                tone = ((int32_t)v->ph[0] < 0 ? 18000 : -18000) + ((int32_t)v->ph[1] < 0 ? 14000 : -14000);
            else
                tone = sine_i(v->ph[0]);
            v->ph[0] += inc;
            v->ph[1] += inc + (inc >> 1) - (inc >> 4);   /* the bell's second tone, ~1.44x */
        }
        x = ((tone * tone_share) >> 15) + ((nz * v->s[7]) >> 15);
        if (drive) x = softclip(x * (16 + drive / 4) >> 4);
        out[i] += voice_amp(x, m, i);
    }
}

static const preset_t KIT_PRESETS[] = {
    {"STD KIT", {64, 64, 64, 0, 0, 0, 0, 0}, {0, 0, 127, 0}, 0, 0, FX(0, 0, 0, 0), PAT(0)},
    {"TIGHT KIT", {70, 30, 90, 8, 0, 0, 0, 0}, {0, 0, 127, 0}, 0, 0, FX(0, 0, 0, 0), PAT(0)},
    {"BOOM KIT", {56, 100, 40, 30, 0, 0, 0, 0}, {0, 0, 127, 0}, 0, 0, FX(0, 0, 0, 0), PAT(0)},
};

static const engine_t ENG_KIT = {
    .name = "KIT",
    .page_title = {"KIT", "KIT"},
    .edit = {{"TUNE", F_INT, 0, 127, 64, 0, 0}, {"DECAY", F_INT, 0, 127, 64, 0, 0}, {"TONE", F_INT, 0, 127, 64, 0, 0},
             {"DRIVE", F_INT, 0, 127, 0, 0, 0}},
    .presets = KIT_PRESETS,
    .npresets = NELEM(KIT_PRESETS),
    .oneshot = 1,
    .note_on = dk_note_on,
    .render = dk_render,
    .amp = dk_amp,
};

/* ------------------------------------------------ SLOOP's synthesised kits --- */
/* COMPOSER glue (US-21): SLOOP 2.8.5's 32 synthesised drum kits (drum_synth.c, felucca_drumkits.h from
 * tools/gen_drumkits.py, copied unchanged: SLOOP-PROVENANCE.md) as a second drum engine, appended to the sound
 * catalog after every other sound (the three kits above keep their ids and their sound). Its presets are the kits,
 * in SLOOP's order; GM notes 35..82 map as SLOOP maps them (DS_MAP: 36 kick, 38 snare, 42 closed hat, 46 open hat,
 * 49 crash, ...), any other note is silent.
 * Each voice's SLOOP state (dsv_t) lives beside the voice: dsyn_v[part][voice]. SLOOP's voice makes its own
 * envelopes and velocity; the synth's amplitude path only fades a stolen / stopped voice (v->s[0], halved each
 * block in stage 4), ramped over the block. EDIT: E0 tune (64 = as designed, +-4 semitones), E1 decay (SLOOP's
 * DECAY -4..+3, 64 = as designed); E2 holds the kit (the preset's, not shown). */
#include "drum_tables.h"                        /* DECAY_K (SLOOP's gen_tables.py; Felucca-Engines' tables lack it) */
static uint32_t dsyn_rng_state = 0x2545F491u;   /* SLOOP seeds each hit's noise from its rng(): our own here */
static uint32_t dsyn_rng(void)
{
    dsyn_rng_state ^= dsyn_rng_state << 13;
    dsyn_rng_state ^= dsyn_rng_state >> 17;
    dsyn_rng_state ^= dsyn_rng_state << 5;
    return dsyn_rng_state;
}
#define rng dsyn_rng
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wunused-variable"           /* (its user-kit bank and editor helpers: unused here) */
#pragma GCC diagnostic ignored "-Wunused-const-variable"
#include "drum_synth.c"
#pragma GCC diagnostic pop
#undef rng

#define DSYN_NOTE_LO 35u                        /* DS_MAP's notes; outside them, silence */
#define DSYN_NOTE_HI 82u
static dsv_t dsyn_v[NPART][NVOICE];

static dsv_t *dsyn_of(struct track *t, voice_t *v) { return &dsyn_v[t - trk][v - t->v]; }

static void dsyn_note_on(struct track *t, voice_t *v)
{
    const int16_t *p = t->p;
    uint32_t kit = (uint32_t)clamp(p[P_E2], 0, (int32_t)DS_NKITS - 1);
    if (v->note < DSYN_NOTE_LO || v->note > DSYN_NOTE_HI) {   /* outside the map: no voice */
        v->active = 0; v->stage = 0; v->gate = 0;
        return;
    }
    ds_on(dsyn_of(t, v), DS_KITS[kit].s, DS_KITS[kit].crush, v->note, v->vel, clamp(p[P_E0], 0, 127) - 64,
          (uint32_t)clamp(p[P_E1], 0, 127) >> 4);
    v->s[0] = v->s[1] = 32767;                  /* the fade: now, and at the end of the last block */
}

static int32_t dsyn_amp(struct track *t, voice_t *v, int32_t adsr)
{
    (void)t; (void)adsr;
    if (v->s[0] < 8) { v->active = 0; v->stage = 0; v->gate = 0; return 0; }
    return v->s[0];
}

static void dsyn_render(struct track *t, voice_t *v, int32_t *out, uint32_t n, const vmod_t *m)
{
    int32_t buf[CTL], f0 = v->s[1], f1 = v->s[0];
    uint32_t i;
    int alive;
    (void)m;
    if (n > CTL) n = CTL;
    alive = ds_render(dsyn_of(t, v), buf, n);
    v->s[1] = f1;
    if (f0 == 32767 && f1 == 32767)
        for (i = 0; i < n; i++) out[i] += buf[i];
    else
        for (i = 0; i < n; i++)
            out[i] += (int32_t)((int64_t)buf[i] * (f0 + ((f1 - f0) * (int32_t)(i + 1u) >> CTL_LOG2)) >> 15);
    if (!alive) { v->active = 0; v->stage = 0; v->gate = 0; }
}

#define DSYN_KIT(name, k) {name, {64, 64, k, 0, 0, 0, 0, 0}, {0, 0, 127, 0}, 0, 0, FX(0, 0, 0, 0), PAT(0)}
static const preset_t DSYN_PRESETS[] = {        /* SLOOP's kits: its order, its names (DS_KITS; test 21.1) */
    DSYN_KIT("808", 0), DSYN_KIT("909", 1), DSYN_KIT("606", 2), DSYN_KIT("80S", 3), DSYN_KIT("VINTAGE", 4),
    DSYN_KIT("TRAP", 5), DSYN_KIT("DRILL", 6), DSYN_KIT("BOOMBAP", 7), DSYN_KIT("LO-FI", 8), DSYN_KIT("PHONK", 9),
    DSYN_KIT("HOUSE", 10), DSYN_KIT("D.HOUSE", 11), DSYN_KIT("TECHNO", 12), DSYN_KIT("MINIMAL", 13),
    DSYN_KIT("ELECTRO", 14), DSYN_KIT("DISCO", 15), DSYN_KIT("GARAGE", 16), DSYN_KIT("JUNGLE", 17),
    DSYN_KIT("DUBSTEP", 18), DSYN_KIT("DEMBOW", 19), DSYN_KIT("AMAPIANO", 20), DSYN_KIT("AFRO", 21),
    DSYN_KIT("LATIN", 22), DSYN_KIT("TRIBAL", 23), DSYN_KIT("SYNTHWV", 24), DSYN_KIT("CHIP", 25),
    DSYN_KIT("ARCADE", 26), DSYN_KIT("GLITCH", 27), DSYN_KIT("INDUSTR", 28), DSYN_KIT("HYPER", 29),
    DSYN_KIT("AMBIENT", 30), DSYN_KIT("JAZZ", 31),
};
_Static_assert(NELEM(DSYN_PRESETS) == DS_NKITS, "a preset per SLOOP kit");

static const engine_t ENG_DSYN = {
    .name = "KIT",                              /* (the drum engine, to the player: KIT, like the three above) */
    .page_title = {"KIT", "KIT"},
    .edit = {{"TUNE", F_INT, 0, 127, 64, 0, 0}, {"DECAY", F_INT, 0, 127, 64, 0, 0}},
    .presets = DSYN_PRESETS,
    .npresets = NELEM(DSYN_PRESETS),
    .oneshot = 1,
    .note_on = dsyn_note_on,
    .render = dsyn_render,
    .amp = dsyn_amp,
};
