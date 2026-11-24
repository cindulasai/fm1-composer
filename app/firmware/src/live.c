/* SPDX-License-Identifier: GPL-3.0-only */
/* live.c (US-24): the PLAYER's live keyboard and MIDI IN, played over the piece.
 *   KEYS: the 27 note keys play the selected channel's sound in the piece's key and scale (text2midi's key, carried in
 *   the piece's sections table, else lk_estimate's guess from its notes): the white keys walk the scale with C4 = the
 *   root, the black keys silent; or every key as it is (CHROMATIC). Auto-chords (TRIAD 7TH 9TH SUS4 POWER): a white
 *   key plays the chord of its degree, up to 4 notes, voice-led from the last one; the black keys are then SLOOP's
 *   CHORD+ modifiers (F# minor/major third, G# +7th, A# sus4, C# +9th, D# inversion), held before the white key.
 *   MIDI IN: notes play as sent (no scale, as SLOOP), MIDI channel 1-8 on COMPOSER's channel 1-8 (one the piece does
 *   not have: the selected one), other channels on the selected one; each note-off ends what its note-on started.
 * The includer defines LK_ON(ch, note, vel) and LK_OFF(ch, note): composer.c queues them to synth_live_on / _off.
 *
 * The scale and chord code (SCALE_MASK .. chord_play_notes) and midi_sel_get / midi_sel_set are SLOOP's
 * (firmware/src/seq.c @ bac9c05, GPL-3.0-only, Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton
 * Instruments), copied unchanged behind a shim: SLOOP's functions read a track's parameters (t->p[P_ROOT], P_SCALE,
 * P_QUANT, P_TRANS, P_CHORD, P_VLEAD); here one lk_track_t holds them. Each COMPOSER change is marked COMPOSER:
 * (app/firmware/SLOOP-PROVENANCE.md). */

/* COMPOSER: the shim. P_ROOT is synth.c's id, P_TRANS sloop_glue.c's (SLOOP's ids for its presets); the rest follow */
enum { P_SCALE = P_TRANS + 1, P_QUANT, P_CHORD, P_VLEAD, LK_NP };
typedef struct { int16_t p[LK_NP]; uint8_t part; } lk_track_t;
#define track_t lk_track_t                      /* COMPOSER: SLOOP's code below calls its parameter type track_t */
#define trk_index(t) ((uint32_t)(t)->part)      /* COMPOSER: the channel a chord is voice-led on */

/* ---- SLOOP firmware/src/seq.c @ bac9c05, unchanged ---- */
static const uint16_t SCALE_MASK[] = {
    0xFFF,                                   /* CHR */
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11),   /* MAJ */
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 10),   /* MIN */
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 10),   /* DOR */
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 10),   /* MIX */
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 7) | (1 << 9),                          /* PEN */
    (1 << 0) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 10),                         /* MPEN */
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 11),   /* HARM */
    (1 << 0) | (1 << 1) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 8) | (1 << 10),   /* PHRY */
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 9) | (1 << 11),   /* LYD */
    (1 << 0) | (1 << 1) | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 8) | (1 << 10),   /* LOC */
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 7) | (1 << 9) | (1 << 11),   /* MEL (ascending) */
    (1 << 0) | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 7) | (1 << 10),              /* BLUES (minor) */
    (1 << 0) | (1 << 2) | (1 << 4) | (1 << 6) | (1 << 8) | (1 << 10),              /* WHOLE */
    (1 << 0) | (1 << 1) | (1 << 3) | (1 << 4) | (1 << 6) | (1 << 7) | (1 << 9) | (1 << 10), /* DIMHW */
    (1 << 0) | (1 << 2) | (1 << 3) | (1 << 5) | (1 << 6) | (1 << 8) | (1 << 9) | (1 << 11), /* DIMWH */
};
#define NSCALES (sizeof SCALE_MASK / sizeof SCALE_MASK[0])
#define KB_SILENT 255u

static uint32_t scale_mask(const track_t *t)
{
    return SCALE_MASK[clamp(t->p[P_SCALE], 0, NSCALES - 1)];
}

/* key k's note on a scaled track with the octave buttons at oct (KB_SILENT: none): QNT, ROOT, SCALE, TRN as they are
 * now. WHITE (and chord mode): the white keys walk the scale from C4 = the root, the black keys are silent; SNAP: every
 * key, rounded down into the scale. The keys (kb_map), and the notes the arp holds (arp_list, 2.6 after Felucca 1.5
 * #191: a latched chord follows TRN, ROOT, SCALE and QNT, in the octave its keys were pressed in) */
