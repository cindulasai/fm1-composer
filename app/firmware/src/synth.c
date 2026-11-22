/* SPDX-License-Identifier: GPL-3.0-only */
/* synth.c: COMPOSER's sound. Eight channels, each a Felucca-Engines part (track_t) with up to NVOICE voices; at
 * most VOICE_BUDGET voices sound at once across all channels (the chip's audio budget), the rest are stolen:
 * first a released voice anywhere, then the oldest voice of the same channel, then the oldest anywhere. A stolen
 * voice fades out over a few blocks (no click). Sounds: every preset of the six Felucca engines plus the drum
 * kits (drums.c); each text2midi role has a default sound.
 *
 * Interface (synth_*): the audio interrupt calls synth_render(); notes come from the player in the same
 * interrupt. The main loop changes sounds and levels through synth_post() (a single-producer queue the next
 * render applies), never directly. Host tests call the direct functions.
 * Integers only (docs/RULES.md). */
#ifdef FS
#undef FS                                       /* (the host defines it too; same value) */
#endif
#define NVOICE 8
#define NPART 8
#ifndef VOICE_BUDGET
#define VOICE_BUDGET 16
#endif
#include "fe_tables.h"
#include "felucca_engine.h"
enum { P_ATK, P_REL, P_FTYPE, P_E0, P_E1, P_E2, P_E3, P_E4, P_E5, P_E6, P_E7, P_COUNT };
typedef struct track { int16_t p[P_COUNT]; voice_t v[NVOICE]; } track_t;
static track_t trk[NPART];
#include "dsp.c"
#include "eng_analog.c"
#include "eng_phase.c"
#include "eng_lofi.c"
#include "eng_trio.c"
#include "eng_wheel.c"
#include "eng_noise.c"
#include "drums.c"
#include "synth_tables.h"
#include "piece.h"

/* ---------------------------------------------------------------- the sounds --- */
static const engine_t *const ENGINES[] = {&ENG_KIT, &ENG_ANALOG, &ENG_PHASE, &ENG_TRIO, &ENG_WHEEL, &ENG_LOFI, &ENG_NOISE};
#define SOUND_MAX 128u
typedef struct { const engine_t *e; const preset_t *pr; } sound_t;
static sound_t sounds[SOUND_MAX];
static uint32_t sound_count;

static void sounds_init(void)
{
    uint32_t i, k;
    sound_count = 0;
    for (i = 0; i < NELEM(ENGINES); i++)
        for (k = 0; k < ENGINES[i]->npresets && sound_count < SOUND_MAX; k++) {
            sounds[sound_count].e = ENGINES[i];
            sounds[sound_count].pr = &ENGINES[i]->presets[k];
            sound_count++;
        }
}

static int snd_eq(const char *a, const char *b)
{
    while (*a && *a == *b) a++, b++;
    return *a == *b;
}

/* a sound by engine and preset name; sound_count when there is none */
static uint32_t sound_find(const char *engine, const char *preset)
{
    uint32_t i;
    for (i = 0; i < sound_count; i++)
        if (snd_eq(sounds[i].e->name, engine) && snd_eq(sounds[i].pr->name, preset)) return i;
    return sound_count;
}

/* the default sound of a role (piece.h ROLE_*) */
static const char *const ROLE_SOUND[ROLE_COUNT][2] = {
    [ROLE_LEAD] = {"ANALOG", "SAW LEAD"},  [ROLE_COUNTER] = {"TRIO", "SYNC LEAD"}, [ROLE_CHORDS] = {"ANALOG", "SINE KEY"},
    [ROLE_PAD] = {"ANALOG", "SOFT PAD"},    [ROLE_BASS] = {"ANALOG", "SQR BASS"},   [ROLE_DRUMS] = {"KIT", "STD KIT"},
    [ROLE_PERC] = {"KIT", "TIGHT KIT"},     [ROLE_ARP] = {"LOFI", "ARP 8BIT"},      [ROLE_DRONE] = {"ANALOG", "STRINGS"},
    [ROLE_FX] = {"NOISE", "RAIN"},          [ROLE_OTHER] = {"ANALOG", "PLUCK"},
};

static uint32_t sound_default(uint32_t role)
{
    uint32_t s;
    if (role >= ROLE_COUNT) role = ROLE_OTHER;
    s = sound_find(ROLE_SOUND[role][0], ROLE_SOUND[role][1]);
    return s;                                   /* (every default exists: test 4.3) */
}

