/* SPDX-License-Identifier: GPL-3.0-only */
/* US-18: SLOOP's effects in COMPOSER's synth (fx.c through synth.c), measured on the rendered output */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#define VCHK(k) ((k) < NVOICE ? (k) : (abort(), 0u))
#include "synth.c"

#define GOLD_FX 0x5d21eeffu                              /* (18.3: recorded when SLOOP's code was copied) */
static int fails;
#define CHECK(c, ...) do { if (!(c)) { printf("FAIL %s:%d: ", __FILE__, __LINE__); printf(__VA_ARGS__); printf("\n"); fails++; } } while (0)
#define FR(s) ((uint32_t)((s) * 44100))
#define MAXF FR(5)
static int16_t L[MAXF], R[MAXF];                 /* the rendered output, left and right */
static uint32_t nf;

static uint32_t find(const char *e, const char *p) { uint32_t id = sound_find(e, p); if (id >= sound_count) { printf("no %s %s\n", e, p); exit(1); } return id; }
static void render_to(uint32_t frames)           /* append whole blocks up to `frames` */
{
    static int32_t out[2 * CTL];
    uint32_t i;
    while (nf + CTL <= frames && nf + CTL <= MAXF) {
        memset(out, 0, sizeof out);
        synth_render(out, CTL);
        for (i = 0; i < CTL; i++) { L[nf + i] = (int16_t)out[2 * i]; R[nf + i] = (int16_t)out[2 * i + 1]; }
        nf += CTL;
    }
}
static void fresh(void) { synth_init(); nf = 0; }
static double energy(uint32_t a, uint32_t b) { double e = 0; for (uint32_t i = a; i < b && i < nf; i++) e += (double)L[i] * L[i] + (double)R[i] * R[i]; return e; }
static int peak(uint32_t a, uint32_t b) { int p = 0; for (uint32_t i = a; i < b && i < nf; i++) { int x = abs(L[i]), y = abs(R[i]); if (x > p) p = x; if (y > p) p = y; } return p; }
static double tone(uint32_t a, uint32_t b, double hz)   /* Goertzel power of the left channel at hz */
{
    double w = 2 * 3.14159265358979 * hz / 44100, c = 2 * __builtin_cos(w), s1 = 0, s2 = 0;
    for (uint32_t i = a; i < b; i++) { double s0 = L[i] + c * s1 - s2; s2 = s1; s1 = s0; }
    return s1 * s1 + s2 * s2 - c * s1 * s2;
}

static void test_18_1_reverb_tail_and_dry_path(void)
{
    uint32_t lead, i;
    double note, tail, tail0;
    for (int send = 0; send < 2; send++) {
        fresh();
        lead = find("ANALOG", "SAW LEAD");
        synth_sound(0, lead);
        synth_fx(0, FX_REV, send ? 100 : 0);
        synth_on(0, 60, 100);
        render_to(FR(0.3));
        synth_off(0, 60);
        render_to(FR(2.0));
        note = energy(0, FR(0.3));
        tail = energy(FR(0.8), FR(1.8));
        if (send) CHECK(tail > note * 0.005, "reverb 100: tail %.3g of note %.3g (want > 0.5 %%)", tail, note);
        else tail0 = tail;
    }
    CHECK(tail0 < note * 1e-6, "reverb 0: no tail (%.3g)", tail0);
    {   /* every effect neutral: v4's dry render (tests/golden/v4_dry.bin), one block later (the limiter's look-ahead),
         * within 1% of its peak (the DC blocker at ~2 Hz) */
        static int16_t g[2 * 414 * CTL];
        FILE *f = fopen("tests/golden/v4_dry.bin", "rb");
        int gp = 0, worst = 0;
        CHECK(f && fread(g, 2, sizeof g / 2, f) == sizeof g / 2, "golden readable");
        if (f) fclose(f);
        fresh();
        synth_sound(0, sound_default(ROLE_LEAD));
        synth_level(0, 100, 64);
        synth_on(0, 60, 100);
        for (uint32_t b = 0; b < 415u; b++) { if (b == 276u) synth_off(0, 60); render_to((b + 1u) * CTL); }
        for (i = 0; i < 414u * CTL; i++) { int a = abs(g[2 * i]); if (a > gp) gp = a; }
        for (i = 0; i < 414u * CTL; i++) {
            int dl = abs(L[i + CTL] - g[2 * i]), dr = abs(R[i + CTL] - g[2 * i + 1]);
            if (dl > worst) worst = dl;
            if (dr > worst) worst = dr;
        }
        for (i = 0; i < CTL; i++) CHECK(L[i] == 0 && R[i] == 0, "the look-ahead block is silent");
        CHECK(gp > 1000 && worst * 100 <= gp, "dry path = v4's: worst %d of peak %d", worst, gp);
    }
}