static uint32_t kb_pitch(const track_t *t, uint32_t k, int32_t oct)
{
    static const int8_t DEGREE[12] = {0, -1, 1, -1, 2, 3, -1, 4, -1, 5, -1, 6};
    int32_t n = 53 + (int32_t)k;
    if (t->p[P_QUANT] == 1 && !t->p[P_CHORD]) {  /* SNAP: every key, rounded down to the scale (the old ON) */
        uint32_t mask = scale_mask(t), guard = 12;
        n += 12 * oct + t->p[P_TRANS];
        while (guard-- && !((mask >> (uint32_t)((n - t->p[P_ROOT] + 120) % 12)) & 1u))
            n--;
        return (uint32_t)clamp(n, 0, 127);
    }
    if (t->p[P_QUANT] == 2 || t->p[P_CHORD]) {   /* WHITE: white keys walk the scale, black keys are silent */
        uint32_t mask = t->p[P_CHORD] && !t->p[P_SCALE] ? SCALE_MASK[2] : scale_mask(t), i;
        int32_t count = 0, degree = DEGREE[n % 12], wo;
        if (degree < 0)
            return KB_SILENT;
        /* C4 is the root. Walk scale degrees on successive white keys, including
         * below C4; scales with 5, 6, 8 or 12 notes still have no duplicated degrees. */
        degree += (n / 12 - 5) * 7;
        for (i = 0; i < 12u; i++)
            count += (mask >> i) & 1u;
        wo = degree / count;
        degree %= count;
        if (degree < 0) {
            degree += count;
            wo--;
        }
        for (i = 0; i < 12u; i++)
            if ((mask >> i) & 1u) {
                if (!degree)
                    break;
                degree--;
            }
        n = 60 + t->p[P_ROOT] + 12 * wo + (int32_t)i;
    }
    return (uint32_t)clamp(n + 12 * oct + t->p[P_TRANS], 0, 127);
}

/* chord mode (P_CHORD): the chord of the scale built on note n (in the scale; CHR: minor), into c[];
 * the notes it holds (<= 4, the most a step keeps) */
static const int8_t CHORD_DEG[6][4] = {
    {0, -1, -1, -1},                     /* OFF */
    {0, 2, 4, -1},                       /* TRIAD: 1 3 5 */
    {0, 2, 4, 6},                        /* 7TH: 1 3 5 7 */
    {0, 2, 6, 8},                        /* 9TH: 1 3 7 9 (the lo-fi / R&B voicing) */
    {0, 3, 4, -1},                       /* SUS4: 1 4 5 */
    {0, -1, -1, -1},                     /* POWER: 1 5 8 (semitones, below) */
};
static uint32_t chord_notes(const track_t *t, uint32_t n, uint8_t *c)
{
    uint32_t type = (uint32_t)clamp(t->p[P_CHORD], 0, 5), mask = t->p[P_SCALE] ? scale_mask(t) : SCALE_MASK[2];
    uint32_t k = 0, j;
    if (type == 5u) {
        static const uint8_t PW[3] = {0, 7, 12};
        for (j = 0; j < 3u; j++)
            if (n + PW[j] < 128u)
                c[k++] = (uint8_t)(n + PW[j]);
        return k;
    }
    for (j = 0; j < 4u && CHORD_DEG[type][j] >= 0; j++) {
        int32_t m = (int32_t)n, d = CHORD_DEG[type][j], guard = 48;
        while (d > 0 && guard--) {                       /* d scale degrees up */
            m++;
            if ((mask >> (uint32_t)((m - t->p[P_ROOT] + 120) % 12)) & 1u)
                d--;
        }
        if (m < 128)
            c[k++] = (uint8_t)m;
    }
    return k;
}

/* CHORD+ (2.4, after HiChord / minichord): in chord mode the black keys are modifiers. Held while a white key
 * plays (or pressed while it is held: the chord changes under the finger), they change its chord: F# flips its
 * third (major <-> minor), G# adds the 7th, A# makes it sus4, C# adds the 9th, D# inverts it (its lowest note an
 * octave up); several at once combine. P_VLEAD ON voices each chord nearest the last one played on the track. */
