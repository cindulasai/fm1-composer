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
    for (i = 0; i < n; i++) { lk.held[k][2u + i] = c[i]; LK_ON(ch, c[i], LK_VEL); }
}

static void lk_up(uint32_t k)
{
    uint32_t i;
    lk.down &= ~(1u << k);
    lk.modkeys &= ~(1u << k);
    if (!lk.held[k][0]) return;
    for (i = 0; i < lk.held[k][1]; i++) LK_OFF(lk.held[k][0] - 1u, lk.held[k][2u + i]);
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
        LK_OFF(c - 1u, d1);
    }
    if (type == 0x90u && d2) {                  /* (a note-on at velocity 0 is a note-off) */
        c = mch < nch ? mch : sel;
        midi_sel_set(mch, d1, c + 1u);
        LK_ON(c, d1, d2);
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
            LK_OFF(c - 1u, n);
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