static uint32_t onset(uint32_t from, int thr) { for (uint32_t i = from; i < nf; i++) if (abs(L[i]) > thr) return i; return 0; }

static void test_18_2_delay_follows_the_tempo(void)
{
    for (int send = 0; send < 2; send++) {
        uint32_t d0, d1, want = 13230000u / 1200u;            /* 1/8 at 120 BPM: 11025 samples */
        int p;
        fresh();
        synth_sound(0, sound_default(ROLE_DRUMS));
        synth_tempo(1200);
        synth_fx(0, FX_DLY, send ? 100 : 0);
        synth_on(0, 42, 127);                                  /* a closed hat: short */
        render_to(FR(0.02));
        synth_off(0, 42);
        render_to(FR(0.6));
        p = peak(0, FR(0.1));
        d0 = onset(0, p / 10);
        CHECK(p > 1000 && peak(d0 + 6000, d0 + 9000) < p / 50, "the hat has died away before the echo (%d)", peak(d0 + 6000, d0 + 9000));
        if (send) {
            d1 = onset(d0 + 6000, p / 20);
            CHECK(d1 && (d1 > d0 + want ? d1 - d0 - want : d0 + want - d1) <= 88u, "echo at %u samples, want %u +- 88", d1 - d0, want);
        } else CHECK(peak(d0 + 6000, FR(0.6)) < p / 200, "no send: no echo (%d)", peak(d0 + 6000, FR(0.6)));
    }
    synth_tempo(5000); CHECK(fx_tempo_x10 == 3000, "tempo clamped high");
    synth_init(); CHECK(fx_tempo_x10 == 1200, "a fresh start: 120 BPM");
    synth_tempo(10); CHECK(fx_tempo_x10 == 300, "tempo clamped low");
}

static void test_18_3_filter_and_drive(void)
{
    double hi[2], h3[2], f0 = 440.0 * __builtin_pow(2.0, (48 - 69) / 12.0);
    for (int k = 0; k < 2; k++) {
        fresh();
        synth_sound(0, find("ANALOG", "SAW LEAD"));
        synth_fx(0, FX_CUT, k ? 10 : 127);
        synth_on(0, 48, 110);
        render_to(FR(0.6));
        hi[k] = 0;
        for (int h = (int)(4500 / f0) + 1; h * f0 <= 12000; h++) hi[k] += tone(FR(0.3), FR(0.6), h * f0);   /* the saw's harmonics */
    }
    CHECK(hi[1] * 100 <= hi[0], "filter 10: highs %.3g vs open %.3g (want >= 20 dB down)", hi[1], hi[0]);
    for (int k = 0; k < 2; k++) {
        fresh();
        synth_sound(0, find("ANALOG", "SINE KEY"));
        synth_fx(0, FX_DRIVE, k ? 127 : 0);
        synth_on(0, 57, 120);                                  /* 220 Hz */
        render_to(FR(0.4));
        h3[k] = tone(FR(0.1), FR(0.4), 660) / (tone(FR(0.1), FR(0.4), 220) + 1);
    }
    CHECK(h3[1] >= h3[0] * 4, "drive 127: 3rd/1st %.3g vs %.3g (want >= 6 dB more)", h3[1], h3[0]);
}

static void test_18_4_limiter_and_dc(void)
{
    uint32_t ch, saw;
    int p;
    long long s = 0;
    fresh();
    saw = find("ANALOG", "SAW LEAD");
    for (ch = 0; ch < NPART; ch++) {
        synth_sound(ch, saw);
        synth_level(ch, 127, 64);
        synth_fx(ch, FX_REV, 127); synth_fx(ch, FX_DLY, 127); synth_fx(ch, FX_CHO, 127); synth_fx(ch, FX_DRIVE, 127);
        synth_on(ch, 48 + ch, 127); synth_on(ch, 55 + ch, 127);
    }
    render_to(FR(1.0));
    p = peak(0, FR(1.0));
    CHECK(p <= 16700 && p >= 12000, "16 loud voices, every effect up: peak %d (want 12000..16700: limited, not crushed)", p);
    for (uint32_t i = 2; i < nf; i++) CHECK(!(abs(L[i]) >= 32767 && abs(L[i - 1]) >= 32767 && abs(L[i - 2]) >= 32767), "no clipped run at %u", i);
    for (ch = 0; ch < NPART; ch++) synth_ch_off(ch);
    render_to(FR(5.0));                                        /* (SLOOP's reverb at SIZE 90 rings ~3 s) */
    for (uint32_t i = FR(4.0); i < nf; i++) s += abs(L[i]) + abs(R[i]);
    CHECK(s / (2 * (long long)(nf - FR(4.0))) < 8, "silence after: mean |x| %lld", s / (2 * (long long)(nf - FR(4.0))));
}