enum { CM_MINOR = 1, CM_SEVEN = 2, CM_SUS4 = 4, CM_NINE = 8, CM_INV = 16 };
static uint32_t chord_mod_of_key(uint32_t k)            /* key k's modifier (0: a white key) */
{
    switch ((53u + k) % 12u) {
    case 6: return CM_MINOR;                            /* F# */
    case 8: return CM_SEVEN;                            /* G# */
    case 10: return CM_SUS4;                            /* A# */
    case 1: return CM_NINE;                             /* C# */
    case 3: return CM_INV;                              /* D# */
    default: return 0;
    }
}
/* COMPOSER: NPART is synth.c's 8 channels here (SLOOP: 3 parts), so the voice leading remembers a chord per channel */
static uint8_t vl_prev[NPART][4], vl_n[NPART];          /* the last chord played on each part (voice leading) */
static uint32_t scale_up(const track_t *t, uint32_t n, uint32_t deg)   /* deg scale degrees above n */
{
    uint32_t mask = t->p[P_SCALE] ? scale_mask(t) : SCALE_MASK[2];
    int32_t m = (int32_t)n, guard = 48;
    while (deg > 0 && guard--) {
        m++;
        if ((mask >> (uint32_t)((m - t->p[P_ROOT] + 120) % 12)) & 1u)
            deg--;
    }
    return (uint32_t)m;
}
static void sort_notes(uint8_t *c, uint32_t n)
{
    uint32_t a, b;
    for (a = 1; a < n; a++)
        for (b = a; b > 0 && c[b - 1u] > c[b]; b--) {
            uint8_t x = c[b]; c[b] = c[b - 1u]; c[b - 1u] = x;
        }
}
/* the chord of white key n with the modifiers held, voiced (VLEAD); its notes (<= 4) into c[] */
static uint32_t chord_play_notes(track_t *t, uint32_t n, uint32_t mods, uint8_t *c)
{
    uint32_t k = chord_notes(t, n, c), j, type = (uint32_t)clamp(t->p[P_CHORD], 0, 5), part = trk_index(t);
    if (type != 5u && k >= 2u && mods) {                /* (POWER: the inversion only) */
        uint32_t third = scale_up(t, n, 2u);
        for (j = 0; j < k; j++) {
            if (c[j] == third && (mods & CM_SUS4))
                c[j] = (uint8_t)scale_up(t, n, 3u);     /* the 4th instead of the 3rd */
            else if (c[j] == third && (mods & CM_MINOR))
                c[j] = (uint8_t)(c[j] - n == 4u ? c[j] - 1u : c[j] - n == 3u ? c[j] + 1u : c[j]);
        }
        if ((mods & CM_SEVEN) && k < 4u) {
            uint32_t s7 = scale_up(t, n, 6u);
            for (j = 0; j < k && c[j] != s7; j++)
                ;
            if (j == k && s7 < 128u)
                c[k++] = (uint8_t)s7;
        }
        if (mods & CM_NINE) {
            uint32_t s9 = scale_up(t, n, 8u), s5 = scale_up(t, n, 4u);
            for (j = 0; j < k && c[j] != s9; j++)
                ;
            if (j == k && s9 < 128u) {
                if (k < 4u) {
                    c[k++] = (uint8_t)s9;
                } else {                                /* four already: the 9th for the 5th */
                    for (j = 0; j < k && c[j] != s5; j++)
                        ;
                    c[j < k ? j : k - 1u] = (uint8_t)s9;
                }
            }
        }
    }
    sort_notes(c, k);
    if (t->p[P_VLEAD] && part < NPART && vl_n[part] && k) {   /* the inversion and octave nearest the last chord */
        uint8_t best[4], cand[4];
        int32_t bcost = 0x7FFFFFFF, inv, oct;
        for (inv = 0; inv < (int32_t)k; inv++)
            for (oct = -1; oct <= 1; oct++) {
                int32_t cost = 0, ok = 1;
                for (j = 0; j < k; j++) {
                    int32_t v = c[j] + 12 * oct + (j < (uint32_t)inv ? 12 : 0);
                    if (v < 24 || v > 108)
                        ok = 0;
                    cand[j] = (uint8_t)(v < 0 ? 0 : v > 127 ? 127 : v);
                }
                if (!ok)
                    continue;
                sort_notes(cand, k);
                for (j = 0; j < k; j++) {
                    int32_t d = (int32_t)cand[j] - vl_prev[part][j < vl_n[part] ? j : vl_n[part] - 1u];
                    cost += d < 0 ? -d : d;
                }
                if (cost < bcost) {
                    bcost = cost;
                    memcpy(best, cand, k);
                }
            }
        if (bcost != 0x7FFFFFFF)
            memcpy(c, best, k);
    }
    if ((mods & CM_INV) && k >= 2u && c[0] + 12u < 128u) {   /* an inversion: the lowest an octave up */
        c[0] = (uint8_t)(c[0] + 12u);
        sort_notes(c, k);
    }
    if (part < NPART) {
        memcpy(vl_prev[part], c, k);
        vl_n[part] = (uint8_t)k;
    }
    return k;
}

/* a channel that plays the selected track: its note-off goes to the track its note-on went to,
 * even when another track was selected in between (else that note would hang) */
static uint8_t midi_sel_on[16][64];                   /* per channel and note, a nibble: track + 1, 0 = none (sloop-lab:
                                                       * 1 KB, not 2: the value is 0..NTRK) */
static inline uint32_t midi_sel_get(uint32_t ch, uint32_t note)
{
    return (midi_sel_on[ch & 15u][(note & 127u) >> 1] >> (4u * (note & 1u))) & 15u;
}
static inline void midi_sel_set(uint32_t ch, uint32_t note, uint32_t v)
{
    uint8_t *b = &midi_sel_on[ch & 15u][(note & 127u) >> 1];
    uint32_t sh = 4u * (note & 1u);
    *b = (uint8_t)((*b & ~(15u << sh)) | ((v & 15u) << sh));
}
/* ---- end of SLOOP's code ---- */
#undef track_t
#undef trk_index