static const char *sound_name(uint32_t id) { return id < sound_count ? sounds[id].pr->name : "?"; }
static const char *sound_engine(uint32_t id) { return id < sound_count ? sounds[id].e->name : "?"; }

/* -------------------------------------------------------------- the channels --- */
typedef struct {
    const engine_t *e;
    const preset_t *pr;
    uint32_t sound;
    int32_t gl, gr;                             /* Q15 gains: volume x pan */
    uint8_t vol, pan, cap;
} chan_t;
static chan_t chs[NPART];
static int32_t vlast[NPART][NVOICE];            /* each voice's amplitude at the end of its last block (Q15) */
static uint32_t vage;

static void chan_gains(chan_t *c)
{
    c->gl = (int32_t)VOL_Q15[c->vol] * PAN_L[c->pan] >> 15;
    c->gr = (int32_t)VOL_Q15[c->vol] * PAN_R[c->pan] >> 15;
}

static void synth_sound(uint32_t ch, uint32_t id)
{
    chan_t *c;
    track_t *t;
    uint32_t k;
    if (ch >= NPART || id >= sound_count) return;
    c = &chs[ch];
    t = &trk[ch];
    c->sound = id;
    c->e = sounds[id].e;
    c->pr = sounds[id].pr;
    c->cap = c->pr->mono ? 1u : (c->e->poly && c->e->poly < NVOICE) ? c->e->poly : (uint8_t)NVOICE;
    for (k = 0; k < NVOICE; k++) vlast[ch][k] = 0;
    __builtin_memset(t, 0, sizeof *t);
    for (k = 0; k < 8u; k++) t->p[P_E0 + k] = c->pr->e[k];
    t->p[P_ATK] = c->pr->env[0];
    t->p[P_REL] = c->pr->env[3];
}

static void synth_level(uint32_t ch, uint32_t vol, uint32_t pan)
{
    if (ch >= NPART) return;
    chs[ch].vol = (uint8_t)(vol > 127u ? 127u : vol);
    chs[ch].pan = (uint8_t)(pan > 127u ? 127u : pan);
    chan_gains(&chs[ch]);
}

static void synth_init(void)
{
    uint32_t ch;
    sounds_init();
    vage = 0;
    for (ch = 0; ch < NPART; ch++) {
        synth_sound(ch, sound_default(ROLE_OTHER));
        synth_level(ch, 100, 64);
    }
}

/* voices that count against the budget: starting or held or releasing (a stolen voice, stage 4, does not) */
#ifndef VCHK
#define VCHK(k) (k)                            /* (host tests check every voice index) */
#endif
#define SOUNDING(v) ((v).active && (v).stage >= 1u && (v).stage <= 3u)
static int sounding(const voice_t *v) { return SOUNDING(*v); }

static uint32_t synth_active(void)
{
    uint32_t ch, k, n = 0;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < NVOICE; k++) n += (uint32_t)SOUNDING(trk[ch].v[VCHK(k)]);
    return n;
}

static uint32_t synth_count(uint32_t ch)        /* sounding voices of one channel */
{
    uint32_t k, n = 0;
    if (ch >= NPART) return 0;
    for (k = 0; k < NVOICE; k++) n += (uint32_t)SOUNDING(trk[ch].v[VCHK(k)]);
    return n;
}

/* the voice to give up when over budget: a released one anywhere (oldest), else the oldest of channel `pref`,
 * else the oldest anywhere; 0 when nothing sounds */
static voice_t *steal_pick(uint32_t pref)
{
    voice_t *best = 0;
    uint32_t ch, k, pass;
    for (pass = 0; pass < 3u && !best; pass++)
        for (ch = 0; ch < NPART; ch++) {
            if (pass == 1u && ch != pref) continue;
            for (k = 0; k < NVOICE; k++) {
                voice_t *v = &trk[ch].v[VCHK(k)];
                if (!sounding(v) || (pass == 0u && v->stage != 3u)) continue;
                if (!best || v->age < best->age) best = v;
            }
        }
    return best;
}

