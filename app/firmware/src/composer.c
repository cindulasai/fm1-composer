/* SPDX-License-Identifier: GPL-3.0-only */
/* COMPOSER: an FM-1 firmware that keeps a library of up to 8-channel pieces composed on a computer
 * (fm1c pushes them over USB-MIDI), plays them, and lets you pick each channel's sound.
 * Built on Dinghy (zvenson) / Felucca (Leo Kuroshita, Hugelton Instruments) / SLOOP (isod89): platform.c and below.
 * Sound engines: Felucca-Engines (MIT), app/engines. The chip's rules: docs/RULES.md.
 *
 * LIBRARY: SELECT turns, PLAY opens the piece and plays it.
 * PLAYER: PLAY starts / stops; KNOB1 (or SELECT) picks a channel, PRESETS its sound, KNOB2 the tempo; white keys
 * F3..F4 (1..8) mute a channel, SEL + key solos it; SAVE keeps the sounds, mutes and tempo for this piece; HOME
 * goes back to the library (and stops). */
#define APP_NAME "COMPOSER"
static void app_idle(void);
#include <stdint.h>
static int app_sysex(const uint8_t *p, uint32_t n);
#define LCD_IDLE app_idle
#define APP_SYSEX app_sysex
#include "platform.c"

#ifdef DINGHY_HOST
#define POOL
#define LIB_READ(off, p, n) host_fl_read(off, p, n)
#define LIB_ERASE(off) host_fl_erase(off)
#define LIB_WRITE(off, p, n) host_fl_write(off, p, n)
#else
#define POOL __attribute__((section(".pool")))
#endif
#define XF_POOL POOL
#include "piece.h"
#include "synth.c"
#include "player.c"
#include "lib.c"
#include "xfer.c"
#include "ui.c"

/* ---- per-piece choices, in blob 0: sounds, mutes, tempo, by piece id ---- */
#define PREF_MAGIC 0x31464550u                  /* "PEF1" */
#define NPREF 200u
typedef struct { uint32_t id, stamp; uint16_t tempo_x10; uint8_t sound[PIECE_MAX_CH], mute, used; } pref_t;
static struct { uint32_t magic, stamp; pref_t p[NPREF]; } prefs;
_Static_assert(sizeof prefs <= SECTOR_MAX, "prefs fit a blob");

static pref_t *pref_find(uint32_t id)
{
    uint32_t i;
    for (i = 0; i < NPREF; i++)
        if (prefs.p[i].used && prefs.p[i].id == id) return &prefs.p[i];
    return 0;
}

static pref_t *pref_slot(uint32_t id)            /* this piece's, else a free one, else the least recently saved */
{
    pref_t *p = pref_find(id), *old = &prefs.p[0];
    uint32_t i;
    if (p) return p;
    for (i = 0; i < NPREF; i++) {
        if (!prefs.p[i].used) return &prefs.p[i];
        if (prefs.p[i].stamp < old->stamp) old = &prefs.p[i];
    }
    return old;
}

/* ---- state ---- */
static uint8_t play_buf[PIECE_MAX_BYTES] POOL;  /* the piece being played, copied out of flash */
static volatile uint8_t idle_ok;
static uint32_t scr, cur, top, dirty = D_ALL;
static uint32_t loaded_id = 0xFFFFFFFFu, sel, mute, solo, tempo_x10, save_pending;
static uint8_t role[PIECE_MAX_CH], snd[PIECE_MAX_CH];
static const char *status = "";
static char title[PIECE_NAME + 1];

static void app_audio(int32_t *out, uint32_t n)
{
    uint32_t i;
    for (i = 0; i + CTL <= n; i += CTL) {
        synth_apply();
        player_tick(CTL);
        synth_render(out + 2u * i, CTL);
    }
}

static int app_sysex(const uint8_t *p, uint32_t n) { return xfer_frame(p, n, fm1_ms, player_playing()); }

