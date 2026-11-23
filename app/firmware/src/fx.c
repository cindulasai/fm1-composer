/* SPDX-License-Identifier: GPL-3.0-only
 * Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments (SLOOP's effects, copied below)
 * COMPOSER's glue: GPL-3.0-only */
/* fx.c: COMPOSER's effects, SLOOP 2.8.5's (isod89/sloop-fm1 @ bac9c05, firmware/src/fx.c and dsp.c; see
 * ../SLOOP-PROVENANCE.md). Copied unchanged: track_dist (DIST), lim_block (the look-ahead limiter), dc_block,
 * djf_block / djf_run / tflt_run (the track FILTER), fx_buses (chorus, tempo delay, FDN reverb) and MULQ12R.
 * COMPOSER's glue (marked "COMPOSER:"): the settings SLOOP keeps in its song (fixed at SLOOP's defaults here), the
 * delay's time from COMPOSER's tempo, and per channel: REVERB, DELAY, CHORUS sends, FILTER (a low-pass, 127 open),
 * RESO (SLOOP's FILTER RESO 0..10) and DRIVE (SLOOP's DIST).
 * COMPOSER: the copied code's left shifts of signed values are written as multiplications by the same power of two
 * (dc_block, fx_buses, tflt_run): the same result on the chip, and defined C for the host's sanitizers. */
#ifndef POOL
#define POOL
#endif
/* COMPOSER: SLOOP's names for what the copied code reads */
enum { P_DIST };                                         /* track_dist's parameter */
typedef struct { int16_t p[1]; uint8_t dist_on; int32_t dist_hp, dist_lp1, dist_lp2; } fxtrack_t;
enum { G_DTIME, G_DFDBK, G_DCOLOR, G_DMIX, G_RSIZE, G_RDAMP, G_CRATE, G_CDEPTH, G_N };
static const struct { int16_t g[G_N]; } song = {{1, 60, 70, 90, 90, 60, 40, 60}};   /* SLOOP's defaults (params.c) */
static const int32_t thr_dly = 0;                        /* (SLOOP's ECHO THROW: none in COMPOSER) */
static uint32_t fx_tempo_x10 = 1200;                     /* COMPOSER: the tempo the delay follows */
static uint32_t dly_samples(uint32_t d) { (void)d; return 13230000u / (fx_tempo_x10 ? fx_tempo_x10 : 1200u); }   /* 1/8 */
#define track_t fxtrack_t
/* LFO 0.05 .. 40 Hz, phase increment per control tick (SLOOP tools/gen_tables.py) */
static const uint32_t LFO_INC[128] = {
    155826u, 164248u, 173124u, 182481u, 192343u, 202738u, 213695u, 225244u,
    237417u, 250248u, 263773u, 278028u, 293054u, 308892u, 325586u, 343183u,
    361730u, 381279u, 401885u, 423605u, 446499u, 470630u, 496065u, 522874u,
    551133u, 580919u, 612314u, 645406u, 680287u, 717053u, 755806u, 796653u,
    839708u, 885090u, 932924u, 983343u, 1036488u, 1092504u, 1151548u, 1213783u,
    1279382u, 1348525u, 1421406u, 1498225u, 1579196u, 1664543u, 1754503u, 1849324u,
    1949270u, 2054618u, 2165658u, 2282701u, 2406068u, 2536103u, 2673166u, 2817636u,
    2969914u, 3130422u, 3299604u, 3477930u, 3665893u, 3864015u, 4072844u, 4292959u,
    4524970u, 4769520u, 5027287u, 5298984u, 5585366u, 5887224u, 6205397u, 6540765u,
    6894258u, 7266855u, 7659589u, 8073548u, 8509880u, 8969793u, 9454561u, 9965529u,
    10504112u, 11071802u, 11670173u, 12300882u, 12965678u, 13666403u, 14404998u, 15183510u,
    16004096u, 16869031u, 17780710u, 18741661u, 19754546u, 20822172u, 21947498u, 23133641u,
    24383889u, 25701706u, 27090743u, 28554851u, 30098086u, 31724724u, 33439273u, 35246484u,
    37151366u, 39159195u, 41275537u, 43506256u, 45857533u, 48335884u, 50948176u, 53701648u,
    56603931u, 59663066u, 62887531u, 66286261u, 69868674u, 73644696u, 77624792u, 81819991u,
    86241918u, 90902825u, 95815630u, 100993944u, 106452119u, 112205278u, 118269365u, 124661182u
};