static void voice_start(uint32_t ch, voice_t *v, uint32_t note, uint32_t vel)
{
    track_t *t = &trk[ch];
    const engine_t *e = chs[ch].e;
    int live = v->active && v->stage != 0u && v->stage != 4u;
    uint32_t ph0 = v->ph[0], ph1 = v->ph[1], ph2 = v->ph[2], k;
    int32_t s[8];
    for (k = 0; k < 8u; k++) s[k] = v->s[k];
    if (!v->active) {                           /* a fresh voice: its engine state from zero */
        for (k = 0; k < 8u; k++) v->s[k] = 0;
        v->ph[0] = v->ph[1] = v->ph[2] = 0;
        v->env = v->env_out = 0;
        vlast[ch][v - t->v] = 0;
    }
    v->note = (uint8_t)note;
    v->vel = (uint8_t)vel;
    v->gate = 1;
    v->active = 1;
    v->stage = 1;
    v->age = ++vage;
    v->pitch16 = v->pitch_cur = (int32_t)note * 16;
    v->fine = 0;
    if (!live) v->env = 0;
    e->note_on(t, v);
    if (live && !e->oneshot) {                  /* a retrigger keeps the phases, the engine's keep slots */
        v->ph[0] = ph0; v->ph[1] = ph1; v->ph[2] = ph2;
        for (k = 0; k < 8u; k++)
            if (k != 2u && k != 3u && (e->keep >> k & 1u)) v->s[k] = s[k];
    }
}

static void synth_on(uint32_t ch, uint32_t note, uint32_t vel)
{
    track_t *t;
    voice_t *v = 0;
    uint32_t k, cap;
    if (ch >= NPART || note > 127u || vel > 127u) return;
    if (vel == 0u) return;                      /* (a note-on at velocity 0 is the player's note-off) */
    t = &trk[ch];
    cap = chs[ch].cap;
    if (cap == 1u) v = &t->v[0];                /* MONO: one voice, retriggered */
    for (k = 0; k < cap && !v; k++)             /* the same note again: retrigger it */
        if (sounding(&t->v[VCHK(k)]) && t->v[VCHK(k)].note == note) v = &t->v[VCHK(k)];
    for (k = 0; k < cap && !v; k++)             /* a free voice */
        if (!t->v[VCHK(k)].active) v = &t->v[VCHK(k)];
    for (k = 0; k < cap && !v; k++)             /* a fading one */
        if (t->v[VCHK(k)].stage == 4u) v = &t->v[VCHK(k)];
    if (!v) {                                   /* the channel is full: its oldest, a released one first */
        v = &t->v[0];
        for (k = 1; k < cap; k++) {
            voice_t *c = &t->v[VCHK(k)];
            int crel = c->stage == 3u, vrel = v->stage == 3u;
            if (crel != vrel ? crel : c->age < v->age) v = c;
        }
    }
    if (!sounding(v)) {                         /* a voice that does not sound yet: is there room? */
        while (synth_active() >= VOICE_BUDGET) {
            voice_t *s = steal_pick(ch);
            if (!s) break;
            s->stage = 4u;                      /* fade out (env_tick), off the budget */
            s->gate = 0;
        }
    }
    voice_start(ch, v, note, vel);
}

static void synth_off(uint32_t ch, uint32_t note)
{
    uint32_t k;
    if (ch >= NPART || note > 127u || chs[ch].e->oneshot) return;
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[ch].v[VCHK(k)];
        if (v->active && v->gate && v->note == note) { v->gate = 0; v->stage = 3u; }
    }
}

static void synth_ch_off(uint32_t ch)           /* release every held note of a channel (mute, stop) */
{
    uint32_t k;
    if (ch >= NPART) return;
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[ch].v[VCHK(k)];
        if (v->active && v->stage != 4u) { v->gate = 0; v->stage = chs[ch].e->oneshot ? 4u : 3u; }
    }
}

static void synth_hush(void)                    /* every voice fades out within a few ms (stop) */
{
    uint32_t ch, k;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < NVOICE; k++)
            if (trk[ch].v[VCHK(k)].active) { trk[ch].v[VCHK(k)].stage = 4u; trk[ch].v[VCHK(k)].gate = 0; }
}

static void synth_all_off(void)
{
    uint32_t ch;
    for (ch = 0; ch < NPART; ch++) synth_ch_off(ch);
}

/* the envelope, once a block: Q24 in v->env, returns Q15 */
static int32_t env_tick(uint32_t ch, voice_t *v)
{
    const preset_t *pr = chs[ch].pr;
    int32_t sus = (int32_t)pr->env[2] << 17;    /* 0..127 -> Q24 */
    switch (v->stage) {
    case 1:
        v->env += ENV_ATK[pr->env[0] & 127u];
        if (v->env >= 1 << 24) { v->env = 1 << 24; v->stage = 2; }
        break;
    case 2:
        v->env = sus + (int32_t)((int64_t)(v->env - sus) * ENV_K[pr->env[1] & 127u] >> 16);
        break;
    case 3:
        v->env = (int32_t)((int64_t)v->env * ENV_K[pr->env[3] & 127u] >> 16);
        if (v->env < (1 << 24) / 2000) { v->env = 0; v->stage = 0; v->active = 0; v->gate = 0; }
        break;
    case 4:
        v->env >>= 1;
        if (v->env < (1 << 24) / 2000) { v->env = 0; v->stage = 0; v->active = 0; v->gate = 0; }
        break;
    default:
        v->active = 0;
        break;
    }
    return v->env >> 9;
}