static void midi_in(void)
{
    uint8_t st, d1, d2;
    while (midi_get(&st, &d1, &d2)) {}           /* (MIDI in plays nothing in v1) */
}

static void app_idle(void)
{
    if (idle_ok) midi_in();
}

static void set_status(const char *s) { status = s; dirty |= D_FOOT; }

static void post(uint32_t op, uint32_t ch, uint32_t a, uint32_t b)
{
    while (synth_post(op, ch, a, b)) platform_poll();   /* (the audio interrupt drains it within 6 ms) */
}

/* open library entry i: copy it to RAM, check it, load the player, apply the saved choices */
static int open_piece(uint32_t i)
{
    const lib_entry_t *e = lib_entry(i);
    const pref_t *pf;
    uint32_t ch;
    int n;
    if (!e) return -1;
    player_stop();
    while (player_playing()) platform_poll();
    if ((n = lib_read(i, play_buf, sizeof play_buf)) < 0 || piece_check(play_buf, (uint32_t)n) || player_load(play_buf)) {
        set_status("CANNOT READ THIS PIECE");
        return -1;
    }
    loaded_id = e->id;
    pf = pref_find(e->id);
    for (ch = 0; ch < PIECE_MAX_CH; ch++) {
        uint32_t s = ch < pl_v.nch ? pl_v.ch[ch].sound : 255u;
        role[ch] = ch < pl_v.nch ? pl_v.ch[ch].role : ROLE_OTHER;
        if (pf && pf->sound[ch] < sound_count) s = pf->sound[ch];
        snd[ch] = (uint8_t)(s < sound_count ? s : sound_default(role[ch]));
        post(SC_ALL_OFF, 0, 0, 0);
        post(SC_SOUND, ch, snd[ch], 0);
        if (ch < pl_v.nch) post(SC_LEVEL, ch, pl_v.ch[ch].vol, pl_v.ch[ch].pan);
    }
    mute = pf ? pf->mute : 0u;
    solo = 0;
    tempo_x10 = pf && pf->tempo_x10 ? pf->tempo_x10 : pl_v.tempo_x10;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) player_mute(ch, (int)(mute >> ch & 1u));
    player_tempo(tempo_x10);
    for (ch = 0; ch < PIECE_NAME + 1u; ch++) title[ch] = pl_v.name[ch];
    sel = 0;
    return 0;
}

static void save_prefs(void)
{
    pref_t *p = pref_slot(loaded_id);
    uint32_t ch;
    p->id = loaded_id;
    p->used = 1;
    p->stamp = ++prefs.stamp;
    p->tempo_x10 = (uint16_t)tempo_x10;
    p->mute = (uint8_t)mute;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) p->sound[ch] = snd[ch];
    prefs.magic = PREF_MAGIC;
    set_status(blob_save(0, &prefs, sizeof prefs) ? "SAVE FAILED" : "SAVED");
}

static const uint8_t WHITE_KEY[8] = {0, 2, 4, 6, 7, 9, 11, 12};   /* F3 G3 A3 B3 C4 D4 E4 F4: fm1_in.notes bits */

