/* SPDX-License-Identifier: GPL-3.0-only */
/* US-4: synth.c through its interface: voice budget and stealing, every sound audible and silent after release,
 * role defaults, hostile calls ignored, render cost. US-21: SLOOP's 32 synthesised kits (21.1, 21.2, 21.4, 21.5).
 * US-19: SLOOP's FM6 engine (sloop_glue.c) and the engine pool. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <math.h>
#define VCHK(k) ((k) < NVOICE ? (k) : (abort(), 0u))   /* every voice index in range */
#include "synth.c"

static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)

static int32_t peak, last_peak;
static int64_t tail_abs, tail_n;
static void run(uint32_t blocks, uint32_t tail_from)
{
    int32_t out[2 * CTL];
    uint32_t b, i;
    for (b = 0; b < blocks; b++) {
        memset(out, 0, sizeof out);
        synth_apply();
        synth_render(out, CTL);
        for (i = 0; i < 2 * CTL; i++) {
            int32_t a = out[i] < 0 ? -out[i] : out[i];
            if (a > 32767) { CHECK(0, "sample %d beyond full scale", out[i]); }
            if (a > peak) peak = a;
            if (b >= tail_from) { tail_abs += a; tail_n++; if (a > last_peak) last_peak = a; }
        }
    }
}
#define SEC(s) ((uint32_t)((s) * 44100 / CTL))

static uint32_t find(const char *eng, const char *pr) { uint32_t s = sound_find(eng, pr); CHECK(s < sound_count, "no %s %s", eng, pr); return s; }

static void test_4_1_budget_and_stealing(void)
{
    uint32_t ch, pad, k;
    synth_init();
    pad = find("ANALOG", "SOFT PAD");
    for (ch = 0; ch < NPART; ch++) synth_sound(ch, pad);
    for (ch = 0; ch < NPART; ch++) { synth_on(ch, 60, 100); run(1, 99); synth_on(ch, 64, 100); run(1, 99); }
    CHECK(synth_active() == 16, "16 notes sound: %u", synth_active());
    synth_off(3, 60);                                   /* released, still sounding */
    run(2, 99);
    CHECK(synth_active() == 16, "released still counts: %u", synth_active());
    synth_on(0, 67, 100);                               /* the 17th: takes the released voice of channel 3 */
    CHECK(synth_active() == 16, "budget kept: %u", synth_active());
    CHECK(synth_count(3) == 1 && synth_count(0) == 3, "released voice stolen: ch3 %u ch0 %u", synth_count(3), synth_count(0));
    synth_on(1, 67, 100);                               /* nothing released: the oldest of channel 1 */
    CHECK(synth_active() == 16 && synth_count(1) == 2, "own oldest stolen: %u / %u", synth_active(), synth_count(1));
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[1].v[k];
        if (sounding(v)) CHECK(v->note == 64 || v->note == 67, "channel 1 kept %u", v->note);
    }
    run(SEC(0.05), 99);                                 /* the stolen ones fade out and free their slots */
    for (k = 0; k < NVOICE; k++) CHECK(trk[1].v[k].stage != 4u, "fade finished");
}

static void test_4_1_mono_and_retrigger(void)
{
    uint32_t bass;
    synth_init();
    bass = find("ANALOG", "SQR BASS");                  /* a MONO preset */
    synth_sound(2, bass);
    synth_on(2, 40, 100); synth_on(2, 43, 100); synth_on(2, 45, 100);
    CHECK(synth_count(2) == 1 && trk[2].v[0].note == 45, "mono plays the last note: %u", synth_count(2));
    synth_sound(4, find("ANALOG", "SOFT PAD"));
    synth_on(4, 60, 100); synth_on(4, 60, 90);          /* the same note again: one voice */
    CHECK(synth_count(4) == 1, "same note retriggers: %u", synth_count(4));
    run(SEC(0.5), 99);
    {
        int32_t before = trk[4].v[0].env;
        synth_on(4, 60, 90);                            /* a held note retriggered: the envelope goes on */
        CHECK(before > (1 << 20) && trk[4].v[0].env == before, "retrigger keeps the level: %d -> %d", before, trk[4].v[0].env);
    }
}

static void test_4_2_every_sound_sounds_and_ends(void)
{
    uint32_t id;
    synth_init();
    for (id = 0; id < sound_count; id++) {
        uint32_t note = sounds[id].e->oneshot ? 38u : 60u;
        synth_init();
        synth_sound(0, id);
        synth_level(0, 127, 64);
        peak = 0; last_peak = 0; tail_abs = 0; tail_n = 0;
        synth_on(0, note, 110);
        run(SEC(1.5), 1u << 30);
        CHECK(peak > 300, "%s %s is silent (peak %d)", sound_engine(id), sound_name(id), peak);
        synth_off(0, note);
        peak = 0;
        run(SEC(11.0), SEC(10.5));               /* the longest release (127) is 10 s to -60 dB */
        CHECK(tail_abs / (tail_n ? tail_n : 1) < 200, "%s %s does not end (mean %lld)", sound_engine(id), sound_name(id),
              (long long)(tail_abs / (tail_n ? tail_n : 1)));
        CHECK(synth_active() == 0, "%s %s voice stuck", sound_engine(id), sound_name(id));
    }
}

static void test_4_2_every_drum_sounds(void)
{
    static const uint8_t NOTES[] = {36, 37, 38, 39, 42, 45, 46, 49, 51, 56, 70};
    uint32_t i;
    for (i = 0; i < sizeof NOTES; i++) {
        synth_init();
        synth_sound(0, find("KIT", "STD KIT"));
        synth_level(0, 127, 64);
        peak = 0;
        synth_on(0, NOTES[i], 120);
        run(SEC(0.05), 1u << 30);
        CHECK(peak > 300, "drum note %u silent (%d)", NOTES[i], peak);
        run(SEC(2.0), 1u << 30);
        CHECK(synth_active() == 0, "drum note %u still sounding after 2 s", NOTES[i]);
    }
}