/* ---- copied from SLOOP firmware/src/dsp.c (unchanged) ---- */
#define MULQ12R(x, g, k) \
    ((((x) >> (k)) * (g) + ((((x) & ((1 << (k)) - 1)) * (g) + (1 << ((k) - 1))) >> (k)) + (1 << (11 - (k)))) >> (12 - (k)))

/* ---- copied from SLOOP firmware/src/fx.c (unchanged, but for the sections: COMPOSER keeps them in its pool) ---- */
#define DLY_LEN 65536u           /* 1.49 s: 1/4 down to 41 BPM, 1/8 dotted down to 31, 1/4 dotted down to 61 (clamped below) */
#define CHO_LEN 2048u
static int16_t dly_buf[DLY_LEN] POOL;
static int16_t cho_buf[CHO_LEN] POOL;
/* the reverb: two input diffusers, then four delay lines mixed by a Hadamard matrix (a feedback delay
 * network: every echo feeds all four, so it thickens instead of ringing like a comb), damped in the
 * loop, one line slowly modulated (no metallic tone on long tails); left and right take different lines */
#define REV_MOD 12               /* samples the modulated line moves (+-) */
static const uint16_t REV_LINE[4] = {1559, 1931, 2389, 2791};   /* 35..63 ms, coprime */
static const uint16_t REV_AP[2] = {556, 441};
static int16_t rev_line[1559 + 1931 + 2389 + 2791 + REV_MOD + 2] POOL;   /* (the pool is
                                                                      * full; .lowram: the SRAM below the RAM region, app.ld) */
static int16_t rev_ap[556 + 441] POOL;
static struct {
    uint32_t dly_w, cho_w, cho_ph, rev_ph;
    int32_t dly_lp;
    uint16_t line_i[4], ap_i[2];
    int32_t line_lp[4];
} fx;
static volatile uint8_t rev_frz;         /* punch-in FREEZE (punch.c): the reverb holds, no loss, no damping */
static int32_t rev_l[CTL], rev_r[CTL];   /* the reverb's own return of this block (FREEZE plays it) */
/* DIST: low cut -> drive (1x..8x, exponential) -> asymmetric soft clip
 * (a little bias = even harmonics) -> tone low-pass that closes with drive ->
 * make-up gain. State per part (track_t dist_*). */
static void track_dist(track_t *t, int32_t *b, uint32_t n)
{
    int32_t d = t->p[P_DIST], i, g, k, mk, bias = 2400, b0;
    if (!d) {
        t->dist_on = 0;
        return;
    }
    if (!t->dist_on) {                                  /* coming on: no stale high-pass state (a thump) */
        t->dist_on = 1;
        t->dist_hp = b[0];
        t->dist_lp1 = t->dist_lp2 = 0;
    }
    g = 4096 + d * d * 2;                                /* Q12: 1x .. ~9x, gentle at first */
    k = 32000 - d * 95;                                  /* tone: transparent at low drive .. ~3 kHz, Q15 */
    mk = 30000 - d * 120;                                /* make-up */
    b0 = softclip(bias);
    for (i = 0; i < (int32_t)n; i++) {
        int32_t x = b[i], y;
        t->dist_hp += (x - t->dist_hp + 64) >> 7;           /* ~55 Hz low cut: keep the bass out of the clipper */
        x = clamp(x - t->dist_hp, -230000, 230000);         /* (x >> 2) * g fits 32 bits; the clip is flat out there */
        y = softclip(MULQ12R(x, g, 2) + bias) - b0;         /* (as (x >> 2) * g >> 10 in 32 bits: no overflow for loud
                                                             * poly; #4: rounded, no step of 4 x the drive) */
        t->dist_lp1 += mulq15(y - t->dist_lp1, k);         /* two poles: tames the fizz */
        t->dist_lp2 += mulq15(t->dist_lp1 - t->dist_lp2, k);
        b[i] = mulq15(t->dist_lp2, mk);
    }
}

