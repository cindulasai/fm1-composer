/* SPDX-License-Identifier: GPL-3.0-only */
/* synth.c: COMPOSER's sound. Eight channels, each a Felucca-Engines part (track_t) with up to NVOICE voices; at
 * most VOICE_BUDGET voices sound at once across all channels (the chip's audio budget), the rest are stolen:
 * first a released voice anywhere, then the oldest voice of the same channel, then the oldest anywhere. A stolen
 * voice fades out over a few blocks (no click). A channel plays POLY, MONO or LEGATO with glide (US-30: SLOOP's voice code;
 * synth_voice: the piece's mode, AUTO = the sound's SLOOP mode + glide); POLY is v8's allocation as it was.
 * Live notes (US-24: the keys in KEYS mode, MIDI IN; synth_live_on / _off)
 * take at most LIVE_VOICES voices; while one sounds the piece keeps to VOICE_BUDGET - LIVE_VOICES and never takes a live
 * voice, nor ends one (synth_off, synth_ch_off: the piece's notes only). Sounds: every preset of the six Felucca engines plus the drum
 * kits (drums.c), then SLOOP's FM6, PHYS, DIGITAL and VOICE presets (sloop_glue.c: appended, so ids 0-39 keep their v4
 * meaning); each
 * text2midi role has a default sound.
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
#define pitch_frac mrnd                         /* (US-30) SLOOP's glide fraction (voice_t: Felucca's layout kept; mrnd is unused) */
enum { P_ATK, P_REL, P_FTYPE, P_E0, P_E1, P_E2, P_E3, P_E4, P_E5, P_E6, P_E7, P_VOICE, P_ROOT,
       P_GLIDE, P_GLMODE, P_PRIO, P_DETUNE, P_COUNT };   /* (P_VOICE, P_ROOT: SLOOP's, 0: POLY, C; PHYS SYMP's ROOT chord
                                                 * reads it. US-30: P_VOICE (V_POLY..), P_GLIDE, P_GLMODE as SLOOP's; P_PRIO
                                                 * LAST, P_DETUNE unused: no UNISON) */
typedef struct track {
    int16_t p[P_COUNT];
    voice_t v[NVOICE];
    uint8_t eng_req, pold, xf_on, xf;           /* SLOOP's engine of the part and its fade (eng_fm6.c reads them; sloop_glue.c) */
    uint8_t mono_stack[8], nmono, mono_note;    /* (US-30) SLOOP core.h: MONO / LEGATO's held keys, the note sounding */
    uint8_t slide_glide, slide_next;            /* the next note glides (SLOOP's); a SLIDE note sounds: the next one will */
} track_t;
static track_t trk[NPART];
#include "dsp.c"
#include "eng_analog.c"
#include "eng_phase.c"
#include "eng_lofi.c"
#include "eng_trio.c"
#include "eng_wheel.c"
#include "eng_noise.c"
#include "drums.c"
#include "sloop_glue.c"                        /* SLOOP's FM6, PHYS, DIGITAL, VOICE and the engine pool */
#include "synth_tables.h"
#include "piece.h"

/* ---------------------------------------------------------------- the sounds --- */
static const engine_t *const ENGINES[] = {&ENG_KIT, &ENG_ANALOG, &ENG_PHASE, &ENG_TRIO, &ENG_WHEEL, &ENG_LOFI, &ENG_NOISE,
                                          &ENG_DSYN, &ENG_FM6, &ENG_PHYS, &ENG_DIGITAL, &ENG_FORMANT};   /* (new engines go last:
                                                 * saved sound ids keep their meaning) */
#define SOUND_MAX 192u                          /* (a piece stores a sound in a byte, 255 = the role's default) */
typedef struct { const engine_t *e; const preset_t *pr; } sound_t;
static sound_t sounds[SOUND_MAX];
static uint32_t sound_count;