/* ---- COMPOSER: the keys, the piece's key, MIDI IN ---- */
#define LK_KEYS 27u
#define LK_VEL 100u                             /* (the FM-1's keys have no velocity) */
#define LK_CHROMATIC 16u                        /* the scale position past SLOOP's 16 scales: every key as it is */
static const char *const LK_SCALE_NAME[17] = {"CHR", "MAJ", "MIN", "DOR", "MIX", "PEN", "MPEN", "HARM", "PHRY", "LYD",
                                              "LOC", "MEL", "BLUES", "WHOLE", "DIMHW", "DIMWH", "CHROMATIC"};   /* SLOOP params.c N_SCALE */
static const char *const LK_ROOT_NAME[12] = {"C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"};
static const char *const LK_CHORD_NAME[6] = {"", "TRIAD", "7TH", "9TH", "SUS4", "POWER"};   /* SLOOP CHORD_DEG's order */
static struct {
    lk_track_t t;                               /* root, scale, QNT (2: WHITE, 0: chromatic), chord, voice leading */
    uint8_t on;                                 /* KEYS mode */
    int8_t oct;                                 /* OCT- / OCT+: -3 .. 3 */
    uint32_t down, modkeys;                     /* the keys sounding (or modifying) now; the CHORD+ keys held */
    uint8_t held[LK_KEYS][6];                   /* per key: channel + 1 (0: none), note count, its notes */
} lk;

/* the piece's key when it carries none: the pitch classes of its pitched notes, weighted by length, against the
 * Krumhansl-Kessler major and minor profiles (x10, times 12 less their sum: centred); the best root and MAJ / MIN.
 * (Channels past nch have no notes: piece_view) */
static void lk_estimate(const piece_view_t *v, uint32_t *root, uint32_t *scale)
{
    static const int16_t KK[2][12] = {{349, -155, 1, -143, 109, 73, -119, 205, -131, 25, -143, -71},
                                      {311, -121, -25, 203, -133, -25, -145, 131, 35, -121, -49, -61}};
    uint32_t h[12] = {0}, ch, i, pc;
    int64_t best = 0;
    *root = 0;
    *scale = 1;                                 /* (nothing pitched: C major) */
    for (ch = 0; ch < PIECE_MAX_CH; ch++) {
        piece_cur_t c;
        piece_note_t n;
        if (v->ch[ch].role == ROLE_DRUMS || v->ch[ch].role == ROLE_PERC) continue;
        piece_cursor(&v->ch[ch], &c);
        while (piece_next(&c, &n)) h[n.pitch % 12u] += n.dur;
    }
    for (i = 0; i < 24u; i++) {                 /* C major .. B major, C minor .. B minor */
        int64_t s = 0;
        for (pc = 0; pc < 12u; pc++) s += (int64_t)KK[i / 12u][(pc + 12u - i % 12u) % 12u] * h[pc];
        if (s > best) { best = s; *root = i % 12u; *scale = i / 12u + 1u; }
    }
}

static void lk_key(uint32_t root, uint32_t scale)     /* root 0..11, scale 0..15 (SLOOP's) or LK_CHROMATIC */
{
    lk.t.p[P_ROOT] = (int16_t)(root % 12u);
    lk.t.p[P_QUANT] = scale >= LK_CHROMATIC ? 0 : 2;
    lk.t.p[P_SCALE] = (int16_t)(scale >= LK_CHROMATIC ? 0u : scale);
}
static uint32_t lk_scale(void) { return lk.t.p[P_QUANT] ? (uint32_t)lk.t.p[P_SCALE] : LK_CHROMATIC; }

/* US-26: what plays live is also heard by the recorder (below) */
static void rc_on(uint32_t ch, uint32_t note, uint32_t vel);
static void rc_off(uint32_t ch, uint32_t note);
#define LV_ON(ch, note, vel) do { LK_ON(ch, note, vel); rc_on(ch, note, vel); } while (0)
#define LV_OFF(ch, note) do { LK_OFF(ch, note); rc_off(ch, note); } while (0)

/* a piece opened: its key (carried, else estimated), chords off, the octave 0, no voice leading yet */
static void lk_open(const piece_view_t *v)
{
    uint32_t root, scale;
    if (v->key) { root = v->key - 1u; scale = v->scale; }
    else lk_estimate(v, &root, &scale);
    memset(&lk.t, 0, sizeof lk.t);
    lk_key(root, scale);
    lk.t.p[P_VLEAD] = 1;
    lk.oct = 0;
    memset(vl_n, 0, sizeof vl_n);
}