/* master: peak limiter in front of the soft clipper, a 35 ms hold then ~46 ms release (2.6), threshold where tanh is still nearly linear,
 * so chords get quieter instead of crushed. SLOOP 2.6: it looks one block ahead (lim_block): the output is the mix
 * CTL samples late (0.73 ms) and the gain glides over each block to what the loudest sample of it and of the next one
 * needs, so a hot kick or bass is already turned down when it comes. Before, the gain caught up in ~4 samples and the
 * first ones went through the tanh knee at up to 4x: a crackle on the beats with the MASTER up (in headphones most) */
#define LIM_T 18000
typedef struct {
    int32_t dl[CTL], dr[CTL];           /* the mix of the last block (the delay) */
    uint32_t w;                         /* its oldest sample (the next out) */
    int32_t env;                        /* the peak the gain fits under LIM_T (LIM_T: none) */
    int32_t g;                          /* the gain at the end of the last block, Q15 (32768: 1) */
    uint32_t hold;                      /* blocks before the release starts (LIM_HOLD after a peak near env) */
} lim_t;
/* (2.6, a whistle on loud basses) the release waits LIM_HOLD blocks after the last peak near the envelope: a held bass
 * (down to ~29 Hz, a period under 35 ms) keeps one gain, instead of the gain dipping and creeping back at every cycle
 * (a sawtooth at the bass's pitch with a corner at every block: tones around 1.4 kHz and up) */
#define LIM_HOLD 48u
#define LIM_REL 11                      /* the release after the hold: ~46 ms (12 before the hold: ~93 ms; the same loudness) */
static lim_t lim_dac = {.env = LIM_T, .g = 32768};   /* COMPOSER: (no USB-only path) */
/* l, r: n (<= CTL) samples in; out: the n samples of CTL before, limited (before the knee) */
static void lim_block(lim_t *s, int32_t *l, int32_t *r, uint32_t n)
{
    uint32_t i, w = s->w;
    int32_t pk = 0, a, g0 = s->g, g1, dg, e = s->env;
    for (i = 0; i < CTL; i++) {                         /* the samples going out now and the ones after them (the
                                                         * rest of the delay, then this block): CTL ahead */
        a = s->dl[i] < 0 ? -s->dl[i] : s->dl[i], pk = a > pk ? a : pk;
        a = s->dr[i] < 0 ? -s->dr[i] : s->dr[i], pk = a > pk ? a : pk;
    }
    for (i = 0; i < n; i++) {
        a = l[i] < 0 ? -l[i] : l[i], pk = a > pk ? a : pk;
        a = r[i] < 0 ? -r[i] : r[i], pk = a > pk ? a : pk;
    }
    if ((int64_t)pk * 17 >= (int64_t)e * 16)           /* a peak within ~0.5 dB of the envelope: the hold again */
        s->hold = LIM_HOLD;
    if (s->hold)
        s->hold--;
    else if (e > LIM_T)                                 /* the release (after the hold), a block at a time */
        e -= (int32_t)((((int64_t)(e - LIM_T) * (int32_t)n) >> LIM_REL) + 1);
    if (e < LIM_T)
        e = LIM_T;
    if (pk > e)
        e = pk, s->hold = LIM_HOLD;
    s->env = e;
    g1 = e > LIM_T ? (int32_t)(((uint32_t)LIM_T << 15) / (uint32_t)e) : 32768;
    s->g = g1;
    dg = (g1 - g0) / (int32_t)(n ? n : 1u);             /* (towards 0: the gain never above both ends' bound) */
    for (i = 0; i < n; i++) {
        int32_t xl = s->dl[w], xr = s->dr[w], g = g0 + dg * (int32_t)(i + 1u);
        s->dl[w] = l[i], s->dr[w] = r[i];
        w = (w + 1u) & (CTL - 1u);
        if (i + 1u == n)
            g = g1;
        if (g < 32768) {                                /* (64-bit: |x| may be far above Q15. 2.6 fix: x >> 4 first cut
                                                         * the limited signal to 12 bits, a whistle on loud basses) */
            xl = (int32_t)(((int64_t)xl * g) >> 15);
            xr = (int32_t)(((int64_t)xr * g) >> 15);
        }
        l[i] = xl, r[i] = xr;
    }
    s->w = w;
}