static void sounds_init(void)
{
    uint32_t i, k;
    sloop_presets_init();
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
static uint8_t ch_role[NPART];                  /* each channel's role (piece.h ROLE_*): its sound when the pool is full */
static uint8_t ch_vm[NPART], ch_vg[NPART];      /* (US-30) each channel's voice mode (VM_*) and glide, as the piece asks */
static int32_t vlast[NPART][NVOICE];            /* each voice's amplitude at the end of its last block (Q15) */
#include "fx.c"                                 /* SLOOP's effects: per channel, the buses, the limiter */
static uint32_t vage;

static void chan_gains(chan_t *c)
{
    c->gl = (int32_t)VOL_Q15[c->vol] * PAN_L[c->pan] >> 15;
    c->gr = (int32_t)VOL_Q15[c->vol] * PAN_R[c->pan] >> 15;
}

/* the channel's voice cap and its mode as played: VM_POLY is v8's (the preset's MONO a one-voice cap), AUTO what
 * SLOOP gives the sound (sloop_voice), MONO / LEGATO one voice; a kit (one-shot) always POLY, no glide */
static void voice_apply(uint32_t ch)
{
    chan_t *c = &chs[ch];
    track_t *t = &trk[ch];
    int16_t m = V_POLY, g = (int16_t)ch_vg[ch], gm = 0;
    c->cap = c->pr->mono ? 1u : (c->e->poly && c->e->poly < NVOICE) ? c->e->poly : (uint8_t)NVOICE;
    if (c->e == &ENG_PHYS && c->cap > phys_poly(t)) c->cap = (uint8_t)phys_poly(t);   /* (SYMP: two voices, SLOOP voice.c) */
    if (ch_vm[ch] == VM_AUTO) sloop_voice(c->e, c->pr, &m, &g, &gm);
    else if (ch_vm[ch] == VM_MONO) m = V_MONO;
    else if (ch_vm[ch] == VM_LEGATO) m = V_LEGATO;
    if (c->e->oneshot) m = V_POLY, g = 0;
    if (m != V_POLY) c->cap = 1u;
    t->p[P_VOICE] = m;
    t->p[P_GLIDE] = g;
    t->p[P_GLMODE] = gm;
}

static void synth_sound(uint32_t ch, uint32_t id)
{
    chan_t *c;
    track_t *t;
    uint32_t k;
    if (ch >= NPART || id >= sound_count) return;
    if (eng_claim(ch, eng_id(sounds[id].e)))    /* no engine block free: the role's default (never a pool engine) */
        id = sound_default(ch_role[ch]);
    c = &chs[ch];
    t = &trk[ch];
    c->sound = id;
    c->e = sounds[id].e;
    c->pr = sounds[id].pr;
    for (k = 0; k < NVOICE; k++) vlast[ch][k] = 0;
    __builtin_memset(t, 0, sizeof *t);
    for (k = 0; k < 8u; k++) t->p[P_E0 + k] = c->pr->e[k];
    t->p[P_ATK] = c->pr->env[0];
    t->p[P_REL] = c->pr->env[3];
    voice_apply(ch);
    eng_loaded(t, c->e);
}

/* (SC_VOICE) channel ch's voice: mode (piece.h VM_*; another value: POLY), glide 0..127; what sounds goes on */
static void synth_voice(uint32_t ch, uint32_t mode, uint32_t glide)
{
    if (ch >= NPART) return;
    ch_vm[ch] = (uint8_t)(mode < VM_N ? mode : VM_POLY);
    ch_vg[ch] = (uint8_t)(glide > PIECE_MAX_GLIDE ? PIECE_MAX_GLIDE : glide);
    voice_apply(ch);
}

static void synth_role(uint32_t ch, uint32_t role)
{
    if (ch < NPART) ch_role[ch] = (uint8_t)(role < ROLE_COUNT ? role : ROLE_OTHER);
}

static void synth_level(uint32_t ch, uint32_t vol, uint32_t pan)
{
    if (ch >= NPART) return;
    chs[ch].vol = (uint8_t)(vol > 127u ? 127u : vol);
    chs[ch].pan = (uint8_t)(pan > 127u ? 127u : pan);
    chan_gains(&chs[ch]);
}

/* a channel's effect (FX_REV .. FX_DRIVE), clamped to its range */
static void synth_fx(uint32_t ch, uint32_t par, uint32_t v)
{
    if (ch < NPART && par < FX_N) fxp[ch][par] = (uint8_t)fx_clamp(par, v);
}
static void synth_tempo(uint32_t x10) { fx_tempo_x10 = x10 < 300u ? 300u : x10 > 3000u ? 3000u : x10; }   /* the delay */

static void synth_init(void)
{
    uint32_t ch;
    sounds_init();
    eng_pool_reset();
    fm6_init();
    vage = 0;
    for (ch = 0; ch < NPART; ch++) {
        ch_role[ch] = ROLE_OTHER;
        ch_vm[ch] = VM_POLY;
        ch_vg[ch] = 0;
        synth_sound(ch, sound_default(ROLE_OTHER));
        synth_level(ch, 100, 64);
    }
    fx_reset();
}

/* voices that count against the budget: starting or held or releasing (a stolen voice, stage 4, does not) */
#ifndef VCHK
#define VCHK(k) (k)                            /* (host tests check every voice index) */
#endif
#define SOUNDING(v) ((v).active && (v).stage >= 1u && (v).stage <= 3u)
static int sounding(const voice_t *v) { return SOUNDING(*v); }

#define LIVE_VOICES 4u
static uint8_t vlive[NPART];                    /* bit k: trk[ch].v[k] plays a live note */
static int live_v(uint32_t ch, uint32_t k) { return (vlive[ch] >> k & 1u) && SOUNDING(trk[ch].v[VCHK(k)]); }
static uint32_t synth_live(void)                /* live voices sounding */
{
    uint32_t ch, k, n = 0;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < NVOICE; k++) n += (uint32_t)live_v(ch, k);
    return n;
}

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
 * else the oldest anywhere; never a live one; 0 when nothing (else) sounds */