static uint32_t lk_mods(void)
{
    uint32_t b, m = 0;
    for (b = lk.modkeys; b; b &= b - 1u) m |= chord_mod_of_key((uint32_t)__builtin_ctz(b));
    return m;
}

static void lk_down(uint32_t k, uint32_t ch)
{
    uint8_t c[4];
    uint32_t n, i, note;
    lk.down |= 1u << k;
    if (lk.t.p[P_CHORD] && chord_mod_of_key(k)) { lk.modkeys |= 1u << k; return; }   /* CHORD+ */
    note = kb_pitch(&lk.t, k, lk.oct);
    if (note == KB_SILENT) return;
    lk.t.part = (uint8_t)ch;
    if (lk.t.p[P_CHORD]) n = chord_play_notes(&lk.t, note, lk_mods(), c);
    else { c[0] = (uint8_t)note; n = 1; }
    lk.held[k][0] = (uint8_t)(ch + 1u);
    lk.held[k][1] = (uint8_t)n;
    for (i = 0; i < n; i++) { lk.held[k][2u + i] = c[i]; LV_ON(ch, c[i], LK_VEL); }
}

static void lk_up(uint32_t k)
{
    uint32_t i;
    lk.down &= ~(1u << k);
    lk.modkeys &= ~(1u << k);
    if (!lk.held[k][0]) return;
    for (i = 0; i < lk.held[k][1]; i++) LV_OFF(lk.held[k][0] - 1u, lk.held[k][2u + i]);
    lk.held[k][0] = 0;
}

/* the keys now (fm1_in.notes) and the presses since the last pass (a press shorter than a pass too): presses play on
 * channel ch, releases end what their press started (whatever is selected now) */
static void lk_keys(uint32_t now, uint32_t pressed, uint32_t ch)
{
    uint32_t b, go;
    now &= (1u << LK_KEYS) - 1u;
    for (b = lk.down & ~now; b; b &= b - 1u) lk_up((uint32_t)__builtin_ctz(b));
    go = (pressed | now) & ((1u << LK_KEYS) - 1u) & ~lk.down;
    for (b = go; b; b &= b - 1u)                /* the CHORD+ keys first: a modifier pressed with its key counts */
        if (lk.t.p[P_CHORD] && chord_mod_of_key((uint32_t)__builtin_ctz(b))) lk_down((uint32_t)__builtin_ctz(b), ch);
    for (b = go & ~lk.down; b; b &= b - 1u) lk_down((uint32_t)__builtin_ctz(b), ch);
    for (b = lk.down & ~now; b; b &= b - 1u) lk_up((uint32_t)__builtin_ctz(b));   /* (pressed and let go within the pass) */
}

static void lk_keys_off(void)                   /* every key's notes end (KEYS off, leaving the PLAYER) */
{
    uint32_t k;
    for (k = 0; k < LK_KEYS; k++) lk_up(k);
}

/* a MIDI message (status, data 1, data 2) with the selected channel sel and the piece's nch channels */
static void lk_midi(uint32_t st, uint32_t d1, uint32_t d2, uint32_t sel, uint32_t nch)
{
    uint32_t mch = st & 15u, type = st & 0xF0u, c;
    if (d1 > 127u || (type != 0x80u && type != 0x90u)) return;
    c = midi_sel_get(mch, d1);
    if (c) {                                    /* its note-off; or the same note again without one: ended first */
        midi_sel_set(mch, d1, 0);
        LV_OFF(c - 1u, d1);
    }
    if (type == 0x90u && d2) {                  /* (a note-on at velocity 0 is a note-off) */
        c = mch < nch ? mch : sel;
        midi_sel_set(mch, d1, c + 1u);
        LV_ON(c, d1, d2);
    }
}

/* SLOOP 2.8.3 (seq.c events_block): a note message was lost (the ring full): every note MIDI holds ends, none stays
 * stuck for a lost note-off. Also leaving the piece */
static void lk_midi_off(void)
{
    uint32_t i;
    for (i = 0; i < 16u * 64u; i++)             /* (a byte: two notes) */
        while (midi_sel_on[i >> 6][i & 63u]) {
            uint32_t mch = i >> 6, n = (i & 63u) << 1 | (midi_sel_get(i >> 6, (i & 63u) << 1) ? 0u : 1u);
            uint32_t c = midi_sel_get(mch, n);
            midi_sel_set(mch, n, 0);
            LV_OFF(c - 1u, n);
        }
}

/* "KEYS D DOR TRIAD OCT+1" */
static void lk_label(char *b, uint32_t max)
{
    static const char *const OCT[7] = {" OCT-3", " OCT-2", " OCT-1", "", " OCT+1", " OCT+2", " OCT+3"};
    const char *o = OCT[lk.oct + 3];
    uint32_t s = lk_scale();
    if (s == LK_CHROMATIC) ui_fmt(b, max, "KEYS CHROMATIC%s%s%s", lk.t.p[P_CHORD] ? " " : "", LK_CHORD_NAME[lk.t.p[P_CHORD]], o);
    else ui_fmt(b, max, "KEYS %s %s%s%s%s", LK_ROOT_NAME[lk.t.p[P_ROOT]], LK_SCALE_NAME[s], lk.t.p[P_CHORD] ? " " : "",
                LK_CHORD_NAME[lk.t.p[P_CHORD]], o);
}