static void test_18_5_no_click_at_release(void)
{
    for (uint32_t ph = 0; ph < 8u; ph++) {
        static int32_t out[2 * CTL];
        uint32_t off, i;
        int before = 0, after = 0;
        fresh();
        synth_sound(0, find("ANALOG", "SAW LEAD"));
        synth_fx(0, FX_CUT, 60); synth_fx(0, FX_RESO, 10);
        synth_on(0, 52, 120);
        memset(out, 0, sizeof out);
        synth_render(out, 1u + ph * 3u);                       /* a part block: the release lands at another phase */
        for (i = 0; i < 1u + ph * 3u; i++) { L[nf] = (int16_t)out[2 * i]; R[nf] = (int16_t)out[2 * i + 1]; nf++; }
        render_to(nf + FR(0.3) / CTL * CTL);
        off = nf + CTL;                                        /* (heard one block later: the limiter) */
        synth_off(0, 52);
        render_to(nf + FR(0.1) / CTL * CTL);
        for (i = off - 220u; i < off; i++) { int d = abs(L[i] - L[i - 1]); if (d > before) before = d; }
        for (i = off; i < off + 220u; i++) { int d = abs(L[i] - L[i - 1]); if (d > after) after = d; }
        CHECK(before > 0 && after * 4 <= before * 5, "phase %u: step after release %d, before %d (a click: far more)", ph, after, before);
    }
}

static void test_18_8_out_of_range_is_clamped(void)
{
    fresh();
    synth_fx(0, FX_REV, 255); CHECK(fxp[0][FX_REV] == 127, "send clamped");
    synth_fx(0, FX_RESO, 200); CHECK(fxp[0][FX_RESO] == 10, "reso clamped");
    synth_fx(8, FX_REV, 50); synth_fx(0, FX_N, 50);           /* no channel 9, no 7th effect: ignored */
    for (uint32_t ch = 0; ch < NPART; ch++) CHECK(fxp[ch][FX_CUT] == 127 && fxp[ch][FX_DLY] == 0, "untouched %u", ch);
    {   /* the queue carries effects and the tempo */
        uint8_t v[FX_N] = {1, 2, 77, 100, 3, 4};
        CHECK(synth_fx_post(2, v) == 0 && synth_fx_post(9, v) == 0, "queued (channel 9: nothing)");
        synth_post(SC_TEMPO, 0, 1500 & 255, 1500 >> 8);
        synth_apply();
        CHECK(!memcmp(fxp[2], v, FX_N) && fx_tempo_x10 == 1500, "posted: cho %u tempo %u", fxp[2][FX_CHO], fx_tempo_x10);
        CHECK(fxp[1][FX_REV] == 0 && fxp[3][FX_CHO] == 0, "only channel 3 changed");
    }
    {   uint8_t v[FX_N];
        fx_defaults(ROLE_PAD, v); CHECK(v[FX_REV] > 0 && v[FX_CUT] == 127 && v[FX_DRIVE] == 0, "pad starts wet");
        fx_defaults(ROLE_BASS, v); CHECK(v[FX_REV] == 0, "bass starts dry");
        fx_defaults(99, v); CHECK(v[FX_REV] == FX_ROLE[ROLE_OTHER][0], "unknown role: OTHER's");
    }
}

static void test_18_10_stop_ends_the_tails(void)
{
    uint32_t ch, k;
    fresh();
    for (ch = 0; ch < NPART; ch++) {
        CHECK(fxp[ch][FX_REV] == 0 && fxp[ch][FX_DLY] == 0 && fxp[ch][FX_CHO] == 0 && fxp[ch][FX_CUT] == 127 &&
              fxp[ch][FX_RESO] == 0 && fxp[ch][FX_DRIVE] == 0, "a fresh start: channel %u dry and open", ch);
        synth_sound(ch, find("ANALOG", "SOFT PAD"));
        for (k = 0; k < 3u; k++) synth_fx(ch, k, 127);
        synth_on(ch, 50 + ch, 110);
    }
    CHECK(fx_tempo_x10 == 1200, "120 BPM");
    render_to(FR(1.0));
    CHECK(peak(FR(0.5), FR(1.0)) > 2000, "loud and wet");
    synth_hush();
    render_to(FR(1.05));
    CHECK(peak(FR(1.05) - 2 * CTL, FR(1.05)) < 30, "stop: silent within 50 ms, tails included (%d)", peak(FR(1.05) - 2 * CTL, FR(1.05)));
    render_to(FR(1.6));
    CHECK(peak(FR(1.05), FR(1.6)) < 30, "and stays silent (%d)", peak(FR(1.05), FR(1.6)));
}

static uint32_t crc_out(void)                  /* FNV-1a of the rendered output */
{
    uint32_t h = 2166136261u;
    for (uint32_t i = 0; i < nf; i++) { h = (h ^ (uint16_t)L[i]) * 16777619u; h = (h ^ (uint16_t)R[i]) * 16777619u; }
    return h;
}