static voice_t *steal_pick(uint32_t pref)
{
    voice_t *best = 0;
    uint32_t ch, k, pass;
    for (pass = 0; pass < 3u && !best; pass++)
        for (ch = 0; ch < NPART; ch++) {
            if (pass == 1u && ch != pref) continue;
            for (k = 0; k < NVOICE; k++) {
                voice_t *v = &trk[ch].v[VCHK(k)];
                if (!sounding(v) || (pass == 0u && v->stage != 3u) || live_v(ch, k)) continue;
                if (!best || v->age < best->age) best = v;
            }
        }
    return best;
}

/* ---- SLOOP firmware/src/voice.c @ bac9c05 lines 255-281, unchanged (US-30) ---- */
/* glide, in 1/4096 semitone a control tick (pitch_cur . pitch_frac): RATE = GLIDE is the time an octave
 * takes (any interval at that speed), TIME = GLIDE is the time of any glide. A sequencer SLIDE with
 * GLIDE 0 glides in SLIDE_T (TIME), as an acid slide */
#define SLIDE_T 54                                       /* TIME_MS index: ~50 ms */
static int32_t glide_ticks(int32_t g)                    /* control ticks of TIME_MS index g */
{
    int32_t k = (int32_t)ENV_LIN[g & 127];
    return k ? (int32_t)((1u << 24) / (uint32_t)k) : 1;
}
static void glide_set(track_t *t, voice_t *v, int glide)
{
    int32_t d, ticks;
    if (!glide || !v->pitch_cur) {
        v->pitch_cur = v->pitch16;
        v->pitch_frac = 0;
        v->gstep = 0;
        return;
    }
    if (t->p[P_GLMODE] || !t->p[P_GLIDE]) {
        d = (v->pitch16 << 8) - ((v->pitch_cur << 8) + v->pitch_frac);
        d = d < 0 ? -d : d;
        ticks = glide_ticks(t->p[P_GLIDE] ? t->p[P_GLIDE] : SLIDE_T);
        v->gstep = d / ticks > 0 ? d / ticks : 1;
    } else {
        v->gstep = 0;
    }
}
/* ---- end of SLOOP's code ---- */

/* a voice starts (or a sounding one again: its phases and the engine's kept state stay); glide: from its last pitch */
static void voice_start(track_t *t, voice_t *v, uint32_t note, uint32_t vel, int glide)
{
    uint32_t ch = (uint32_t)(t - trk);
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
    v->pitch16 = (int32_t)note * 16;
    glide_set(t, v, glide);                     /* (glide 0: pitch_cur = pitch16, as before US-30) */
    v->fine = 0;
    if (!live) v->env = 0;
    e->note_on(t, v);
    if (live && !e->oneshot) {                  /* a retrigger keeps the phases, the engine's keep slots */
        v->ph[0] = ph0; v->ph[1] = ph1; v->ph[2] = ph2;
        for (k = 0; k < 8u; k++)
            if (k != 2u && k != 3u && (e->keep >> k & 1u)) v->s[k] = s[k];
    }
}