static void test_4_3_role_defaults(void)
{
    uint32_t r;
    synth_init();
    for (r = 0; r < ROLE_COUNT; r++) {
        uint32_t d = sound_default(r);
        CHECK(d < sound_count && sound_find(ROLE_SOUND[r][0], ROLE_SOUND[r][1]) == d, "role %u default missing", r);
        CHECK(snd_eq(sound_name(d), ROLE_SOUND[r][1]) && snd_eq(sound_engine(d), ROLE_SOUND[r][0]), "role %u name", r);
    }
    CHECK(sound_default(ROLE_DRUMS) == find("KIT", "STD KIT"), "drums get the kit");
    CHECK(sound_default(200) == sound_default(ROLE_OTHER), "unknown role: other's sound");
    CHECK(sound_count == 72u + ENG_FM6.npresets && sound_count <= SOUND_MAX, "catalog size %u", sound_count);   /* v4's 40, SLOOP's 32 kits, FM6 */
    CHECK(snd_eq(sound_name(sound_count), "?") && snd_eq(sound_engine(9999), "?"), "out of range names");
}

static void test_4_4_hostile_calls_ignored(void)
{
    uint32_t s0, i;
    synth_init();
    s0 = chs[0].sound;
    synth_on(8, 60, 100); synth_on(0, 128, 100); synth_on(0, 60, 0); synth_on(0, 60, 200); synth_on(99999, 1, 1);
    CHECK(synth_active() == 0, "bad notes ignored: %u", synth_active());
    synth_sound(0, 9999); synth_sound(8, 0);
    CHECK(chs[0].sound == s0, "bad sound ignored");
    synth_off(0, 50); synth_off(8, 50); synth_off(0, 200); synth_ch_off(8); synth_level(9, 1, 1);
    synth_level(1, 300, 300);
    CHECK(chs[1].vol == 127 && chs[1].pan == 127, "levels clamp: %u %u", chs[1].vol, chs[1].pan);
    for (i = 0; i < 32; i++) CHECK(synth_post(SC_LEVEL, 0, 100, 64) == 0, "post %u", i);
    CHECK(synth_post(SC_LEVEL, 0, 100, 64) == -1, "queue full refuses");
    synth_post(99, 0, 0, 0);
    run(1, 99);
    CHECK(sq_r == sq_w, "queue drained");
    synth_post(SC_NOTE_ON, 0, 60, 100); run(1, 99);
    CHECK(synth_count(0) == 1, "posted note on");
    synth_post(SC_NOTE_OFF, 0, 60, 0); synth_post(SC_CH_OFF, 0, 0, 0); synth_post(SC_ALL_OFF, 0, 0, 0);
    synth_post(SC_SOUND, 0, (uint8_t)find("ANALOG", "SAW LEAD"), 0);
    run(SEC(3), 99);
    CHECK(synth_active() == 0 && chs[0].sound == find("ANALOG", "SAW LEAD"), "posted off + sound: %u", synth_active());
    synth_on(0, 60, 100); synth_ch_off(0); run(SEC(3), 99);
    CHECK(synth_active() == 0, "channel off ends notes");
    synth_sound(5, find("KIT", "STD KIT")); synth_on(5, 49, 100); synth_off(5, 49);
    CHECK(synth_count(5) == 1, "note off does not cut a drum");
    synth_all_off(); run(SEC(0.1), 99);
    CHECK(synth_active() == 0, "all off stops drums too");
}

static void test_4_5_render_cost(void)
{
    uint32_t id, ch, worst = 0, worst_id = 0;
    clock_t t0;
    for (id = 0; id < sound_count; id++) {             /* the most expensive sound, 16 voices */
        clock_t c0 = clock(), c;
        synth_init();
        for (ch = 0; ch < 8; ch++) { synth_sound(ch, id); synth_on(ch, 48 + ch, 100); synth_on(ch, 55 + ch, 100); }
        run(200, 1u << 30);
        c = clock() - c0;
        if ((uint32_t)c > worst) { worst = (uint32_t)c; worst_id = id; }
    }
    synth_init();
    for (ch = 0; ch < 8; ch++) { synth_sound(ch, worst_id); synth_on(ch, 48 + ch, 100); synth_on(ch, 55 + ch, 100); }
    t0 = clock();
    run(SEC(5), 1u << 30);
    {
        double us_per_256 = (double)(clock() - t0) / CLOCKS_PER_SEC * 1e6 / (SEC(5) / 8.0);
        printf("render cost: worst sound %s %s, 16 voices: %.0f us per 256 frames on this host (budget 5805)\n",
               sound_engine(worst_id), sound_name(worst_id), us_per_256);
        CHECK(us_per_256 < 2000.0, "host render too slow: %.0f us", us_per_256);
    }
}

static void test_4_1_full_channel_takes_its_released_voice(void)
{
    uint32_t k, n;
    synth_init();
    synth_sound(6, find("ANALOG", "SOFT PAD"));
    for (n = 0; n < NVOICE; n++) { synth_on(6, 50 + n, 100); run(1, 99); }
    synth_off(6, 50 + NVOICE - 1);                      /* the newest is released, the oldest still held */
    synth_on(6, 80, 100);
    for (k = 0; k < NVOICE; k++) {
        voice_t *v = &trk[6].v[k];
        CHECK(v->note != 50 + NVOICE - 1 || !sounding(v) || v->note == 80, "released voice reused");
        if (v->note == 50) CHECK(sounding(v), "oldest held voice kept");
    }
}

static void test_4_2_sustain_level_held(void)
{
    uint32_t pad;
    int32_t want;
    synth_init();
    pad = find("ANALOG", "SOFT PAD");
    synth_sound(0, pad);
    synth_on(0, 60, 100);
    run(SEC(6), 99);
    want = (int32_t)sounds[pad].pr->env[2] << 17;
    CHECK(trk[0].v[0].env > want - want / 50 && trk[0].v[0].env < want + want / 50, "sustain %d want %d", trk[0].v[0].env, want);
}

static double low_share(uint32_t note, uint32_t ms0, uint32_t ms1)
{
    int32_t out[2 * CTL], buf[44100];
    uint32_t b, i, n = 0, hi = 0, nb = SEC(ms1 / 1000.0) + 1;
    int32_t pk = 0;
    synth_init();
    synth_sound(0, find("KIT", "STD KIT"));
    synth_level(0, 127, 64);
    synth_on(0, note, 120);
    for (b = 0; b < nb; b++) {
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
        for (i = 0; i < CTL; i++) {
            uint32_t at = b * CTL + i;
            if (at >= ms0 * 441u / 10u && at < ms1 * 441u / 10u && n < 44100u) buf[n++] = out[2 * i];
        }
    }
    for (i = 0; i < n; i++) { int32_t a = buf[i] < 0 ? -buf[i] : buf[i]; if (a > pk) pk = a; }
    for (i = 0; i < n; i++) { int32_t a = buf[i] < 0 ? -buf[i] : buf[i]; if (a * 4 < pk) hi++; }
    return n ? (double)hi / n : 0.0;
}