/* SLOOP's code as copied: a render through every effect (DRIVE, FILTER with RESO, the three buses, the limiter) is bit
 * for bit the one recorded when it was copied (any change to the copied arithmetic changes it) */
static void test_18_3_every_effect_bit_for_bit(void)
{
    uint32_t ch, h;
    fresh();
    for (ch = 0; ch < 4u; ch++) {
        synth_sound(ch, find("ANALOG", "SAW LEAD"));
        synth_fx(ch, FX_REV, 90); synth_fx(ch, FX_DLY, 70); synth_fx(ch, FX_CHO, 80);
        synth_fx(ch, FX_CUT, 50 + ch * 20); synth_fx(ch, FX_RESO, 4 + ch * 2); synth_fx(ch, FX_DRIVE, 30 + ch * 30);
        synth_on(ch, 45 + ch * 5, 120);
    }
    render_to(FR(0.4));
    for (ch = 0; ch < 4u; ch++) synth_off(ch, 45 + ch * 5);
    render_to(FR(1.2));
    h = crc_out();
    printf("18.3 every effect: render %08x\n", h);
    CHECK(h == GOLD_FX, "every effect: render %08x, recorded %08x", h, GOLD_FX);
}

static void test_18_8_queue_ignores_channel_9(void)
{
    fresh();
    synth_post(SC_FX, 8, 0, 0);                                /* (synth_fx_post never queues it; a bad entry is ignored) */
    synth_apply();
    for (uint32_t ch = 0; ch < NPART; ch++) CHECK(fxp[ch][FX_CUT] == 127, "untouched %u", ch);
}

static void test_18_9_cost_with_every_effect(void)
{
    static int32_t out[2 * CTL];
    uint32_t ch, id, worst = 0, k;
    double us_worst = 0;
    for (id = 0; id < sound_count; id++) {
        clock_t t0;
        double us;
        fresh();
        for (ch = 0; ch < NPART; ch++) {
            synth_sound(ch, id);
            for (k = 0; k < FX_N; k++) synth_fx(ch, k, k == FX_CUT ? 40 : 127);
            synth_on(ch, 48 + ch, 120); synth_on(ch, 60 + ch, 120);
        }
        for (k = 0; k < 64u; k++) synth_render(out, CTL);
        t0 = clock();
        for (k = 0; k < 8u * 400u; k++) synth_render(out, CTL);
        us = (double)(clock() - t0) * 1e6 / CLOCKS_PER_SEC / 400.0;   /* per 256 frames */
        if (us > us_worst) us_worst = us, worst = id;
    }
    printf("fx cost: worst %s %s with every effect, 16 voices: %.0f us per 256 frames (budget 5805)\n", sound_engine(worst), sound_name(worst), us_worst);
    CHECK(us_worst < 3000, "%.0f us", us_worst);
}

/* stop with the reverb return up: it fades out over the first block, no click (the largest sample-to-sample step
 * in the 4 blocks after stop stays under a tenth of the level before; cutting it gives ~1400 against ~4300) */
static void test_35_stop_fades_the_reverb_return(void)
{
    uint32_t h, j, worst = 0;
    int before;
    fresh();
    synth_sound(0, find("ANALOG", "SOFT PAD"));
    synth_fx(0, FX_REV, 127);
    synth_on(0, 48, 100); synth_on(0, 55, 100); synth_on(0, 60, 100);
    render_to(FR(1.0));
    h = nf;
    before = peak(h - CTL, h);
    synth_hush();
    render_to(h + 8 * CTL);
    for (j = h; j < h + 4 * CTL; j++) { uint32_t d = (uint32_t)abs(L[j] - L[j - 1]); if (d > worst) worst = d; }
    CHECK(before > 1000 && worst * 10u < (uint32_t)before, "stop: the reverb fades (step %u, level %d)", worst, before);
    CHECK(peak(h + 6 * CTL, h + 8 * CTL) < 30, "and is silent soon after (%d)", peak(h + 6 * CTL, h + 8 * CTL));
}

int main(void)
{
    test_18_1_reverb_tail_and_dry_path();
    test_18_2_delay_follows_the_tempo();
    test_18_3_filter_and_drive();
    test_18_3_every_effect_bit_for_bit();
    test_18_8_queue_ignores_channel_9();
    test_18_4_limiter_and_dc();
    test_18_5_no_click_at_release();
    test_18_8_out_of_range_is_clamped();
    test_18_10_stop_ends_the_tails();
    test_35_stop_fades_the_reverb_return();
    test_18_9_cost_with_every_effect();
    printf(fails ? "FAILED\n" : "ok\n");
    return fails != 0;
}