/* (US-30) what SLOOP's mono_play needs: room for a voice that does not sound yet (COMPOSER's budget: synth_on's
 * steal), always given for MONO's one voice; no UNISON (SPEC decision 48), so one voice and no spread */
static void voice_room_make(uint32_t ch)
{
    uint32_t live = synth_live();               /* (while a live note sounds, 4 of the 16 are kept for them) */
    while (synth_active() >= VOICE_BUDGET || (live && synth_active() - live >= VOICE_BUDGET - LIVE_VOICES)) {
        voice_t *s = steal_pick(ch);
        if (!s) break;
        s->stage = 4u;                          /* fade out (env_tick), off the budget */
        s->gate = 0;
    }
}
static int voice_room(track_t *t, int soft) { (void)soft; voice_room_make((uint32_t)(t - trk)); return 1; }
#define trk_nvoice(t) 1u
#define unison_spread(t, v, r) ((void)0)
#define pend side                               /* (SLOOP's pending note-on of a sample voice: never; side is unused) */
#define VP_ON 1u
#define VP_UNI 8u

/* ---- SLOOP firmware/src/voice.c @ bac9c05 lines 390-447, unchanged ---- */
/* MONO / LEGATO / UNISON: one note on one voice (eight for UNISON, or the
 * engine's voice cap, spread by DETUNE over the same width) */
static void mono_play(track_t *t, uint32_t note, uint32_t vel, int retrig, int glide)
{
    uint32_t mode = (uint32_t)t->p[P_VOICE], nv = mode == V_UNISON ? trk_nvoice(t) : 1u, i;
    for (i = 0; i < nv; i++) {
        voice_t *v = &t->v[i];
        int32_t k = 2 * (int32_t)i - (int32_t)(nv - 1u);   /* -7 .. 7 */
        if (nv > 1u && nv < NVOICE)
            k = k * (NVOICE - 1) / (int32_t)(nv - 1u);      /* fewer voices: the outer ones as wide */
        v->fine = nv > 1u ? k * t->p[P_DETUNE] * 56 / 889 : 0;   /* up to ~±40 cents */
        if (retrig || !v->active || !v->gate) {
            int was_sounding = v->active && v->stage != 0;
            if ((!v->active || v->stage == 4u) && !voice_room(t, i > 0))
                continue;                                   /* no room for this extra UNISON voice */
            /* level: about the same sum for 8 or 4 voices at random phases */
            voice_start(t, v, note, nv >= NVOICE ? vel * 36u / 100u : nv > 1u ? vel / 2u : vel, glide);
            if (nv > 1u && i && !was_sounding) {   /* random start phases (SAMPLE: its start; not SLICE): */
                static uint32_t seed = 0x1234567u;          /* in phase they stack, evenly spread they cancel */
                seed = seed * 1664525u + 1013904223u;
                unison_spread(t, v, seed);
            } else if (nv > 1u && i && (v->pend & VP_ON))
                v->pend |= VP_UNI;                          /* (a sample's, after its fade: spread as it starts) */
        } else {                                            /* legato: new pitch, same envelope */
            v->note = (uint8_t)note;
            v->pitch16 = (int32_t)note * 16;
            if (vel > v->vel && nv == 1u)
                v->vel = (uint8_t)vel;
            glide_set(t, v, glide);
        }
    }
    t->mono_note = (uint8_t)note;
}

/* the note the mono stack asks for (PRIO: LAST / LOW / HIGH), 0 = none */
static uint32_t mono_pick(const track_t *t)
{
    uint32_t i, n;
    if (!t->nmono)
        return 0;
    n = t->mono_stack[t->nmono - 1u];
    for (i = 0; i < t->nmono; i++) {
        if (t->p[P_PRIO] == 1 && t->mono_stack[i] < n)
            n = t->mono_stack[i];
        if (t->p[P_PRIO] == 2 && t->mono_stack[i] > n)
            n = t->mono_stack[i];
    }
    return n;
}

static void mono_remove(track_t *t, uint32_t note)
{
    uint32_t j, k = 0;
    for (j = 0; j < t->nmono; j++)
        if (t->mono_stack[j] != note)
            t->mono_stack[k++] = t->mono_stack[j];
    t->nmono = (uint8_t)k;
}
/* ---- end of SLOOP's code ---- */
#undef pend