static void test_4_2_drum_shapes(void)
{
    int32_t out[2 * CTL];
    uint32_t b, i;
    double kick = low_share(36, 2, 12), bell = low_share(56, 2, 6);
    int32_t first;
    CHECK(kick < 0.30, "kick tone is a sine (share below a quarter of the peak %.2f)", kick);
    CHECK(bell > 0.35, "cowbell tone is two squares (share below a quarter of the peak %.2f)", bell);
    synth_init();                                       /* the kick still rings at 100 ms (320 ms decay) */
    synth_sound(0, find("KIT", "STD KIT"));
    synth_level(0, 127, 64);
    synth_on(0, 36, 120);
    peak = 0;
    first = 0;
    for (b = 0; b < SEC(0.12); b++) {
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
        if (b < SEC(0.01))
            for (i = 0; i < 2 * CTL; i++) { int32_t a = out[i] < 0 ? -out[i] : out[i]; if (a > first) first = a; }
        if (b >= SEC(0.09))
            for (i = 0; i < 2 * CTL; i++) { int32_t a = out[i] < 0 ? -out[i] : out[i]; if (a > peak) peak = a; }
    }
    CHECK(peak * 20 > first && first > 5000, "kick at 100 ms: %d of %d", peak, first);
}

static void test_4_4_drum_render_stays_in_its_buffer(void)
{
    int32_t out[CTL + 1];
    vmod_t m;
    uint32_t i;
    synth_init();
    synth_sound(0, find("KIT", "STD KIT"));
    synth_on(0, 38, 120);
    memset(&m, 0, sizeof m);
    m.amp0 = m.amp1 = 32767;
    for (i = 0; i <= CTL; i++) out[i] = 0x5A5A5A5A;
    for (i = 0; i < 16; i++) out[i] = 0;
    dk_render(&trk[0], &trk[0].v[0], out, 16, &m);
    CHECK(out[16] == 0x5A5A5A5A, "drum render wrote past n");
}

static void test_4_4_channel_8_touches_nothing(void)
{
    synth_init();
    synth_sound(NPART, 0); synth_level(NPART, 50, 50); synth_on(NPART, 60, 100); synth_off(NPART, 60);
    synth_ch_off(NPART); synth_post(SC_SOUND, NPART, 1, 0); synth_post(SC_LEVEL, NPART, 9, 9); run(2, 99);
    /* (an index past the channels would stop this test: the bounds sanitizer) */
    CHECK(synth_count(NPART) == 0, "channel 8 counts nothing");
}

static void test_4_3_lookups_at_the_edges(void)
{
    synth_init();
    CHECK(sound_find("NOPE", "NOPE") == sound_count && sound_find("KIT", "NOPE") == sound_count, "missing sound");
    CHECK(snd_eq(sound_name(sound_count), "?") && snd_eq(sound_engine(sound_count), "?"), "one past the end");
    CHECK(snd_eq(sound_name(sound_count - 1), sounds[sound_count - 1].pr->name), "the last sound");
    CHECK(sound_default(ROLE_COUNT) == sound_default(ROLE_OTHER) && sound_default(ROLE_COUNT - 1) == find("ANALOG", "PLUCK"), "role edges");
}

static void test_4_2_pan_and_volume(void)
{
    int32_t out[2 * CTL];
    int64_t l = 0, r = 0;
    uint32_t b, i, pass;
    for (pass = 0; pass < 3; pass++) {
        synth_init();
        synth_sound(0, find("ANALOG", "SINE KEY"));
        synth_level(0, pass == 2 ? 0 : 127, pass == 1 ? 0 : 64);
        synth_on(0, 60, 120);
        l = r = 0;
        for (b = 0; b < SEC(0.2); b++) {
            memset(out, 0, sizeof out);
            synth_render(out, CTL);
            for (i = 0; i < CTL; i++) { l += out[2 * i] < 0 ? -out[2 * i] : out[2 * i]; r += out[2 * i + 1] < 0 ? -out[2 * i + 1] : out[2 * i + 1]; }
        }
        if (pass == 0) CHECK(l > 100000 && r == l, "centre: L %lld R %lld", (long long)l, (long long)r);
        if (pass == 1) CHECK(l > 100000 && r == 0, "hard left: L %lld R %lld", (long long)l, (long long)r);
        if (pass == 2) CHECK(l == 0 && r == 0, "volume 0 is silent");
    }
}

static void test_4_1_retrigger_keeps_engine_state(void)
{
    uint32_t id, k, tried = 0;
    for (id = 0; id < sound_count; id++) {
        voice_t *v;
        int32_t s[8];
        uint32_t ph0;
        if (!sounds[id].e->keep || sounds[id].e->oneshot) continue;
        synth_init();
        synth_sound(0, id);
        synth_on(0, 60, 100);
        run(10, 99);
        v = &trk[0].v[0];
        for (k = 0; k < 8; k++) s[k] = v->s[k];
        ph0 = v->ph[0];
        synth_on(0, 60, 90);
        CHECK(v->ph[0] == ph0, "%s: retrigger keeps the phase", sound_name(id));
        for (k = 0; k < 8; k++)
            if (k != 2 && k != 3 && (sounds[id].e->keep >> k & 1u)) CHECK(v->s[k] == s[k], "%s: keeps s[%u]", sound_name(id), k);
        tried++;
    }
    CHECK(tried > 0, "some engine keeps state");
}

static void test_4_1_reused_voice_starts_like_new(void)
{
    uint32_t id = find("ANALOG", "SAW LEAD"), ph_new, k;
    int32_t s_new[8];
    synth_init(); synth_sound(0, id); synth_on(0, 64, 100);
    ph_new = trk[0].v[0].ph[0];
    for (k = 0; k < 8; k++) s_new[k] = trk[0].v[0].s[k];
    synth_init(); synth_sound(0, id); synth_on(0, 64, 100);
    trk[0].v[0].stage = 0; trk[0].v[0].active = 0;      /* (a used voice: its old state still there) */
    trk[0].v[0].ph[0] = 0x12345678u; trk[0].v[0].s[5] = 999;
    synth_on(0, 64, 100);
    CHECK(trk[0].v[0].ph[0] == ph_new, "reused voice phase %u, new %u", trk[0].v[0].ph[0], ph_new);
    CHECK(trk[0].v[0].s[5] == s_new[5], "reused voice: old engine state cleared (%d)", trk[0].v[0].s[5]);   /* (s[2] may seed from the age) */
}