/* ---- US-26: REC, live play recorded into the selected channel ----
 * PLAYER: REC, then a bar of clicks (the count-in: playing, the next whole bar; stopped, a bar at the tempo, then it
 * plays from the start), then each note played live on the channel selected at REC (the keys, or MIDI IN that plays
 * on it) goes into the piece where it was heard: SLOOP's rec_target takes the key-to-ear latency (REC_LAT) back and
 * picks the grid step (or, quantize OFF, the step and its nudge: the tick it was heard at). It is written when its key
 * comes up (its velocity, the length it was held), once the playhead is past it (it must not sound twice). OVERDUB
 * keeps what is there; REPLACE clears the channel's notes just ahead of the playhead as it goes (each lap replaces the
 * last one). A full piece (32 KB) or a pass too long to undo stops the pass: what fits is kept, the footer says so.
 * The pass is one undo (edit.c's pass log; FX in EDIT or STEPS). The edits go through edit.c, held from the player. */
/* COMPOSER: the shim for SLOOP's rec_target. Its units: a sample adds song.g[G_BPM] (SLOOP: the BPM; here the
 * tempo x10, so a beat is 60 x 10 x 44100 and a tick RC_TICK); trk_grid: the step playing, how far into it, its length */
#define MICRO_MIN (-32)                         /* (SLOOP core.h) a step's nudge, in 1/64 of the step */
#define MICRO_MAX 31
#define RC_TICK (PL_TICK_UNITS / PIECE_PPQ)     /* 275625 */
enum { G_BPM };
typedef struct { uint32_t seq_abs; } rc_track_t;
static struct { int16_t g[1]; } rc_song;
static uint32_t rc_grid_at(uint32_t *into, uint32_t *slen);
#define song rc_song                            /* COMPOSER: fx.c has a song of its own */
#define track_t rc_track_t
#define trk_grid(t, into, slen) rc_grid_at(into, slen)

/* ---- SLOOP firmware/src/seq.c @ bac9c05, unchanged (recording) ---- */
/* key to ear, in samples: the key's debounce (~3 ms) and the audio out buffer (HALF_FRAMES to
 * 2 x HALF_FRAMES, ~9 ms on average). A note played in time with what the player hears reaches
 * the sequencer this much later than the sound it was played to: recording takes it back. */
#define REC_LAT 512u

/* the step a note played now goes into (as heard: REC_LAT earlier): the one playing, or the next one
 * when it is past the middle of the playing one; *later: it has not played yet (it must not sound twice) */
static int8_t rec_m;                     /* (2.7) the last rec_target's note off the grid, as a nudge (rec_micro_set) */
static uint32_t rec_target(const track_t *t, uint32_t *later)
{
    uint32_t into, slen, abs = trk_grid(t, &into, &slen), half = slen / 2u, lat = REC_LAT * (uint32_t)song.g[G_BPM];
    int32_t off = (int32_t)into - (int32_t)lat, m;  /* where it was heard, from the start of the playing step */
    if (into > half + (lat < half ? lat : half)) {  /* (32 bits: a step is 8 beats at most, 8 x FS x 60 x 64 < 2^31) */
        abs++;
        off -= (int32_t)slen;                       /* (before the next one: early) */
    }
    if (off < -(int32_t)slen)
        off = -(int32_t)slen;
    m = slen ? (off * 64 + (off >= 0 ? (int32_t)(slen / 2u) : -(int32_t)(slen / 2u))) / (int32_t)slen : 0;
    rec_m = (int8_t)(m < MICRO_MIN ? MICRO_MIN : m > MICRO_MAX ? MICRO_MAX : m);
    *later = abs != t->seq_abs;
    return abs;
}
/* ---- end of SLOOP's code (recording) ---- */
#undef song
#undef track_t
#undef trk_grid