static void player_input(uint32_t pressed, uint32_t keys)
{
    int32_t d;
    uint32_t k;
    if (pressed & 1u << BTN_PLAY) {
        if (player_playing()) { player_stop(); set_status("STOPPED"); }
        else { player_play(); set_status("PLAYING"); }
    }
    if (pressed & 1u << BTN_SAVE) {
        if (player_playing()) { save_pending = 1; set_status("SAVES WHEN STOPPED"); }
        else save_prefs();
    }
    if ((d = fm1_enc_take(ENC_KNOB1) + fm1_enc_take(ENC_SELECT))) {
        sel = (uint32_t)(((int32_t)sel + d) % (int32_t)pl_v.nch + (int32_t)pl_v.nch) % pl_v.nch;
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_PRESETS))) {
        snd[sel] = (uint8_t)(((int32_t)snd[sel] + d) % (int32_t)sound_count + (int32_t)sound_count) % sound_count;
        post(SC_SOUND, sel, snd[sel], 0);
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_KNOB2))) {
        int32_t t = (int32_t)tempo_x10 + d * 10;
        tempo_x10 = (uint32_t)(t < 300 ? 300 : t > 3000 ? 3000 : t);
        player_tempo(tempo_x10);
        dirty |= D_HEAD;
    }
    for (k = 0; k < 8u; k++) {
        if (!(keys >> WHITE_KEY[k] & 1u) || k >= pl_v.nch) continue;
        if (fm1_in.buttons & 1u << BTN_SEL) { solo ^= 1u << k; player_solo(k, (int)(solo >> k & 1u)); }
        else { mute ^= 1u << k; player_mute(k, (int)(mute >> k & 1u)); }
        dirty |= D_STRIPS;
    }
}

static void library_input(uint32_t pressed)
{
    int32_t d = fm1_enc_take(ENC_SELECT);
    uint32_t n = lib_count();
    if (d && n) {
        int32_t c = (int32_t)cur + d;
        cur = (uint32_t)(c < 0 ? 0 : c >= (int32_t)n ? (int32_t)n - 1 : c);
        dirty |= D_LIST;
    }
    if ((pressed & 1u << BTN_PLAY) && n && !open_piece(cur)) {
        scr = 1;
        player_play();
        status = "PLAYING";
        dirty = D_ALL;
    }
}

static void app_main(void)
{
    uint32_t last_pos = 0xFFFFFFFFu, last_beat = 0xFFFFFFFFu, was_playing = 0, held = 0;
    synth_init();
    lib_mount();
    if (blob_load(0, &prefs, sizeof prefs) || prefs.magic != PREF_MAGIC) { __builtin_memset(&prefs, 0, sizeof prefs); prefs.magic = PREF_MAGIC; }
    for (;;) {
        uint32_t rel, pressed, keys, playing;
        platform_poll();
        midi_in();
        playing = (uint32_t)player_playing();
        xfer_poll(fm1_ms, (int)playing);
        if (xf_changed) {
            xf_changed = 0;
            if (cur >= lib_count()) cur = lib_count() ? lib_count() - 1u : 0u;
            dirty |= scr ? 0u : D_LIST;
        }
        pressed = fm1_input_edges(&rel);
        keys = fm1_input_note_edges() | (fm1_in.notes & ~held);   /* (a press shorter than a pass, or one seen held) */
        held = fm1_in.notes;
        if (scr == 0) {
            fm1_enc_take(ENC_KNOB1); fm1_enc_take(ENC_PRESETS); fm1_enc_take(ENC_KNOB2);
            library_input(pressed);
        } else {
            if (pressed & 1u << BTN_HOME) {
                player_stop();
                while (player_playing()) platform_poll();
                scr = 0;
                dirty = D_ALL;
            } else {
                player_input(pressed, keys);
            }
        }
        if (was_playing && !playing) {
            if (save_pending) { save_pending = 0; save_prefs(); }
            else if (scr) set_status("STOPPED");
        }
        was_playing = playing;
        if (scr && playing) {
            uint32_t pos = player_pos(), beat = pos / PIECE_PPQ;
            if (beat != last_beat) { last_beat = beat; last_pos = pos; dirty |= D_POS; }
        } else if (scr && last_pos != player_pos()) {
            last_pos = player_pos(); last_beat = last_pos / PIECE_PPQ; dirty |= D_POS;
        }
        if (dirty) {
            idle_ok = 1;
            if (scr == 0) ui_library(dirty, cur, &top);
            else {
                ui_player_t p = {title, status, pl_v.nch, sel, mute, solo, tempo_x10, player_pos(), role, snd};
                ui_player(dirty, &p);
            }
            idle_ok = 0;
            dirty = 0;
        }
    }
}