static void test_4_2_decay_falls_to_sustain(void)
{
    uint32_t id = find("ANALOG", "SINE KEY"), b;
    int32_t sus = (int32_t)sounds[id].pr->env[2] << 17, last = 1 << 24;
    synth_init(); synth_sound(0, id); synth_on(0, 60, 100);
    while (trk[0].v[0].stage == 1u) run(1, 99);
    CHECK(sus < (1 << 24), "the preset decays");
    for (b = 0; b < SEC(3); b++) {
        run(1, 99);
        CHECK(trk[0].v[0].env >= sus && trk[0].v[0].env <= last, "decay block %u: %d (sus %d)", b, trk[0].v[0].env, sus);
        last = trk[0].v[0].env;
        if (fails) break;
    }
}

static void test_4_2_render_adds_to_out(void)
{
    int32_t a[2 * CTL], b[2 * CTL];
    uint32_t i, blk;
    for (blk = 0; blk < 20; blk++) {
        synth_init(); synth_sound(0, find("ANALOG", "SAW LEAD")); synth_on(0, 60, 120);
        for (i = 0; i <= blk; i++) { memset(a, 0, sizeof a); synth_render(a, CTL); }
        synth_init(); synth_sound(0, find("ANALOG", "SAW LEAD")); synth_on(0, 60, 120);
        for (i = 0; i <= blk; i++) { for (int j = 0; j < 2 * CTL; j++) b[j] = 1000; synth_render(b, CTL); }
        for (i = 0; i < 2 * CTL; i++) CHECK(b[i] == a[i] + 1000 || b[i] == 32767, "mixes into out: %d vs %d", b[i], a[i]);
        if (fails) break;
    }
}

static voice_t *drum(uint32_t note, int e0, int e1, int e2, int e3)
{
    synth_init();
    synth_sound(0, find("KIT", "STD KIT"));
    synth_level(0, 127, 64);
    trk[0].p[P_E0] = (int16_t)e0; trk[0].p[P_E1] = (int16_t)e1; trk[0].p[P_E2] = (int16_t)e2; trk[0].p[P_E3] = (int16_t)e3;
    synth_on(0, note, 120);
    return &trk[0].v[0];
}

static double drum_meas(uint32_t note, int e2, int e3, uint32_t ms, double *rms)
{
    int32_t out[2 * CTL], prev = 0;
    uint32_t b, i, zc = 0, n = 0;
    double sq = 0;
    drum(note, 64, 64, e2, e3);
    for (b = 0; b < ms * 441u / 10u / CTL; b++) {
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
        for (i = 0; i < CTL; i++) { int32_t x = out[2 * i]; zc += (x < 0) != (prev < 0); prev = x; sq += (double)x * x; n++; }
    }
    *rms = __builtin_sqrt(sq / n);
    return (double)zc / ms;
}

static void test_4_2_drum_voices(void)
{
    uint32_t inc64, inc0, inc127, k0, k127, seed1;
    double hat, snare, r, r0, r1;
    inc64 = (uint32_t)drum(36, 64, 64, 64, 0)->s[2];
    inc0 = (uint32_t)drum(36, 0, 64, 64, 0)->s[2];
    inc127 = (uint32_t)drum(36, 127, 64, 64, 0)->s[2];
    CHECK(inc0 < inc64 && inc64 < inc127 && inc127 < inc64 + inc64 / 3, "tune: %u %u %u", inc0, inc64, inc127);
    CHECK((uint32_t)drum(35, 64, 64, 64, 0)->s[2] == inc64, "35 and 36 are the same kick");
    CHECK((uint32_t)drum(41, 64, 64, 64, 0)->s[2] == pitch_inc(41 * 16) && (uint32_t)drum(50, 64, 64, 64, 0)->s[2] == pitch_inc(50 * 16), "toms by note");
    k0 = (uint32_t)drum(38, 64, 0, 64, 0)->s[1];
    k127 = (uint32_t)drum(38, 64, 127, 64, 0)->s[1];
    CHECK(k0 < DK[DK_SNARE].k && DK[DK_SNARE].k < k127 && k127 <= 65535, "decay: %u %u", k0, k127);
    seed1 = (uint32_t)drum(38, 64, 64, 64, 0)->s[4];
    synth_on(0, 38, 120);                               /* the same drum again: a new hit, new noise */
    CHECK((uint32_t)trk[0].v[0].s[4] != seed1, "hits differ (noise seed)");
    drum(36, 64, 64, 64, 0);
    run(SEC(0.3), 1u << 30);
    CHECK(trk[0].v[0].active && (uint32_t)trk[0].v[0].s[2] == 4869577u, "the kick bottoms at 50 Hz: %d", trk[0].v[0].s[2]);
    drum(38, 64, 64, 64, 0);
    run(SEC(0.3), 1u << 30);
    CHECK(!trk[0].v[0].active || (uint32_t)trk[0].v[0].s[2] < 4869577u, "only the kick has the floor");
    hat = drum_meas(42, 64, 0, 20, &r);
    snare = drum_meas(38, 64, 0, 20, &r);
    CHECK(hat > snare * 1.3, "hats are brighter: %.1f vs %.1f crossings a ms", hat, snare);
    drum_meas(36, 64, 0, 30, &r0);
    drum_meas(36, 64, 127, 30, &r1);
    CHECK(r1 > r0 * 1.1, "drive is louder: %.0f vs %.0f", r1, r0);
    drum(56, 64, 64, 64, 0);
    run(1, 99);
    CHECK((uint64_t)trk[0].v[0].ph[1] * 100 / trk[0].v[0].ph[0] >= 140 && (uint64_t)trk[0].v[0].ph[1] * 100 / trk[0].v[0].ph[0] <= 145, "cowbell second tone 1.44x");
}

static void test_4_4_hush_silences_everything(void)
{
    uint32_t ch;
    synth_init();
    for (ch = 0; ch < NPART; ch++) { synth_sound(ch, find("ANALOG", "SOFT PAD")); synth_on(ch, 50 + ch, 100); synth_on(ch, 60 + ch, 100); }
    run(SEC(1), 99);
    synth_hush();
    CHECK(synth_active() == 0, "hush takes every voice off the budget");
    peak = 0;
    run(SEC(0.05), 99);
    run(2, 0); peak = 0; run(2, 99);
    CHECK(peak < 30, "hush: silent within 50 ms (%d)", peak);
}