#define RC_HELD 32u                             /* notes held (or waiting for the playhead) at once */
#define RC_BAR (4u * PIECE_PPQ)
#define RC_CLICK 882u                           /* a click: 20 ms */
#define RC_AMP 9000u
enum { RC_OFF, RC_COUNT, RC_ON };
static const uint8_t RC_GRID[5] = {0, 12, 24, 48, 96};
static const char *const RC_GRID_NAME[5] = {"OFF", "1/32", "1/16", "1/8", "1/4"};
typedef struct { uint32_t tick, on, dur; uint8_t pitch, vel, open; } rc_note_t;
static struct {
    uint32_t mode, ch, quant, replace, start;   /* start: counted in from stopped, the player not seen playing yet */
    uint32_t pos, acc;                          /* the playhead rec_target sees */
    uint32_t h, last;                           /* REPLACE: cleared up to tick h; the playhead the last pass */
    uint32_t n, ops, beat, nheld;               /* notes recorded this pass; edits ever; count-in beats left */
    const char *msg;                            /* for the footer */
    char txt[40];
    rc_note_t held[RC_HELD];
} rc = {.quant = 2};
static rc_track_t rc_trk;
/* the audio interrupt: ticks at the tempo since boot (the count-in's and the lengths' clock), the click */
static volatile uint32_t rc_clock, rc_go, rc_next, rc_counting;
static uint32_t rc_acc, rc_ck, rc_ph, rc_hz;

static uint32_t rc_g(void) { return RC_GRID[rc.quant] ? RC_GRID[rc.quant] : PL_STEPT; }   /* (OFF: SLOOP's 1/16 + nudge) */

static uint32_t rc_grid_at(uint32_t *into, uint32_t *slen)
{
    uint32_t g = rc_g();
    *slen = g * RC_TICK;
    *into = rc.pos % g * RC_TICK + rc.acc / PIECE_PPQ;
    return rc.pos / g;
}

/* the tick a note played now goes to */
static uint32_t rc_where(void)
{
    uint32_t later, g = rc_g(), abs;
    int32_t t;
    rc.pos = player_pos();
    rc.acc = pl_acc;
    rc_song.g[G_BPM] = (int16_t)pl_tempo;
    rc_trk.seq_abs = rc.pos / g;
    abs = rec_target(&rc_trk, &later);
    t = (int32_t)(abs * g);
    if (!RC_GRID[rc.quant]) t += (rec_m * (int32_t)g + (rec_m < 0 ? -32 : 32)) / 64;   /* the nudge, in ticks */
    return t < 0 ? 0u : (uint32_t)t % pl_v.len_ticks;   /* (before the top: on it; past the end: the loop's start) */
}

static void rc_close(rc_note_t *n)
{
    uint32_t d = rc_clock - n->on;
    n->dur = d < 1u ? 1u : d > 65535u ? 65535u : d;
    n->open = 0;
}

static void rc_off(uint32_t ch, uint32_t note)
{
    uint32_t i;
    if (ch != rc.ch) return;
    for (i = 0; i < rc.nheld; i++)
        if (rc.held[i].open && rc.held[i].pitch == note) { rc_close(&rc.held[i]); return; }
}

static void rc_on(uint32_t ch, uint32_t note, uint32_t vel)
{
    if (rc.mode != RC_ON || ch != rc.ch || !player_playing()) return;
    rc_off(ch, note);                           /* (the same note again, not let go: that one ends) */
    if (rc.nheld < RC_HELD) rc.held[rc.nheld++] = (rc_note_t){rc_where(), rc_clock, 0, (uint8_t)note, (uint8_t)vel, 1};
}

/* the notes let go into the piece (all: every one, now); the first refusal */
static int rc_flush(uint8_t *b, int all)
{
    uint32_t i = 0, g = rc_g(), pos = player_pos(), len = pl_v.len_ticks;
    int r = ED_OK;
    while (i < rc.nheld) {
        rc_note_t *n = &rc.held[i];
        uint32_t ahead = (n->tick + len - pos) % len;
        if (all && n->open) rc_close(n);
        if (n->open || (!all && ahead && ahead <= g)) { i++; continue; }   /* held; or the playhead not there yet */
        if (r == ED_OK) {
            player_hold();
            r = edit_pass_insert(b, rc.ch, n->tick, n->pitch, n->dur, n->vel);
            player_edited();
            if (r == ED_OK) { rc.n++; rc.ops++; }
            else if (r == ED_EXISTS) r = ED_OK; /* (that very note is there already) */
        }
        rc.held[i] = rc.held[--rc.nheld];
    }
    return r;
}

/* REPLACE: the channel's notes from h to a step ahead of the playhead go (a jump, a loop: from the playhead) */
static int rc_erase(uint8_t *b)
{
    uint32_t pos = player_pos(), to = pos + rc_g();
    int r = ED_OK;
    if (pos < rc.last || pos > rc.last + PIECE_PPQ) rc.h = pos;
    rc.last = pos;
    if (to > pl_v.len_ticks) to = pl_v.len_ticks;
    while (r == ED_OK && rc.h < to) {
        piece_cur_t c;
        piece_note_t x;
        int hit = 0;
        piece_cursor(&pl_v.ch[rc.ch], &c);
        while (!hit && piece_next(&c, &x) && x.tick < to) hit = x.tick >= rc.h;
        if (!hit) break;
        player_hold();
        r = edit_pass_delete(b, rc.ch, x.tick, x.pitch);
        player_edited();
        if (r == ED_OK) rc.ops++;
    }
    if (rc.h < to) rc.h = to;
    return r;
}