static int32_t dc_l, dc_r, dce_l, dce_r;               /* COMPOSER: (fx.c keeps these with the low cut) */
/* DC blocker (~2 Hz), always on: a leaky integrator of the input (Q6 state) subtracted from it.
 * The >> 12 step keeps its remainder (error feedback, 0..4095) and adds it to the next one, so no
 * part of the step is lost: the state follows the input exactly, down to 0 after the sound stops.
 * (It was rounded, and a rounded step of (x - dc) / 4096 stops moving at |x - dc| < 2048: a
 * constant offset of up to +-31 stayed at the output after silence.) */
static inline int32_t dc_block(int32_t x, int32_t *dc, int32_t *err)
{
    int32_t e = (x * 64) - *dc + *err, d = e >> 12;
    *err = e - (d * 4096);
    *dc += d;
    return x - ((*dc + 32) >> 6);
}

static uint32_t delay_samples(void)
{
    uint32_t s = dly_samples((uint32_t)song.g[G_DTIME]);
    return s < 16u ? 16u : s >= DLY_LEN ? DLY_LEN - 1u : s;
}

/* process the three buses for one block; sends in, wet out (stereo). The LFOs (chorus, reverb line)
 * are computed per block and ramped: no sine per sample */
#define HALF0(v) ((v) >= 0 ? (v) >> 1 : -(-(v) >> 1))   /* v / 2 towards zero */
static void fx_buses(const int32_t *cho_in, const int32_t *dly_in, const int32_t *rev_in, int32_t *wet_l,
                     int32_t *wet_r, uint32_t n)
{
    uint32_t i, k, dl = delay_samples();
    int32_t fb = song.g[G_DFDBK] * 230, col = 2000 + song.g[G_DCOLOR] * 240;
    if (thr_dly && fb < 89 * 230)                       /* ECHO THROW: FDBK at least 70 %, gliding with it (2.6) */
        fb += ((89 * 230 - fb) * thr_dly) >> 15;
    int32_t dmix = song.g[G_DMIX] * 258;
    int32_t g = 17000 + song.g[G_RSIZE] * 104, lpk = 32767 - song.g[G_RDAMP] * 200;   /* loop gain (RT60 ~0.4..4 s), damping */
    int32_t frnd = rev_frz ? 128 : 0;                  /* FREEZE: what is in the lines stays there (below: no
                                                         * loss, rounded to nearest; 0: as always, bit for bit) */
    int32_t cdepth = song.g[G_CDEPTH] * 6;
    int32_t ca0, ca1, cb0, cb1, ma, mb, dca, dcb;
    const uint32_t L0 = REV_LINE[0] + REV_MOD + 2u, B1 = L0, B2 = B1 + REV_LINE[1], B3 = B2 + REV_LINE[2];
    {   /* the chorus' two read points (Q8 samples back) and line 0's extra length, at both ends of the block */
        int32_t s0 = osc_sine(fx.cho_ph), s1, m0 = osc_sine(fx.rev_ph), m1;
        fx.cho_ph += LFO_INC[song.g[G_CRATE] & 127];
        fx.rev_ph += LFO_INC[30];
        s1 = osc_sine(fx.cho_ph);
        m1 = osc_sine(fx.rev_ph);
        ca0 = (400 << 8) + ((s0 + 32768) * cdepth >> 8), ca1 = (400 << 8) + ((s1 + 32768) * cdepth >> 8);
        cb0 = (400 << 8) + ((32767 - s0) * cdepth >> 8), cb1 = (400 << 8) + ((32767 - s1) * cdepth >> 8);
        ma = (REV_MOD << 8) + ((m0 * REV_MOD) >> 7), mb = (REV_MOD << 8) + ((m1 * REV_MOD) >> 7);
        dca = (ca1 - ca0) >> CTL_LOG2, dcb = (cb1 - cb0) >> CTL_LOG2;
    }
    for (i = 0; i < n; i++) {
        int32_t yl, yr, x, a, o0, o1, o2, o3;
        /* chorus: two modulated short delays, 5..15 ms, the LFO half a turn apart: left and right move apart */
        cho_buf[fx.cho_w & (CHO_LEN - 1u)] = (int16_t)clamp(cho_in[i] >> 1, -32768, 32767);
        {
            int32_t r0 = ca0 + dca * (int32_t)i, r1 = cb0 + dcb * (int32_t)i;
            uint32_t i0 = (uint32_t)r0 >> 8, i1 = (uint32_t)r1 >> 8;
            int32_t c0 = cho_buf[(fx.cho_w - i0) & (CHO_LEN - 1u)], c1 = cho_buf[(fx.cho_w - i0 - 1u) & (CHO_LEN - 1u)];
            int32_t d0 = cho_buf[(fx.cho_w - i1) & (CHO_LEN - 1u)], d1 = cho_buf[(fx.cho_w - i1 - 1u) & (CHO_LEN - 1u)];
            yl = (c0 + (((c1 - c0) * (r0 & 255)) >> 8)) * 2;
            yr = (d0 + (((d1 - d0) * (r1 & 255)) >> 8)) * 2;
        }
        fx.cho_w++;
        /* delay with a low-passed feedback (in the middle) */
        x = dly_buf[(fx.dly_w - dl) & (DLY_LEN - 1u)];
        fx.dly_lp += mulq15(x - fx.dly_lp, col);
        dly_buf[fx.dly_w & (DLY_LEN - 1u)] =
            (int16_t)clamp((dly_in[i] >> 1) + mulq15(fx.dly_lp, fb), -32768, 32767);
        fx.dly_w++;
        x = mulq15(x * 2, dmix);
        yl += x;
        yr += x;
        /* reverb: two diffusers, then the four lines */
        a = mulq15(rev_in[i], 13000);
        {
            int16_t *c = rev_ap;
            for (k = 0; k < 2u; k++) {
                int32_t b = c[fx.ap_i[k]], v = a + (b >> 1);
                c[fx.ap_i[k]] = (int16_t)clamp(v, -32768, 32767);
                a = b - (v >> 1);
                if (++fx.ap_i[k] >= REV_AP[k])
                    fx.ap_i[k] = 0;
                c += REV_AP[k];
            }
        }
        {
            int16_t *c = rev_line;
            int32_t s0, s1, d0, d1, r = ma + (((mb - ma) * (int32_t)i) >> CTL_LOG2);   /* (between the two: never below 0) */
            uint32_t ri = fx.line_i[0] + ((uint32_t)r >> 8), rj;
            if (ri >= L0)                                   /* (the oldest sample is at line_i: reading */
                ri -= L0;                                   /* past it shortens line 0 by 0..2 REV_MOD) */
            rj = ri + 1u >= L0 ? 0u : ri + 1u;
            o0 = c[ri] + (((c[rj] - c[ri]) * (r & 255) + frnd) >> 8);
            o1 = c[B1 + fx.line_i[1]];
            o2 = c[B2 + fx.line_i[2]];
            o3 = c[B3 + fx.line_i[3]];
            s0 = o0 + o1, d0 = o0 - o1, s1 = o2 + o3, d1 = o2 - o3;   /* Hadamard / 2: each feeds all four */
            if (!frnd) {
                fx.line_lp[0] += mulq15(((s0 + s1) >> 1) - fx.line_lp[0], lpk);
                fx.line_lp[1] += mulq15(((d0 + d1) >> 1) - fx.line_lp[1], lpk);
                fx.line_lp[2] += mulq15(((s0 - s1) >> 1) - fx.line_lp[2], lpk);
                fx.line_lp[3] += mulq15(((d0 - d1) >> 1) - fx.line_lp[3], lpk);
                c[fx.line_i[0]] = (int16_t)clamp(mulq15(fx.line_lp[0], g) + a, -32768, 32767);
                c[B1 + fx.line_i[1]] = (int16_t)clamp(mulq15(fx.line_lp[1], g) - a, -32768, 32767);
                c[B2 + fx.line_i[2]] = (int16_t)clamp(mulq15(fx.line_lp[2], g) + a, -32768, 32767);
                c[B3 + fx.line_i[3]] = (int16_t)clamp(mulq15(fx.line_lp[3], g) - a, -32768, 32767);
            } else {                                    /* FREEZE: a loop gain of exactly 1, so no rounding that
                                                         * leans one way (a DC that would grow while held): the
                                                         * halves towards zero, no damping or gain multiply */
                fx.line_lp[0] = HALF0(s0 + s1), fx.line_lp[1] = HALF0(d0 + d1);
                fx.line_lp[2] = HALF0(s0 - s1), fx.line_lp[3] = HALF0(d0 - d1);
                c[fx.line_i[0]] = (int16_t)clamp(fx.line_lp[0] + a, -32768, 32767);
                c[B1 + fx.line_i[1]] = (int16_t)clamp(fx.line_lp[1] - a, -32768, 32767);
                c[B2 + fx.line_i[2]] = (int16_t)clamp(fx.line_lp[2] + a, -32768, 32767);
                c[B3 + fx.line_i[3]] = (int16_t)clamp(fx.line_lp[3] - a, -32768, 32767);
            }
            if (++fx.line_i[0] >= L0) fx.line_i[0] = 0;
            if (++fx.line_i[1] >= REV_LINE[1]) fx.line_i[1] = 0;
            if (++fx.line_i[2] >= REV_LINE[2]) fx.line_i[2] = 0;
            if (++fx.line_i[3] >= REV_LINE[3]) fx.line_i[3] = 0;
        }
        wet_l[i] = yl + o0 + o2;
        wet_r[i] = yr + o1 - o3;
        rev_l[i] = o0 + o2;
        rev_r[i] = o1 - o3;
    }
}