static void test_4_4_render_before_init_is_silent(void)
{
    int32_t out[2 * CTL];
    uint32_t i;
    memset(chs, 0, sizeof chs); memset(trk, 0, sizeof trk);
    trk[3].v[0].active = 1;                             /* (whatever the RAM held) */
    for (i = 0; i < 2 * CTL; i++) out[i] = 7;
    synth_render(out, CTL);
    for (i = 0; i < 2 * CTL; i++) CHECK(out[i] == 7, "untouched before init");
}

/* ------------------------------------------------ US-21: SLOOP's synthesised kits --- */
static uint32_t dsyn_first(void)                        /* the first SLOOP kit's sound id */
{
    uint32_t id;
    for (id = 0; id < sound_count && sounds[id].e != &ENG_DSYN; id++) {}
    return id;
}

/* one hit on channel 0 (full level, centre) of sound id: the left channel's first n samples */
static void hit(uint32_t id, uint32_t note, uint32_t vel, int32_t *mono, uint32_t n)
{
    int32_t out[2 * CTL];
    uint32_t i, b;
    synth_init();
    synth_sound(0, id);
    synth_level(0, 100, 64);
    synth_on(0, note, vel);
    for (b = 0; b * CTL < n; b++) {
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
        for (i = 0; i < CTL && b * CTL + i < n; i++) mono[b * CTL + i] = out[2 * i];
    }
}

#define CN 1024u                                        /* 23 ms from the hit */
/* spectral centroid, Hz: of the power spectrum (Hann window, DFT), where the sound's energy sits. (Of the magnitude
 * spectrum, a snare's high-passed white noise reads higher than a band-passed 9 kHz hat: 11 vs 9 kHz) */
static double centroid(const int32_t *x)
{
    static double c[CN], s[CN], w[CN];
    double num = 0, den = 0;
    uint32_t k, i;
    if (w[1] == 0)
        for (i = 0; i < CN; i++) {
            c[i] = cos(2 * M_PI * i / CN); s[i] = sin(2 * M_PI * i / CN); w[i] = 0.5 - 0.5 * c[i];
        }
    for (k = 1; k < CN / 2; k++) {
        double re = 0, im = 0, pw;
        for (i = 0; i < CN; i++) { re += x[i] * w[i] * c[(k * i) % CN]; im -= x[i] * w[i] * s[(k * i) % CN]; }
        pw = re * re + im * im;
        num += pw * k * 44100.0 / CN;
        den += pw;
    }
    return den > 0 ? num / den : 0;
}

static void test_21_1_every_kit_sounds_its_gm_notes_kick_darker_than_snare_than_hat(void)
{
    static const uint8_t GM[5] = {36, 38, 42, 46, 49};   /* kick, snare, closed hat, open hat, crash */
    static int32_t x[4096];
    uint32_t id0 = dsyn_first(), k, j, i;
    CHECK(id0 == 40u && DS_NKITS == 32u && sounds[id0 + DS_NKITS - 1u].e == &ENG_DSYN, "32 kits after v4's 40: %u + %u of %u",
          id0, DS_NKITS, sound_count);
    for (k = 0; k < DS_NKITS && id0 + k < sound_count; k++) {
        uint32_t id = id0 + k;
        double cen[3];
        CHECK(sounds[id].e == &ENG_DSYN && snd_eq(sound_name(id), DS_KITS[k].name) && snd_eq(sound_engine(id), "KIT"),
              "kit %u is SLOOP's %s (%s)", k, DS_KITS[k].name, sound_name(id));
        CHECK(sound_find("KIT", DS_KITS[k].name) == id, "kit %s found by name", DS_KITS[k].name);
        for (j = 0; j < 5; j++) {
            int32_t pk = 0;
            hit(id, GM[j], 110, x, 4096);
            for (i = 0; i < 4096; i++) pk = x[i] > pk ? x[i] : -x[i] > pk ? -x[i] : pk;
            CHECK(pk > 300, "%s note %u silent (peak %d)", sound_name(id), GM[j], pk);
            if (j < 3) cen[j] = centroid(x);
        }
        CHECK(cen[0] < cen[1] && cen[1] < cen[2], "%s: kick %.0f Hz < snare %.0f Hz < hat %.0f Hz", sound_name(id), cen[0],
              cen[1], cen[2]);
    }
    printf("21.1 %u kits: GM 36 38 42 46 49 sound; kick < snare < closed hat (spectral centroid)\n", DS_NKITS);
}

/* a kit's render as a CRC-32: every GM drum note 35..81 and the edges 0 / 127 at three velocities, one hit every 3
 * blocks, overlapping, on channel 2; then 1 s of tails; again with the kit's EDIT moved */
static uint32_t crc32_upd(uint32_t c, const void *p, size_t n)
{
    const uint8_t *b = p;
    size_t i;
    int j;
    c = ~c;
    for (i = 0; i < n; i++) { c ^= b[i]; for (j = 0; j < 8; j++) c = c >> 1 ^ (0xEDB88320u & -(c & 1u)); }
    return ~c;
}

static uint32_t kit_crc(uint32_t id)
{
    static const uint8_t VEL[3] = {127, 90, 30};
    int32_t out[2 * CTL];
    uint32_t crc = 0, pass, note, b, v;
    for (pass = 0; pass < 2; pass++) {
        synth_init();
        synth_sound(2, id);
        synth_level(2, 90, 64);
        if (pass) { trk[2].p[P_E0] = 20; trk[2].p[P_E1] = 110; trk[2].p[P_E2] = 10; trk[2].p[P_E3] = 90; }
        for (v = 0; v < 3; v++)
            for (note = 34; note <= 82; note++) {
                synth_on(2, note == 34 ? 0 : note == 82 ? 127 : note, VEL[v]);
                for (b = 0; b < 3; b++) { memset(out, 0, sizeof out); synth_render(out, CTL); crc = crc32_upd(crc, out, sizeof out); }
            }
        for (b = 0; b < 44100 / CTL; b++) { memset(out, 0, sizeof out); synth_render(out, CTL); crc = crc32_upd(crc, out, sizeof out); }
    }
    return crc;
}