/* MONO / LEGATO: a note of the piece; SLOOP's trk_note_on, its mono part (voice.c lines 496-509, unchanged) */
static void mono_on(track_t *t, uint32_t note, uint32_t vel)
{
    uint32_t mode = (uint32_t)t->p[P_VOICE];
    {
        uint32_t sounding = t->mono_note && t->v[0].active && t->v[0].gate, want;
        mono_remove(t, note);
        if (t->nmono >= 8u)
            mono_remove(t, t->mono_stack[0]);
        t->mono_stack[t->nmono++] = (uint8_t)note;
        want = mono_pick(t);
        if (sounding && want == t->mono_note)
            return;                                     /* LOW / HIGH priority: this key does not win */
        if (mode == V_MONO)                             /* MONO: always retrigger, glide when GLIDE is set */
            mono_play(t, want, vel, 1, t->p[P_GLIDE] != 0 || t->slide_glide);
        else                                            /* LEGATO / UNISON: glide only between held notes */
            mono_play(t, want, vel, !sounding, sounding && (t->p[P_GLIDE] != 0 || t->slide_glide));
    }
}

/* SLOOP's trk_note_off, its mono part (voice.c lines 536-553, unchanged) */
static void mono_off(track_t *t, uint32_t note)
{
    uint32_t i, mode = (uint32_t)t->p[P_VOICE];
    mono_remove(t, note);                               /* in any mode: a key let go after a VOICE change must
                                                         * not stay in the MONO stack (after Felucca 1.0) */
    if (mode != V_POLY) {
        uint32_t nv = mode == V_UNISON ? trk_nvoice(t) : 1u;
        if (t->mono_note == note) {
            uint32_t next = mono_pick(t);
            if (next) {                                 /* fall back to a held note, legato */
                mono_play(t, next, t->v[0].vel, 0, t->p[P_GLIDE] != 0);
            } else {
                for (i = 0; i < nv; i++)
                    if (t->v[i].gate) {
                        t->v[i].gate = 0;
                        t->v[i].stage = 3;
                    }
                t->mono_note = 0;
            }
        }
    }
}

/* the glide, once a block (SLOOP's track_render, voice.c lines 850-858, unchanged) */
static void glide_tick(const track_t *t, voice_t *v)
{
    const int16_t *p = t->p;
        if (v->pitch_cur != v->pitch16 || v->pitch_frac) {   /* glide, in 1/4096 semitone */
            int32_t q = (v->pitch_cur << 8) + v->pitch_frac, d = (v->pitch16 << 8) - q;
            int32_t st = v->gstep ? v->gstep : (int32_t)((ENV_LIN[p[P_GLIDE] & 127] * 3u) >> 10);   /* RATE: an octave in GLIDE */
            if (st < 1)
                st = 1;
            q += d > 0 ? (d < st ? d : st) : (-d < st ? d : -st);
            v->pitch_cur = q >> 8;
            v->pitch_frac = (uint8_t)(q & 255);
        }
}

/* a SLIDE note just started on channel ch (the player holds it into the next note): the next note glides into */
static void synth_slide(uint32_t ch) { if (ch < NPART) trk[ch].slide_next = 1; }

static void synth_on(uint32_t ch, uint32_t note, uint32_t vel)
{
    track_t *t;
    voice_t *v = 0;
    uint32_t k, cap;
    if (ch >= NPART || note > 127u || vel > 127u) return;
    if (vel == 0u) return;                      /* (a note-on at velocity 0 is the player's note-off) */
    t = &trk[ch];
    cap = chs[ch].cap;
    if (t->p[P_VOICE] != V_POLY) {              /* MONO / LEGATO (SLOOP's); not while its voice plays a live note */
        t->slide_glide = (uint8_t)(t->slide_next && t->mono_note && t->v[0].gate);
        t->slide_next = 0;
        if (!live_v(ch, 0)) mono_on(t, note, vel);
        return;
    }
    if (cap == 1u) {                            /* MONO: one voice, retriggered; not while it plays a live note */
        if (live_v(ch, 0)) return;
        v = &t->v[0];
    }
    for (k = 0; k < cap && !v; k++)             /* the same note again: retrigger it */
        if (sounding(&t->v[VCHK(k)]) && t->v[VCHK(k)].note == note && !live_v(ch, k)) v = &t->v[VCHK(k)];
    for (k = 0; k < cap && !v; k++)             /* a free voice */
        if (!t->v[VCHK(k)].active) v = &t->v[VCHK(k)];
    for (k = 0; k < cap && !v; k++)             /* a fading one */
        if (t->v[VCHK(k)].stage == 4u) v = &t->v[VCHK(k)];
    if (!v) {                                   /* the channel is full: its oldest, a released one first; not a live one */
        for (k = 0; k < cap; k++) {
            voice_t *c = &t->v[VCHK(k)];
            int crel = c->stage == 3u, vrel = v && v->stage == 3u;
            if (live_v(ch, k)) continue;
            if (!v || (crel != vrel ? crel : c->age < v->age)) v = c;
        }
        if (!v) return;                         /* every voice of the channel plays a live note */
    }
    if (!sounding(v)) voice_room_make(ch);      /* a voice that does not sound yet: is there room? */
    t->nmono = 0;                               /* (SLOOP: no stale mono stack after a mode change) */
    t->mono_note = 0;
    t->slide_next = 0;
    vlive[ch] &= (uint8_t)~(1u << (v - t->v));
    voice_start(t, v, note, vel, t->p[P_GLIDE] != 0);
}