/* ---- the track FILTER (SLOOP fx.c, unchanged) ---- */
typedef struct {
    int32_t cut;                                        /* now, 0..127 << 8 (CUTOFF_HZ index) */
    int8_t mode;                                        /* -1 LP, 1 HP, 0 off */
    uint8_t seed;                                       /* (2.6) bit per channel: the LP's state from its next sample */
    int32_t z[4][2];                                    /* the SVF states, per channel */
} djf_t;

/* the knob v -> this block's coefficients in *c; 0 = bypassed (nothing to do). reso: tsvf_coef's resonance */
static int djf_block(djf_t *f, int32_t v, tsvf_t *c, int32_t reso)
{
    int32_t to;
    if (v < 0 && f->mode >= 0) {                        /* (switching side: from open) */
        f->mode = -1;
        f->cut = 127 << 8;
        memset(f->z, 0, sizeof f->z);
        f->seed = 15;                                   /* (2.6) the low-pass starts where the signal is: from rest
                                                         * its output began at 0 under a sounding note (a click) */
    } else if (v > 0 && f->mode <= 0) {
        f->mode = 1;
        f->cut = 0;
        memset(f->z, 0, sizeof f->z);
    }
    if (!f->mode)
        return 0;
    to = f->mode < 0 ? (v < 0 ? (127 << 8) + v * 90 * 4 : 127 << 8) : (v > 0 ? v * 90 * 4 : 0);
    f->cut += clamp(to - f->cut, -384, 384);            /* ~1.5 index a block */
    if (!v && f->cut == to) {
        f->mode = 0;                                    /* fully open again: off */
        return 0;
    }
    tsvf_coef(c, f->cut, reso);
    return 1;
}
/* n samples of channel ch through the filter (x within +-140000) */
static void djf_run(djf_t *f, const tsvf_t *c, int32_t *b, uint32_t n, uint32_t ch)
{
    uint32_t i;
    int32_t *z = f->z[ch];
    if ((f->seed >> ch) & 1u) {
        f->seed &= (uint8_t)~(1u << ch);
        if (f->mode < 0 && n) {
            z[0] = 0;                                   /* at rest on this input: the output is it (DC: exact) */
            z[1] = clamp(b[0], -140000, 140000);
        }
    }
    for (i = 0; i < n; i++) {
        int32_t x = clamp(b[i], -140000, 140000), y = tsvf_lp(c, x, &z[0], &z[1]);
        b[i] = f->mode < 0 ? y : x - y;
    }
}
/* a track's filter on a signal at 4x its level (b >> 2 into the filter's range, back << 2) */
static void tflt_run(djf_t *f, const tsvf_t *c, int32_t *b, uint32_t n, uint32_t ch)
{
    uint32_t i;
    for (i = 0; i < n; i++)
        b[i] >>= 2;
    djf_run(f, c, b, n, ch);
    for (i = 0; i < n; i++)
        b[i] *= 4;
}
#undef track_t