static void test_21_2_composer_kits_keep_ids_and_render_bit_identical(void)
{
    /* captured from US-18's synth (ee8f389: v4's kits through SLOOP's limiter and DC filter), before the SLOOP kits
     * were added (v4 itself, 3615169, before the effects: EB9AFAC1, 02B5F725, B6D8327E) */
    static const struct { const char *name; uint32_t crc; } V4[3] = {
        {"STD KIT", 0x889EF25Eu}, {"TIGHT KIT", 0x4DA0AA79u}, {"BOOM KIT", 0x0BDDD56Bu}};
    uint32_t k;
    synth_init();
    for (k = 0; k < 3; k++) {
        uint32_t crc;
        CHECK(sounds[k].e == &ENG_KIT && snd_eq(sound_name(k), V4[k].name), "id %u is %s (%s)", k, V4[k].name, sound_name(k));
        crc = kit_crc(k);
        CHECK(crc == V4[k].crc, "%s renders as before the SLOOP kits: %08X, then %08X", V4[k].name, crc, V4[k].crc);
    }
    CHECK(sound_find("ANALOG", "SAW LEAD") == 3 && sound_find("NOISE", "RAIN") < 40, "the other sounds keep their ids");
    printf("21.2 STD / TIGHT / BOOM KIT: ids 0..2, renders bit-identical to before the SLOOP kits\n");
}

static void test_21_4_notes_outside_the_map_silent_six_hits_on_a_tick_all_sound(void)
{
    static const uint8_t SIX[6] = {36, 38, 42, 46, 49, 39};
    static const uint8_t OFF[4] = {0, 34, 83, 127};
    int32_t out[2 * CTL];
    uint32_t id0 = dsyn_first(), k, j, i, b;
    for (k = 0; k < DS_NKITS; k++) {
        synth_init();
        synth_sound(0, id0 + k);
        synth_level(0, 127, 64);
        for (j = 0; j < 4; j++) synth_on(0, OFF[j], 127);
        CHECK(synth_count(0) == 0, "%s: notes outside the map take no voice (%u)", sound_name(id0 + k), synth_count(0));
        peak = 0;
        run(SEC(0.1), 1u << 30);
        CHECK(peak == 0, "%s: notes 0 34 83 127 silent (peak %d)", sound_name(id0 + k), peak);
        for (j = 0; j < 6; j++) synth_on(0, SIX[j], 127);   /* one tick */
        for (b = 0; b < 2; b++) {
            memset(out, 0, sizeof out);
            synth_render(out, CTL);
        }
        CHECK(synth_count(0) == 6, "%s: 6 hits, 6 voices (%u)", sound_name(id0 + k), synth_count(0));
        for (i = 0; i < NVOICE; i++) {                  /* each of the six: its own lane, and sound in its next block */
            voice_t *v = &trk[0].v[i];
            dsv_t copy = dsyn_v[0][i];
            int32_t one[CTL], pk = 0;
            int32_t semi;
            if (!v->active) continue;
            CHECK(dsyn_v[0][i].d == &DS_KITS[k].s[ds_lane(v->note, &semi)], "%s: hit %u on its lane", sound_name(id0 + k), v->note);
            ds_render(&copy, one, CTL);
            for (b = 0; b < CTL; b++) pk = one[b] > pk ? one[b] : -one[b] > pk ? -one[b] : pk;
            CHECK(pk > 30, "%s: hit %u sounds among the six (peak %d)", sound_name(id0 + k), v->note, pk);
        }
    }
    {                                                   /* every note, every velocity: no crash, nothing past full scale */
        synth_init();
        synth_sound(0, id0 + 28);                       /* (INDUSTR: the most drive) */
        synth_level(0, 127, 64);
        for (j = 0; j < 128; j++) { synth_on(0, j, (j * 37u) % 127u + 1u); run(1, 1u << 30); }
        run(SEC(3), 1u << 30);
        CHECK(synth_active() == 0, "every hit ends");
    }
    printf("21.4 notes 0 34 83 127 silent on every kit; 6 hits on one tick: 6 voices sounding\n");
}

/* the densest piece of the suite (test_emu.py dense(): 8 channels of 16th-note pairs at 140 BPM), its drum channel on
 * a SLOOP kit: microseconds per 256 frames on this host */