static void synth_off(uint32_t ch, uint32_t note)   /* a note of the piece ends (not a live one) */
{
    uint32_t k;
    track_t *t;
    if (ch >= NPART || note > 127u || chs[ch].e->oneshot) return;
    t = &trk[ch];
    mono_off(t, note);                          /* (MONO / LEGATO: a held key takes over, legato) */
    for (k = 0; k < NVOICE; k++) {              /* (SLOOP: and POLY voices, also ones left from another mode) */
        voice_t *v = &t->v[VCHK(k)];
        if (v->active && v->gate && v->note == note && !(vlive[ch] >> k & 1u) &&
            (t->p[P_VOICE] == V_POLY || k >= 1u || t->mono_note != note)) { v->gate = 0; v->stage = 3u; }
    }
}

static void synth_live_off(uint32_t ch, uint32_t note)
{
    uint32_t k;
    if (ch >= NPART || note > 127u || chs[ch].e->oneshot) return;
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[ch].v[VCHK(k)];
        if (v->active && v->gate && v->note == note && (vlive[ch] >> k & 1u)) { v->gate = 0; v->stage = 3u; }
    }
}

/* a live note on channel ch, in its sound: at most LIVE_VOICES at once (a 5th: the oldest gives way); a voice of the
 * piece makes room for it, never another live one while the channel has one of the piece's */
static void synth_live_on(uint32_t ch, uint32_t note, uint32_t vel)
{
    track_t *t;
    voice_t *v = 0;
    uint32_t k, c, cap;
    if (ch >= NPART || note > 127u || vel > 127u) return;
    if (vel == 0u) { synth_live_off(ch, note); return; }
    t = &trk[ch];
    cap = chs[ch].cap;
    for (k = 0; k < cap && !v; k++)             /* the same live note again: retriggered */
        if (live_v(ch, k) && t->v[VCHK(k)].note == note) v = &t->v[VCHK(k)];
    if (!v && synth_live() >= LIVE_VOICES) {    /* a 5th: the oldest live note fades out */
        voice_t *o = 0;
        uint32_t oc = 0, ok = 0;
        for (c = 0; c < NPART; c++)
            for (k = 0; k < NVOICE; k++)
                if (live_v(c, k) && (!o || trk[c].v[VCHK(k)].age < o->age)) { o = &trk[c].v[VCHK(k)]; oc = c; ok = k; }
        o->stage = 4u;
        o->gate = 0;
        vlive[oc] &= (uint8_t)~(1u << ok);
    }
    for (k = 0; k < cap && !v; k++)             /* a free voice, else a fading one */
        if (!t->v[VCHK(k)].active) v = &t->v[VCHK(k)];
    for (k = 0; k < cap && !v; k++)
        if (t->v[VCHK(k)].stage == 4u) v = &t->v[VCHK(k)];
    for (c = 0; c < 2u && !v; c++)              /* the piece's oldest on the channel (a released one first), else the
                                                 * oldest live one (a channel of at most 4 voices, all live) */
        for (k = 0; k < cap; k++) {
            voice_t *x = &t->v[VCHK(k)];
            int xrel = x->stage == 3u, vrel = v && v->stage == 3u;
            if ((uint32_t)live_v(ch, k) != c) continue;
            if (!v || (xrel != vrel ? xrel : x->age < v->age)) v = x;
        }
    if (!sounding(v))
        while (synth_active() >= VOICE_BUDGET) {
            voice_t *s = steal_pick(ch);
            if (!s) break;
            s->stage = 4u;
            s->gate = 0;
        }
    if (v == &t->v[0]) t->nmono = t->mono_note = 0;   /* (US-30: MONO's voice is the key's now, the piece's note gone) */
    vlive[ch] |= (uint8_t)(1u << (v - t->v));
    voice_start(t, v, note, vel, 0);
}

