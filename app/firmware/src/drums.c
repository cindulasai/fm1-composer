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