static double dense_us(uint32_t kit_id, uint32_t secs)
{
    static const uint8_t ROLES[8] = {ROLE_LEAD, ROLE_BASS, ROLE_DRUMS, ROLE_PAD, ROLE_CHORDS, ROLE_COUNTER, ROLE_ARP, ROLE_FX};
    int32_t out[2 * CTL];
    uint32_t ch, fr, s = 0, total = secs * 44100u / CTL * CTL, k;
    struct timespec t0, t1;
    synth_init();
    for (ch = 0; ch < 8; ch++) { synth_sound(ch, ch == 2 ? kit_id : sound_default(ROLES[ch])); synth_level(ch, 100, 64); }
    clock_gettime(CLOCK_MONOTONIC, &t0);
    for (fr = 0; fr < total; fr += CTL) {
        if (fr >= s * 4725u) {                          /* a 16th at 140 BPM: 4725 frames */
            for (ch = 0; ch < 8; ch++) {
                if (s) for (k = 0; k < 2; k++) synth_off(ch, (ch == 2 ? 38 : 40 + ch * 5) + k * 7 + (s - 1) % 5);
                for (k = 0; k < 2; k++) synth_on(ch, (ch == 2 ? 38 : 40 + ch * 5) + k * 7 + s % 5, 100);
            }
            s++;
        }
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    return ((t1.tv_sec - t0.tv_sec) * 1e6 + (t1.tv_nsec - t0.tv_nsec) / 1e3) / (total / 256.0);
}

static void test_21_5_densest_drum_pattern_on_a_sloop_kit_well_under_budget(void)
{
    uint32_t id0 = dsyn_first(), k, worst = id0;
    double w = 0, us, us_std;
    for (k = 0; k < DS_NKITS; k++) {
        double t = dense_us(id0 + k, 1);
        if (t > w) { w = t; worst = id0 + k; }
    }
    us = dense_us(worst, 5);
    us_std = dense_us(0, 5);
    printf("21.5 densest piece, drums on %s: %.0f us per 256 frames on this host (STD KIT %.0f; budget 3000)\n",
           sound_name(worst), us, us_std);
    CHECK(us < 1500.0, "densest piece on %s: %.0f us per 256 frames", sound_name(worst), us);
}

/* ------------------------------------------------------------------ US-19 --- */
static uint32_t fm6_first(void) { return find("FM6", FM6_PRESETS[0].name); }

static void test_19_1_every_fm6_preset_on_any_channel(void)
{
    uint32_t k, ch;
    for (k = 0; k < ENG_FM6.npresets; k++)
        for (ch = 0; ch < NPART; ch++) {
            uint32_t id = fm6_first() + k;
            int before = fails;
            synth_init();
            synth_sound(ch, id);
            synth_level(ch, 127, 64);
            CHECK(chs[ch].e == &ENG_FM6 && chs[ch].sound == id, "19.1 ch %u plays FM6 %s", ch, sound_name(id));
            peak = 0; last_peak = 0; tail_abs = 0; tail_n = 0;
            synth_on(ch, 60, 110);
            synth_on(ch, 67, 90);
            run(SEC(1.5), 1u << 30);                    /* (run: no sample beyond +-32767) */
            CHECK(peak > 300, "19.1 FM6 %s ch %u is silent (peak %d)", sound_name(id), ch, peak);
            synth_off(ch, 60);
            synth_off(ch, 67);
            run(SEC(2.5), SEC(2.0));                    /* 2 s after the note-off */
            CHECK(tail_abs / (tail_n ? tail_n : 1) < 200, "19.1 FM6 %s ch %u does not end (mean %lld)", sound_name(id), ch,
                  (long long)(tail_abs / (tail_n ? tail_n : 1)));
            run(SEC(2.0), 1u << 30);
            CHECK(synth_active() == 0, "19.1 FM6 %s ch %u: a voice stuck", sound_name(id), ch);
            if (fails != before) return;
        }
}

static void test_19_3_v4_ids_keep_their_sounds(void)
{
    static const char *const V4[40][2] = {
        {"KIT", "STD KIT"}, {"KIT", "TIGHT KIT"}, {"KIT", "BOOM KIT"}, {"ANALOG", "SAW LEAD"},
        {"ANALOG", "SOFT PAD"}, {"ANALOG", "SQR BASS"}, {"ANALOG", "PWM STR"}, {"ANALOG", "ACID"},
        {"ANALOG", "SINE KEY"}, {"ANALOG", "RAVE"}, {"ANALOG", "SUB BASS"}, {"ANALOG", "PLUCK"},
        {"ANALOG", "BRASS"}, {"ANALOG", "WIND"}, {"ANALOG", "STRINGS"}, {"PHASE", "BRASS"}, {"PHASE", "ORGAN"},
        {"PHASE", "STRING"}, {"PHASE", "RESO"}, {"PHASE", "BELL"}, {"PHASE", "WIRE"}, {"TRIO", "FAT BASS"},
        {"TRIO", "ARP LEAD"}, {"TRIO", "SYNC LEAD"}, {"TRIO", "RING BELL"}, {"TRIO", "CHIP CHOIR"},
        {"WHEEL", "FULL ORGAN"}, {"WHEEL", "JAZZ PERC"}, {"WHEEL", "GOSPEL"}, {"WHEEL", "SOFT FLUTE"},
        {"WHEEL", "ROCK DRIVE"}, {"LOFI", "PULSE LD"}, {"LOFI", "WAVE BASS"}, {"LOFI", "ARP 8BIT"},
        {"LOFI", "WAVE LEAD"}, {"LOFI", "STEP LEAD"}, {"NOISE", "WIND"}, {"NOISE", "RAIN"}, {"NOISE", "ARCADE"},
        {"NOISE", "METAL"},
    };
    uint32_t i;
    synth_init();
    for (i = 0; i < 40u; i++)
        CHECK(snd_eq(sound_engine(i), V4[i][0]) && snd_eq(sound_name(i), V4[i][1]), "19.3 id %u is %s %s, v4 had %s %s", i,
              sound_engine(i), sound_name(i), V4[i][0], V4[i][1]);
    for (i = 0; i < ENG_FM6.npresets; i++)
        CHECK(sounds[72u + i].e == &ENG_FM6 && sounds[72u + i].pr == &FM6_PRESETS[i], "19.3 FM6 %u follows at %u", i, 72u + i);
    CHECK(sound_count == 72u + ENG_FM6.npresets, "19.3 nothing else: %u", sound_count);   /* (40..71: SLOOP's kits) */
    for (i = 0; i < ROLE_COUNT; i++)
        CHECK(eng_id(sounds[sound_default(i)].e) == ENG_NONE, "19.3 role %u default needs no engine block", i);
}

/* the pool: ENG_POOL_N channels play FM6, the next gets its role's default; a channel that leaves frees its block */
static void test_19_5_pool_full_gives_the_role_default(void)
{
    uint32_t ch, fm = fm6_first();
    synth_init();
    for (ch = 0; ch < NPART; ch++) synth_role(ch, ch == 4u ? ROLE_BASS : ROLE_PAD);
    for (ch = 0; ch < NPART; ch++) synth_sound(ch, fm);
    for (ch = 0; ch < NPART; ch++)
        CHECK((ch < ENG_POOL_N) == (chs[ch].e == &ENG_FM6), "19.5 ch %u: FM6 only while the pool lasts", ch);
    CHECK(chs[4].sound == sound_default(ROLE_BASS) && chs[5].sound == sound_default(ROLE_PAD), "19.5 refused: role default");
    synth_post(SC_SOUND, 6, fm, ROLE_LEAD + 1u); run(1, 99);
    CHECK(chs[6].sound == sound_default(ROLE_LEAD) && ch_role[6] == ROLE_LEAD, "19.5 posted role + 1 is kept");
    synth_sound(1, find("ANALOG", "SAW LEAD"));          /* channel 1 gives its block back */
    synth_sound(7, fm);
    CHECK(chs[7].e == &ENG_FM6 && eng_arena_of(&trk[7], ENGI_FM6) && !eng_arena_of(&trk[1], ENGI_FM6), "19.5 freed block reused");
    CHECK(!eng_arena_of(&trk[0], ENGI_FM6 + 1u), "19.5 no block for another engine");
    for (ch = 0; ch < NPART; ch++) { synth_on(ch, 60, 100); synth_on(ch, 64, 100); }
    peak = 0;
    run(SEC(0.5), 1u << 30);
    CHECK(peak > 300, "19.5 the mix still plays");
    synth_role(0, 999);
    CHECK(ch_role[0] == ROLE_OTHER, "19.5 a bad role is OTHER");
}

static void test_19_5_hostile_patch_bytes(void)
{
    static const uint8_t FILL[2] = {0xFF, 0x00};
    uint8_t v[FP_SIZE + 1u];
    uint32_t f, i, n;
    for (f = 0; f < 2; f++) {
        synth_init();
        synth_sound(0, fm6_first());
        synth_level(0, 127, 64);
        memset(v, FILL[f], sizeof v);
        fm6_set_patch(0, v);
        for (i = 0; i < FP_SIZE; i++)
            CHECK(fm6_patch[0][i] <= (i >= FP_NAME ? 126u : fm6_max(i)) && (i < FP_NAME || fm6_patch[0][i] >= 32u),
                  "19.5 byte %u clamped: %u", i, fm6_patch[0][i]);
        for (n = 0; n < 6u; n++) synth_on(0, 20u + n * 19u, n & 1u ? 1u : 127u);
        peak = 0;
        run(SEC(1.0), 1u << 30);                        /* (run: every sample within +-32767) */
        for (n = 0; n < 6u; n++) synth_off(0, 20u + n * 19u);
        tail_abs = 0; tail_n = 0;
        run(SEC(5.0), SEC(4.5));                        /* a held release (L4 99) is cut after 4 s */
        CHECK(synth_active() == 0, "19.5 fill %02X: a voice stuck", FILL[f]);
        CHECK(tail_abs / (tail_n ? tail_n : 1) < 200, "19.5 fill %02X: still sounding", FILL[f]);
    }
    synth_init();                                       /* hostile macros and slot numbers */
    synth_sound(0, fm6_first());
    for (i = 0; i < 8u; i++) trk[0].p[P_E0 + i] = (int16_t)(i & 1u ? -32768 : 32767);
    trk[0].p[P_E7] = 32767;
    eng_loaded(&trk[0], &ENG_FM6);
    CHECK(fm6_slot[0] == FM6_NSLOT - 1u, "19.5 PTCH clamped: %u", fm6_slot[0]);
    synth_on(0, 60, 100); synth_on(0, 127, 127); synth_on(0, 0, 1);
    run(SEC(1.0), 1u << 30);
    synth_all_off();
    run(SEC(5.0), 1u << 30);
    CHECK(synth_active() == 0, "19.5 hostile macros: a voice stuck");
    synth_sound(0, fm6_first());                        /* a bank slot (none in COMPOSER): the init voice */
    trk[0].p[P_E7] = FM6_NFACTORY;
    eng_loaded(&trk[0], &ENG_FM6);
    CHECK(fm6_slot[0] == FM6_NFACTORY && !memcmp(&fm6_patch[0][FP_NAME], "INIT VOICE", 10), "19.5 empty bank slot: init voice");
}

static void test_19_5_steal_and_hush_end_fm6_voices(void)
{
    uint32_t ch, n, fm = fm6_first();
    synth_init();
    for (ch = 0; ch < 3u; ch++) synth_sound(ch, fm);
    for (ch = 0; ch < 3u; ch++)
        for (n = 0; n < 6u; n++) { synth_on(ch, 50 + n * 2 + ch, 100); run(1, 99); }
    CHECK(synth_active() == VOICE_BUDGET, "19.5 FM6 voices count against the budget: %u", synth_active());
    synth_hush();
    peak = 0;
    run(SEC(0.05), 99);
    run(2, 0); peak = 0; run(2, 99);
    CHECK(peak < 30 && synth_active() == 0, "19.5 hush ends FM6 voices (%d)", peak);
}

static void fm6_sixteen(uint32_t k)                     /* 16 voices on the channels the pool allows */
{
    uint32_t ch, n, v = 0;
    synth_init();
    for (ch = 0; ch < ENG_POOL_N; ch++) {
        synth_sound(ch, fm6_first() + k);
        for (n = 0; n < FM6_POLY && v < 16u; n++, v++) synth_on(ch, 40 + ch * 12 + n * 3, 100);
    }
}

static void test_19_6_sixteen_fm6_voices_cost(void)
{
    uint32_t k, worst = 0, worst_k = 0;
    double us_per_256;
    clock_t t0;
    for (k = 0; k < ENG_FM6.npresets; k++) {
        clock_t c0, c;
        if (FM6_PRESETS[k].mono) continue;              /* (a MONO patch: one voice a channel) */
        c0 = clock();
        fm6_sixteen(k);
        CHECK(synth_active() == 16u, "19.6 16 FM6 voices sound: %u", synth_active());
        run(SEC(1), 1u << 30);
        c = clock() - c0;
        if ((uint32_t)c > worst) { worst = (uint32_t)c; worst_k = k; }
    }
    fm6_sixteen(worst_k);
    t0 = clock();
    run(SEC(3), 1u << 30);
    us_per_256 = (double)(clock() - t0) / CLOCKS_PER_SEC * 1e6 / (SEC(3) / 8.0);
    printf("19.6 render cost: 16 FM6 voices (%s): %.0f us per 256 frames on this host (limit 3000)\n",
           FM6_PRESETS[worst_k].name, us_per_256);
    CHECK(us_per_256 < 3000.0, "19.6 host render too slow: %.0f us", us_per_256);
}

int main(void)
{
    test_4_4_render_before_init_is_silent();
    test_4_1_budget_and_stealing();
    test_4_1_mono_and_retrigger();
    test_4_1_full_channel_takes_its_released_voice();
    test_4_2_sustain_level_held();
    test_4_2_drum_shapes();
    test_4_2_every_sound_sounds_and_ends();
    test_4_2_every_drum_sounds();
    test_4_3_role_defaults();
    test_4_4_hostile_calls_ignored();
    test_4_4_drum_render_stays_in_its_buffer();
    test_4_4_channel_8_touches_nothing();
    test_4_3_lookups_at_the_edges();
    test_4_2_pan_and_volume();
    test_4_1_retrigger_keeps_engine_state();
    test_4_1_reused_voice_starts_like_new();
    test_4_2_decay_falls_to_sustain();
    test_4_2_render_adds_to_out();
    test_4_2_drum_voices();
    test_4_4_hush_silences_everything();
    test_4_5_render_cost();
    test_21_1_every_kit_sounds_its_gm_notes_kick_darker_than_snare_than_hat();
    test_21_2_composer_kits_keep_ids_and_render_bit_identical();
    test_21_4_notes_outside_the_map_silent_six_hits_on_a_tick_all_sound();
    test_21_5_densest_drum_pattern_on_a_sloop_kit_well_under_budget();
    test_19_1_every_fm6_preset_on_any_channel();
    test_19_3_v4_ids_keep_their_sounds();
    test_19_5_pool_full_gives_the_role_default();
    test_19_5_hostile_patch_bytes();
    test_19_5_steal_and_hush_end_fm6_voices();
    test_19_6_sixteen_fm6_voices_cost();
    printf("%s (%u sounds)\n", fails ? "FAILED" : "ok", sound_count);
    return fails ? 1 : 0;
}