static void synth_ch_off(uint32_t ch)           /* release every held note of the piece on a channel (mute) */
{
    uint32_t k;
    if (ch >= NPART) return;
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[ch].v[VCHK(k)];
        if (v->active && v->stage != 4u && !(vlive[ch] >> k & 1u)) { v->gate = 0; v->stage = chs[ch].e->oneshot ? 4u : 3u; }
    }
    if (!(vlive[ch] & 1u)) trk[ch].nmono = trk[ch].mono_note = 0;   /* (SLOOP trk_all_off) */
    trk[ch].slide_next = trk[ch].slide_glide = 0;
}

static void synth_hush(void)                    /* every voice fades out within a few ms (stop) */
{
    fx_kill = 1;
    uint32_t ch, k;
    for (ch = 0; ch < NPART; ch++)
        for (k = 0; k < NVOICE; k++)
            if (trk[ch].v[VCHK(k)].active) { trk[ch].v[VCHK(k)].stage = 4u; trk[ch].v[VCHK(k)].gate = 0; }
    for (ch = 0; ch < NPART; ch++) trk[ch].nmono = trk[ch].mono_note = trk[ch].slide_next = trk[ch].slide_glide = 0;
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

/* an engine's own envelope (ownenv) after note-off: cut (faded out) at the latest after 4 s, so a patch whose
 * release holds (a DX7 L4 at 99) cannot keep a voice forever: v->env counts it down (Q24, from full) */
#define OWN_REL_STEP ((1 << 24) / (4 * FS / CTL))

/* add n (<= CTL) stereo frames to out (L R L R ...) */
static void synth_render(int32_t *out, uint32_t n)
{
    static int32_t mixl[CTL], mixr[CTL], buf[CTL], sc[CTL], sd[CTL], sr[CTL], wl[CTL], wr[CTL];
    uint32_t ch, k, i, anyv = 0;
    if (n > CTL) n = CTL;
    for (i = 0; i < n; i++) mixl[i] = mixr[i] = sc[i] = sd[i] = sr[i] = 0;
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
            } else if (e->ownenv) {             /* the engine's envelopes are the amplitude (FM6): no ADSR, no velocity */
                if (v->stage == 3u && (v->env -= OWN_REL_STEP) <= 0) { v->stage = 4u; v->env = 1 << 24; }
                if (v->stage != 4u && e->done(t, v)) { v->active = 0; v->gate = 0; v->stage = 0; v->env = 0; }
                if (v->stage == 1u || v->stage == 2u) v->env = 1 << 24;   /* (held: the release timer full) */
                a = v->stage == 4u ? env_tick(ch, v) : 32767;
            } else {
                a = env_tick(ch, v);
                if (e->amp) a = e->amp(t, v, a);
            }
            if (!v->active) { vlast[ch][k] = 0; continue; }
            if (!e->ownenv) a = a * VEL_Q15[v->vel] >> 15;
            if (v->pitch_cur != v->pitch16 || v->pitch_frac) glide_tick(t, v);   /* (US-30; never in POLY without glide) */
            base = pitch_inc((uint32_t)clamp(v->pitch_cur, 0, 2047));
            m.inc = base;
            m.pitch16 = v->pitch_cur;
            m.fine = 0;
            if (v->pitch_frac) {                /* the glide's fraction (SLOOP voice.c lines 869-872) */
                m.fine = (v->pitch_frac & 255) * 3792 >> 16;
                m.inc += (uint32_t)((int32_t)(m.inc >> 12) * m.fine);
            }
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
        anyv = 1;
        fx_channel(ch, buf, n);                 /* DRIVE, FILTER */
        {
            const uint8_t *p = fxp[ch];
            int32_t cg = p[FX_CHO] * 258, dg = p[FX_DLY] * 258, rg = p[FX_REV] * 258, vg = VOL_Q15[c->vol];
            for (i = 0; i < n; i++) {
                int64_t x = (int64_t)buf[i] * vg >> 15;   /* (the sends: mulq15 stays in 32 bits) */
                int32_t xs = (int32_t)(x < -65535 ? -65535 : x > 65535 ? 65535 : x);
                mixl[i] += (int32_t)((int64_t)buf[i] * c->gl >> 15);   /* (64-bit: a loud part, many hits, past 2^31) */
                mixr[i] += (int32_t)((int64_t)buf[i] * c->gr >> 15);
                sc[i] += mulq15(xs, cg);
                sd[i] += mulq15(xs, dg);
                sr[i] += mulq15(xs, rg);
            }
        }
    }
    fx_buses(sc, sd, sr, wl, wr, n);            /* chorus, delay, reverb: back in stereo */
    if (fx_kill) {                              /* (stop) the return fades out over the first block, then stays shut and the
                                                 * lines empty while the stopped voices fade (they would refill them) */
        for (i = 0; i < n; i++) {
            wl[i] = fx_kill == 1u ? wl[i] * (int32_t)(n - 1u - i) / (int32_t)n : 0;
            wr[i] = fx_kill == 1u ? wr[i] * (int32_t)(n - 1u - i) / (int32_t)n : 0;
        }
        fx_clear();
        fx_kill = anyv ? 2u : 0u;
    }
    for (i = 0; i < n; i++) {                   /* headroom for 16 voices, the DC blocker */
        mixl[i] = dc_block((mixl[i] + wl[i]) >> 1, &dc_l, &dce_l);
        mixr[i] = dc_block((mixr[i] + wr[i]) >> 1, &dc_r, &dce_r);
    }
    lim_block(&lim_dac, mixl, mixr, n);         /* SLOOP's look-ahead limiter (the mix one block late) */
    for (i = 0; i < n; i++) {                   /* then the soft knee, then the hard limit */
        int32_t l = out[2 * i] + softclip(mixl[i]), r = out[2 * i + 1] + softclip(mixr[i]);
        out[2 * i] = clamp(l, -32767, 32767);
        out[2 * i + 1] = clamp(r, -32767, 32767);
    }
}