/* add n (<= CTL) stereo frames to out (L R L R ...) */
static void synth_render(int32_t *out, uint32_t n)
{
    static int32_t mixl[CTL], mixr[CTL], buf[CTL];
    uint32_t ch, k, i;
    if (n > CTL) n = CTL;
    for (i = 0; i < n; i++) mixl[i] = mixr[i] = 0;
    for (ch = 0; ch < NPART; ch++) {
        track_t *t = &trk[ch];
        const chan_t *c = &chs[ch];
        const engine_t *e = c->e;
        int any = 0;
        if (!e) continue;                       /* (the audio may start before synth_init: silence) */
        if (e->block) e->block(t);
        for (i = 0; i < n; i++) buf[i] = 0;
        for (k = 0; k < NVOICE; k++) {
            voice_t *v = &t->v[VCHK(k)];
            vmod_t m;
            int32_t a;
            uint32_t base;
            if (!v->active) continue;
            if (e->oneshot) {
                a = e->amp(t, v, 0);
                if (v->stage == 4u) { v->s[0] >>= 1; }
            } else {
                a = env_tick(ch, v);
                if (e->amp) a = e->amp(t, v, a);
            }
            if (!v->active) { vlast[ch][k] = 0; continue; }
            a = a * VEL_Q15[v->vel] >> 15;
            base = pitch_inc((uint32_t)clamp(v->pitch16, 0, 2047));
            m.inc = base;
            m.pitch16 = v->pitch16;
            m.fine = 0;
            m.amp0 = vlast[ch][k];
            m.amp1 = a;
            m.envq15 = v->env >> 9;
            m.cutoff = (int32_t)c->pr->fenv * (v->env >> 9) >> 7;   /* the filter envelope: +-64 steps << 8 */
            m.shape = 64 << 8;
            vlast[ch][k] = a;
            e->render(t, v, buf, n, &m);
            any = 1;
        }
        if (!any) continue;
        for (i = 0; i < n; i++) {
            mixl[i] += buf[i] * c->gl >> 15;
            mixr[i] += buf[i] * c->gr >> 15;
        }
    }
    for (i = 0; i < n; i++) {                   /* headroom for 16 voices, then a soft knee, then the hard limit */
        int32_t l = out[2 * i] + softclip(mixl[i] >> 1), r = out[2 * i + 1] + softclip(mixr[i] >> 1);
        out[2 * i] = clamp(l, -32767, 32767);
        out[2 * i + 1] = clamp(r, -32767, 32767);
    }
}

/* ---------------------------------------------- the main loop's changes --- */
enum { SC_SOUND, SC_LEVEL, SC_CH_OFF, SC_ALL_OFF, SC_NOTE_ON, SC_NOTE_OFF };
typedef struct { uint8_t op, ch, a, b; } synth_cmd_t;
static synth_cmd_t sq[32];
static volatile uint32_t sq_w, sq_r;            /* one writer each: main loop sq_w, audio sq_r */

static int synth_post(uint32_t op, uint32_t ch, uint32_t a, uint32_t b)   /* 0 = queued, -1 = full */
{
    uint32_t w = sq_w;
    if (w - sq_r >= 32u) return -1;
    sq[w % 32u].op = (uint8_t)op; sq[w % 32u].ch = (uint8_t)ch; sq[w % 32u].a = (uint8_t)a; sq[w % 32u].b = (uint8_t)b;
    sq_w = w + 1u;
    return 0;
}

static void synth_apply(void)                   /* in the audio interrupt, before rendering */
{
    while (sq_r != sq_w) {
        const synth_cmd_t *c = &sq[sq_r % 32u];
        switch (c->op) {
        case SC_SOUND: synth_sound(c->ch, c->a); break;
        case SC_LEVEL: synth_level(c->ch, c->a, c->b); break;
        case SC_CH_OFF: synth_ch_off(c->ch); break;
        case SC_ALL_OFF: synth_all_off(); break;
        case SC_NOTE_ON: synth_on(c->ch, c->a, c->b); break;
        case SC_NOTE_OFF: synth_off(c->ch, c->a); break;
        default: break;
        }
        sq_r++;
    }
}