/* ---------------------------------------------------------------- COMPOSER: the channels' effects --- */
enum { FX_REV, FX_DLY, FX_CHO, FX_CUT, FX_RESO, FX_DRIVE, FX_N };
static const uint8_t FX_MAX[FX_N] = {127, 127, 127, 127, 10, 127};
static uint8_t fxp[NPART][FX_N];
static fxtrack_t fxt[NPART];
static djf_t fxf[NPART];

/* each role's starting effects: REVERB, DELAY, CHORUS (FILTER open, RESO 0, DRIVE 0) */
static const uint8_t FX_ROLE[ROLE_COUNT][3] = {
    [ROLE_LEAD] = {30, 20, 0}, [ROLE_COUNTER] = {30, 10, 0}, [ROLE_CHORDS] = {40, 0, 30}, [ROLE_PAD] = {70, 0, 40},
    [ROLE_BASS] = {0, 0, 0}, [ROLE_DRUMS] = {0, 0, 0}, [ROLE_PERC] = {20, 0, 0}, [ROLE_ARP] = {30, 40, 0},
    [ROLE_DRONE] = {60, 0, 20}, [ROLE_FX] = {60, 30, 0}, [ROLE_OTHER] = {20, 0, 0},
};
static void fx_defaults(uint32_t role, uint8_t *v)
{
    const uint8_t *r = FX_ROLE[role < ROLE_COUNT ? role : ROLE_OTHER];
    v[FX_REV] = r[0], v[FX_DLY] = r[1], v[FX_CHO] = r[2], v[FX_CUT] = 127, v[FX_RESO] = 0, v[FX_DRIVE] = 0;
}
static uint32_t fx_clamp(uint32_t par, uint32_t v) { return par < FX_N ? (v > FX_MAX[par] ? FX_MAX[par] : v) : 0u; }