/* ---------------------------------------------- the main loop's changes --- */
enum { SC_SOUND, SC_LEVEL, SC_CH_OFF, SC_ALL_OFF, SC_NOTE_ON, SC_NOTE_OFF, SC_FX, SC_TEMPO, SC_LIVE_ON, SC_LIVE_OFF, SC_VOICE };
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

/* (main loop) a channel's six effects in one queued change: the values wait in fx_stage, the next render takes them
 * (a change made again before then: the render may take some of each, every one a valid value). 0 = queued */
static volatile uint8_t fx_stage[NPART][FX_N];
static int synth_fx_post(uint32_t ch, const uint8_t *v)
{
    uint32_t k;
    if (ch >= NPART) return 0;
    for (k = 0; k < FX_N; k++) fx_stage[ch][k] = v[k];
    return synth_post(SC_FX, ch, 0, 0);
}

static void synth_apply(void)                   /* in the audio interrupt, before rendering */
{
    uint32_t k;
    while (sq_r != sq_w) {
        const synth_cmd_t *c = &sq[sq_r % 32u];
        switch (c->op) {
        case SC_SOUND: if (c->b) synth_role(c->ch, c->b - 1u); synth_sound(c->ch, c->a); break;   /* b: role + 1, 0 = kept */
        case SC_LEVEL: synth_level(c->ch, c->a, c->b); break;
        case SC_CH_OFF: synth_ch_off(c->ch); break;
        case SC_ALL_OFF: synth_all_off(); break;
        case SC_NOTE_ON: synth_on(c->ch, c->a, c->b); break;
        case SC_NOTE_OFF: synth_off(c->ch, c->a); break;
        case SC_FX: if (c->ch < NPART) for (k = 0; k < FX_N; k++) synth_fx(c->ch, k, fx_stage[c->ch][k]); break;
        case SC_TEMPO: synth_tempo((uint32_t)c->a | (uint32_t)c->b << 8); break;
        case SC_LIVE_ON: synth_live_on(c->ch, c->a, c->b); break;
        case SC_LIVE_OFF: synth_live_off(c->ch, c->a); break;
        case SC_VOICE: synth_voice(c->ch, c->a, c->b); break;
        default: break;
        }
        sq_r++;
    }
}