static void rc_label(void)                      /* "REC 1/16 OVERDUB" */
{
    ui_fmt(rc.txt, sizeof rc.txt, "REC %s %s", RC_GRID_NAME[rc.quant], rc.replace ? "REPLACE" : "OVERDUB");
    rc.msg = rc.txt;
}

/* the pass ends (r: why, ED_OK when asked): the notes held go in as they are now */
static void rc_end(uint8_t *b, int r)
{
    if (rc.mode == RC_ON && r == ED_OK) r = rc_flush(b, 1);
    rc_counting = 0;
    rc.nheld = 0;
    if (r == ED_FULL) ui_fmt(rc.txt, sizeof rc.txt, "PIECE FULL: %u NOTES KEPT", rc.n);
    else if (r == ED_PASS) ui_fmt(rc.txt, sizeof rc.txt, "PASS FULL: %u NOTES KEPT", rc.n);
    else if (r != ED_OK) ui_fmt(rc.txt, sizeof rc.txt, "CANNOT RECORD THERE");
    else if (rc.mode == RC_COUNT) ui_fmt(rc.txt, sizeof rc.txt, "REC OFF");
    else ui_fmt(rc.txt, sizeof rc.txt, "REC OFF: %u NOTES", rc.n);
    rc.msg = rc.txt;
    rc.mode = RC_OFF;
}

/* REC: channel ch; the count-in: playing, the next whole bar (this one, at its line); stopped, a bar from now */
static void rc_arm(uint32_t ch)
{
    rc.ch = ch;
    rc.n = rc.nheld = rc.beat = 0;
    rc.start = !player_playing();
    rc_go = rc_clock + (rc.start ? 0u : (RC_BAR - player_pos() % RC_BAR) % RC_BAR) + RC_BAR;
    rc_next = rc_go - RC_BAR;
    rc.mode = RC_COUNT;
    rc_counting = 1;
    edit_pass_begin();
}

static uint32_t rc_beats(uint32_t left)        /* the count-in's beats to go: 4 until its bar begins */
{
    uint32_t b = (left + PIECE_PPQ - 1u) / PIECE_PPQ;
    return b > 4u ? 4u : b;
}

/* the main loop, every pass: the count-in ends (stopped: the player starts), REPLACE clears ahead, notes go in */
static void rc_poll(uint8_t *b)
{
    int r = ED_OK;
    if (rc.mode == RC_COUNT) {
        int32_t left = (int32_t)(rc_go - rc_clock);
        if (left <= 0) {
            rc_counting = 0;
            if (rc.start) player_play();
            rc.mode = RC_ON;
            rc.h = rc.last = player_pos();
            if (rc.h % RC_BAR <= 2u) rc.h -= rc.h % RC_BAR;   /* (the bar line the count-in ended on, the clock's tick aside) */
            ui_fmt(rc.txt, sizeof rc.txt, "RECORDING %s %s", RC_GRID_NAME[rc.quant], rc.replace ? "REPLACE" : "OVERDUB");
            rc.msg = rc.txt;
        } else if (rc_beats((uint32_t)left) != rc.beat) {
            rc.beat = rc_beats((uint32_t)left);
            ui_fmt(rc.txt, sizeof rc.txt, "REC IN %u", rc.beat);
            rc.msg = rc.txt;
        }
    } else if (rc.mode == RC_ON) {
        if (player_playing()) {
            rc.start = 0;
            if (rc.replace) r = rc_erase(b);
            if (r == ED_OK) r = rc_flush(b, 0);
            if (r != ED_OK) rc_end(b, r);
        } else if (!rc.start) {
            rc_end(b, ED_OK);                   /* (stopped) */
        }
    }
}

/* the audio interrupt, a block of n frames into out (before the synth adds to it): the clock, the count-in's clicks
 * (the bar's first one higher) */
static void rc_audio(int32_t *out, uint32_t n)
{
    uint32_t i;
    rc_acc += pl_tempo * PIECE_PPQ * n;
    while (rc_acc > PL_TICK_UNITS) { rc_acc -= PL_TICK_UNITS; rc_clock++; }
    if (rc_counting && (int32_t)(rc_clock - rc_next) >= 0 && (int32_t)(rc_go - rc_next) > 0) {
        rc_hz = rc_go - rc_next == RC_BAR ? 2000u : 1000u;
        rc_next += PIECE_PPQ;
        rc_ck = RC_CLICK;
        rc_ph = 0;
    }
    for (i = 0; i < n && rc_ck; i++, rc_ck--) {
        int32_t v = (int32_t)(rc_ck * RC_AMP / RC_CLICK);
        rc_ph = (rc_ph + rc_hz) % 44100u;
        out[2u * i] += rc_ph < 22050u ? v : -v;
        out[2u * i + 1u] += rc_ph < 22050u ? v : -v;
    }
}