/* stop: the buses' return fades out over one block, then every line is cleared (silence at once, as the voices) */
static uint8_t fx_kill;
static void fx_clear(void)
{
    __builtin_memset(dly_buf, 0, sizeof dly_buf);
    __builtin_memset(cho_buf, 0, sizeof cho_buf);
    __builtin_memset(rev_line, 0, sizeof rev_line);
    __builtin_memset(rev_ap, 0, sizeof rev_ap);
    __builtin_memset(&fx, 0, sizeof fx);
    dc_l = dc_r = dce_l = dce_r = 0;                     /* (the DC blocker's slow offset after the sound: gone too) */
}

/* a fresh start (synth_init): every effect dry and open, every state at rest */
static void fx_reset(void)
{
    uint32_t ch;
    fx_clear();
    __builtin_memset(fxp, 0, sizeof fxp);
    __builtin_memset(fxt, 0, sizeof fxt);
    __builtin_memset(fxf, 0, sizeof fxf);
    for (ch = 0; ch < NPART; ch++) fxp[ch][FX_CUT] = 127;
    __builtin_memset(&lim_dac, 0, sizeof lim_dac);
    lim_dac.env = LIM_T, lim_dac.g = 32768;
    fx_kill = 0;
    fx_tempo_x10 = 1200;
}

/* a channel's signal through DRIVE then FILTER (SLOOP's order: DIST, then the track FILTER) */
static void fx_channel(uint32_t ch, int32_t *b, uint32_t n)
{
    const uint8_t *p = fxp[ch];
    int32_t v = p[FX_CUT] >= 127u ? 0 : -(int32_t)((127u - p[FX_CUT]) * 64u / 127u);   /* SLOOP's FILT: -64 .. 0 */
    tsvf_t fc;
    fxt[ch].p[P_DIST] = p[FX_DRIVE];
    track_dist(&fxt[ch], b, n);
    if (djf_block(&fxf[ch], v, &fc, 40 + (int32_t)p[FX_RESO] * 7))   /* (SLOOP's tflt_reso) */
        tflt_run(&fxf[ch], &fc, b, n, 0);
}
