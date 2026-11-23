/* SPDX-License-Identifier: GPL-3.0-only */
/* COMPOSER: an FM-1 firmware that keeps a library of up to 8-channel pieces composed on a computer
 * (fm1c pushes them over USB-MIDI), plays them, lets you pick each channel's sound.
 * Built on Dinghy (zvenson) / Felucca (Leo Kuroshita, Hugelton Instruments) / SLOOP (isod89): platform.c, everything below it.
 * Sound engines: Felucca-Engines (MIT), app/engines. The chip's rules: docs/RULES.md.
 *
 * LIBRARY: SELECT turns, PLAY opens the piece, playing it.
 * PLAYER: PLAY starts / stops; KNOB1 (or SELECT) picks a channel, PRESETS its sound (a drum channel: the 35 kits),
 * KNOB2 the tempo; white keys F3..F4 (1..8) mute a channel, holding SEL a key solos it; SAVE keeps the sounds, mutes,
 * tempo for this piece; HOME goes back to the library (and stops). */
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
#include "edit.c"
#include "lib.c"
#include "xfer.c"
#include "ui.c"

#include "prefs.c"                              /* each piece's choices, in blob 0 */
_Static_assert(sizeof prefs <= SECTOR_MAX, "prefs fit a blob");
_Static_assert(sizeof fxprefs <= SECTOR_MAX, "the effects fit a blob");
_Static_assert(PREF_FX == FX_N && UI_FX == FX_N, "prefs keep and FX shows every effect");

/* ---- state ---- */
static uint8_t play_buf[PIECE_MAX_BYTES] POOL;  /* the piece being played, copied out of flash */
static volatile uint8_t idle_ok;
static uint32_t scr, cur, top, dirty = D_ALL;
static uint32_t loaded_id = 0xFFFFFFFFu, sel, mute, solo, tempo_x10, save_pending;
/* EDIT: the cursor's tick, grid (index into GRID), the keys' octave, the next new note, unsaved edits, HOME armed */
static const uint16_t GRID[4] = {12, 24, 48, 96};
static uint32_t ed_tick, ed_gi = 1, ed_len = 24, ed_vel = 100, edited, discard, preview;
static int32_t ed_octs;
static uint8_t role[PIECE_MAX_CH], snd[PIECE_MAX_CH];
static uint8_t fxv[PIECE_MAX_CH][FX_N];          /* FX: each channel's effects; the row picked */
static uint32_t fxrow;
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

static void set_tempo(uint32_t x10)               /* the player's and the delay's */
{
    player_tempo(x10);
    post(SC_TEMPO, 0, x10 & 255u, x10 >> 8);
}

/* PRESETS turned d clicks on channel ch: the next sound; a drum channel (DRUMS, PERC) turns through the kits only */
static uint32_t sound_turn(uint32_t ch, int32_t d)
{
    int32_t s = snd[ch], n = (int32_t)sound_count, step;
    uint32_t kit = role[ch] == ROLE_DRUMS || role[ch] == ROLE_PERC;   /* drums turn through kits, the rest skip them */
    for (; d; d -= step) {
        step = d > 0 ? 1 : -1;
        do s = (s + step + n) % n; while ((uint32_t)!!sounds[s].e->oneshot != kit);
    }
    return (uint32_t)s;
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
    edit_forget();
    edited = discard = 0;
    ed_tick = 0;
    pf = pref_find(e->id);
    post(SC_ALL_OFF, 0, 0, 0);
    for (ch = 0; ch < PIECE_MAX_CH; ch++) {      /* (piece_view: unused channels are OTHER, default sound, volume 0) */
        uint32_t s = pf ? pf->sound[ch] : pl_v.ch[ch].sound;
        role[ch] = pl_v.ch[ch].role;
        snd[ch] = (uint8_t)(s < sound_count ? s : sound_default(role[ch]));
        post(SC_SOUND, ch, snd[ch], role[ch] + 1u);
        post(SC_LEVEL, ch, pl_v.ch[ch].vol, pl_v.ch[ch].pan);
    }
    {
        const fxpref_t *fp = fxpref_find(e->id);
        uint32_t k;
        for (ch = 0; ch < PIECE_MAX_CH; ch++) {
            if (fp) for (k = 0; k < FX_N; k++) fxv[ch][k] = (uint8_t)fx_clamp(k, fp->v[ch][k]);
            else fx_defaults(role[ch], fxv[ch]);
            while (synth_fx_post(ch, fxv[ch])) platform_poll();
        }
    }
    mute = pf ? pf->mute : 0u;
    solo = 0;
    tempo_x10 = pf && pf->tempo_x10 ? pf->tempo_x10 : pl_v.tempo_x10;
    for (ch = 0; ch < PIECE_MAX_CH; ch++) player_mute(ch, (int)(mute >> ch & 1u));
    set_tempo(tempo_x10);
    for (ch = 0; ch < PIECE_NAME + 1u; ch++) title[ch] = pl_v.name[ch];
    sel = 0;
    return 0;
}

static void save_prefs(void)
{
    pref_put(loaded_id, tempo_x10, snd, mute);
    fxpref_put(loaded_id, (const uint8_t (*)[FX_N])fxv);
    set_status(blob_save(0, &prefs, sizeof prefs) || blob_save(1, &fxprefs, sizeof fxprefs) ? "SAVE FAILED" : "SAVED");
}

/* SAVE: an edited piece first (a new entry, then the old one deleted: a power cut between keeps both), then the
 * choices, under the piece's new id */
static void save_all(void)
{
    if (edited) {
        int32_t id = lib_put(title, play_buf, pc_u32(play_buf + 40));
        if (id < 0) { set_status(id == LIB_FULL ? "LIBRARY FULL" : "SAVE FAILED"); return; }
        lib_del(loaded_id);
        loaded_id = (uint32_t)id;
        edited = 0;
        cur = 0;                                /* (newest first: it is the top entry now) */
        dirty |= D_HEAD;
    }
    save_prefs();
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
        else save_all();
    }
    if (pressed & 1u << BTN_EDIT) {
        uint32_t g = GRID[ed_gi];
        if (player_playing()) ed_tick = player_pos() - player_pos() % (16u * g);
        ed_tick -= ed_tick % g;
        scr = 2;
        dirty = D_ALL;
        set_status(player_playing() ? "STOP TO EDIT" : "");
        return;
    }
    if (pressed & 1u << BTN_FX) {               /* the selected channel's effects */
        scr = 3;
        fxrow = 0;
        dirty = D_ALL;
        return;
    }
    fm1_enc_take(ENC_KNOB3);                    /* (EDIT's: not kept for it) */
    if ((d = fm1_enc_take(ENC_KNOB1) + fm1_enc_take(ENC_SELECT))) {
        sel = (uint32_t)(((int32_t)sel + d) % (int32_t)pl_v.nch + (int32_t)pl_v.nch) % pl_v.nch;
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_PRESETS))) {
        snd[sel] = (uint8_t)sound_turn(sel, d);
        post(SC_SOUND, sel, snd[sel], role[sel] + 1u);
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_KNOB2))) {
        int32_t t = (int32_t)tempo_x10 + d * 10;
        tempo_x10 = (uint32_t)(t < 300 ? 300 : t > 3000 ? 3000 : t);
        set_tempo(tempo_x10);
        dirty |= D_HEAD;
    }
    for (k = 0; k < 8u; k++) {
        if (!(keys >> WHITE_KEY[k] & 1u) || k >= pl_v.nch) continue;
        if (fm1_in.buttons & 1u << BTN_SEL) { solo ^= 1u << k; player_solo(k, (int)(solo >> k & 1u)); }
        else { mute ^= 1u << k; player_mute(k, (int)(mute >> k & 1u)); }
        dirty |= D_STRIPS;
    }
}

/* FX: SELECT picks the row, KNOB1 sets it (heard at once), KNOB2 the channel; PLAY and SAVE as in PLAYER */
static void fx_input(uint32_t pressed)
{
    int32_t d;
    if (pressed & 1u << BTN_PLAY) {
        if (player_playing()) { player_stop(); set_status("STOPPED"); }
        else { player_play(); set_status("PLAYING"); }
    }
    if (pressed & 1u << BTN_SAVE) {
        if (player_playing()) { save_pending = 1; set_status("SAVES WHEN STOPPED"); }
        else save_all();
    }
    fm1_enc_take(ENC_PRESETS);
    fm1_enc_take(ENC_KNOB3);
    if ((d = fm1_enc_take(ENC_SELECT))) {
        fxrow = (uint32_t)(((int32_t)fxrow + d) % (int32_t)FX_N + (int32_t)FX_N) % FX_N;
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_KNOB2))) {
        sel = (uint32_t)(((int32_t)sel + d) % (int32_t)pl_v.nch + (int32_t)pl_v.nch) % pl_v.nch;
        dirty = D_ALL;
    }
    if ((d = fm1_enc_take(ENC_KNOB1))) {
        int32_t v = (int32_t)fxv[sel][fxrow] + d;
        fxv[sel][fxrow] = (uint8_t)fx_clamp(fxrow, (uint32_t)(v < 0 ? 0 : v));
        while (synth_fx_post(sel, fxv[sel])) platform_poll();
        dirty |= D_STRIPS;
    }
}

/* the notes of the selected channel starting in the cursor's step: up to 16 */
static uint32_t cell_notes(piece_note_t *out)
{
    piece_cur_t c;
    piece_note_t n;
    uint32_t k = 0, g = GRID[ed_gi];
    piece_cursor(&pl_v.ch[sel], &c);
    while (piece_next(&c, &n) && n.tick < ed_tick + g)
        if (n.tick >= ed_tick && k < 16u) out[k++] = n;
    return k;
}

static uint32_t ed_low(void) { return (uint32_t)(53 + 12 * ed_octs); }   /* F3 key's pitch */

static void edited_now(int r)
{
    if (r == ED_OK) { edited = 1; discard = 0; player_refresh(); dirty |= D_ROLL | D_HEAD; set_status(""); }
    else set_status(r == ED_FULL ? "PIECE FULL" : "CANNOT EDIT THERE");
}

static void edit_input(uint32_t pressed, uint32_t keys)
{
    piece_note_t cn[16];
    uint32_t k, n, g = GRID[ed_gi], playing = (uint32_t)player_playing();
    int32_t d;
    if (pressed & 1u << BTN_PLAY) {
        if (playing) { player_stop(); set_status("STOPPED"); }
        else { player_play_from(ed_tick - ed_tick % (4u * PIECE_PPQ)); set_status("PLAYING FROM THIS BAR"); }
    }
    if (pressed & 1u << BTN_SAVE) {
        if (playing) { save_pending = 1; set_status("SAVES WHEN STOPPED"); }
        else save_all();
    }
    if ((d = fm1_enc_take(ENC_SELECT))) {       /* the cursor, a step a click, inside the piece */
        int32_t t = (int32_t)ed_tick + d * (int32_t)g, last = (int32_t)((pl_v.len_ticks - 1u) / g * g);
        ed_tick = (uint32_t)(t < 0 ? 0 : t > last ? last : t);
        dirty |= D_ROLL | D_HEAD;
    }
    if ((d = fm1_enc_take(ENC_KNOB1))) {         /* the grid; the cursor snaps to it */
        int32_t i = (int32_t)ed_gi + d;
        ed_gi = (uint32_t)(i < 0 ? 0 : i > 3 ? 3 : i);
        ed_tick -= ed_tick % GRID[ed_gi];
        dirty |= D_ROLL | D_HEAD;
    }
    if (pressed & (1u << BTN_OCT_DOWN | 1u << BTN_OCT_UP)) {
        ed_octs += (pressed & 1u << BTN_OCT_UP ? 1 : 0) - (pressed & 1u << BTN_OCT_DOWN ? 1 : 0);
        ed_octs = ed_octs < -3 ? -3 : ed_octs > 3 ? 3 : ed_octs;
        dirty |= D_ROLL;
    }
    if ((d = fm1_enc_take(ENC_PRESETS))) {
        snd[sel] = (uint8_t)sound_turn(sel, d);
        post(SC_SOUND, sel, snd[sel], role[sel] + 1u);
    }
    for (k = 0; k < ROLL_KEYS; k++)             /* a preview ends with its key */
        if ((preview >> k & 1u) && !(fm1_in.notes >> k & 1u)) { post(SC_NOTE_OFF, sel, ed_low() + k, 0); preview &= ~(1u << k); }
    {
        int32_t dl = fm1_enc_take(ENC_KNOB2), dv = fm1_enc_take(ENC_KNOB3);
        if ((dl || dv || keys) && playing) { set_status("STOP TO EDIT"); return; }
        n = cell_notes(cn);
        if (dl || dv) {                         /* length / velocity: of the notes in the step, also of new ones */
            int32_t len = (int32_t)(n ? cn[0].dur : ed_len) + dl * (int32_t)g, vel = (int32_t)(n ? cn[0].vel : ed_vel) + dv * 4;
            uint32_t i;
            int r = ED_OK;
            ed_len = (uint32_t)(len < 1 ? 1 : len > 65535 ? 65535 : len);
            ed_vel = (uint32_t)(vel < 1 ? 1 : vel > 127 ? 127 : vel);
            edit_mark();                        /* one turn: one undo */
            for (i = 0; i < n && r == ED_OK; i++) r = edit_set(play_buf, sel, cn[i].tick, cn[i].pitch, ed_len, ed_vel);
            if (n) edited_now(r);
            dirty |= D_FOOT;
        }
        for (k = 0; k < ROLL_KEYS; k++) {       /* a key: its note in the step goes, else a new one starts here */
            uint32_t p = ed_low() + k, i;
            int r;
            if (!(keys >> k & 1u)) continue;
            for (i = 0; i < n && cn[i].pitch != p; i++) {}
            edit_mark();
            if (i < n) r = edit_delete(play_buf, sel, cn[i].tick, p);
            else {
                r = edit_insert(play_buf, sel, ed_tick, p, ed_len, ed_vel);
                if (r == ED_OK) { post(SC_NOTE_ON, sel, p, ed_vel); preview |= 1u << k; }
            }
            edited_now(r);
            n = cell_notes(cn);
        }
    }
    if (pressed & 1u << BTN_FX) {               /* undo */
        uint32_t ch, t;
        if (playing) set_status("STOP TO EDIT");
        else if (edit_undo(play_buf, &ch, &t) == ED_OK) {
            sel = ch;
            ed_tick = t - t % g;
            edited = 1;
            player_refresh();
            dirty |= D_ROLL | D_HEAD;
            set_status("UNDONE");
        } else set_status("NOTHING TO UNDO");
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
    if (blob_load(0, &prefs, sizeof prefs) || prefs.magic != PREF_MAGIC) prefs_reset();
    if (blob_load(1, &fxprefs, sizeof fxprefs) || fxprefs.magic != FXP_MAGIC) fxprefs_reset();
    for (;;) {
        uint32_t rel, pressed, keys, playing;
        platform_poll();
        midi_in();
        playing = (uint32_t)player_playing();
#ifdef UX_MAX
        {                                       /* a frame from the MIDI jack: the same transfer as USB */
            const uint8_t *sp;
            uint32_t sn;
            if (uart_sysex_get(&sp, &sn)) { xfer_frame(sp, sn, fm1_ms, (int)playing); uart_sysex_done(); }
        }
#endif
        xfer_poll(fm1_ms, (int)playing);
        if (xf_changed) {
            xf_changed = 0;
            if (cur >= lib_count()) cur = lib_count() ? lib_count() - 1u : 0u;
            dirty |= scr ? 0u : D_LIST;
        }
        pressed = fm1_input_edges(&rel);
        keys = fm1_input_note_edges() | (fm1_in.notes & ~held);   /* (also a press shorter than a pass) */
        held = fm1_in.notes;
        if (scr == 0) {
            fm1_enc_take(ENC_KNOB1); fm1_enc_take(ENC_PRESETS); fm1_enc_take(ENC_KNOB2); fm1_enc_take(ENC_KNOB3);
            library_input(pressed);
        } else {
            if (scr == 2) {
                if (pressed & 1u << BTN_HOME) { scr = 1; dirty = D_ALL; }
                else edit_input(pressed, keys);
            } else if (scr == 3) {
                if (pressed & (1u << BTN_HOME | 1u << BTN_FX)) { scr = 1; dirty = D_ALL; }
                else fx_input(pressed);
            } else if (pressed & 1u << BTN_HOME) {
                if (edited && !discard) { discard = 1; set_status("UNSAVED EDITS: HOME AGAIN TO DISCARD"); }
                else {
                    player_stop();
                    while (player_playing()) platform_poll();
                    scr = 0;
                    dirty = D_ALL;
                }
            } else {
                player_input(pressed, keys);
            }
        }
        if (was_playing && !playing) {
            if (save_pending) { save_pending = 0; save_all(); }
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
            else if (scr == 2) {
                piece_note_t cn[16];
                uint32_t n = cell_notes(cn);
                ui_edit_t e = {status, play_buf, sel, role[sel], ed_tick, GRID[ed_gi], ed_gi, ed_low(),
                               n ? cn[0].dur : ed_len, n ? cn[0].vel : ed_vel, n, n ? cn[0].pitch : 0, edited};
                ui_edit(dirty, &e);
            } else if (scr == 3) {
                ui_fx_t f = {status, sel, role[sel], snd[sel], fxrow, fxv[sel]};
                ui_fx(dirty, &f);
            } else {
                ui_player_t p = {title, status, pl_v.nch, sel, mute, solo, tempo_x10, player_pos(), role, snd};
                ui_player(dirty, &p);
            }
            idle_ok = 0;
            dirty = 0;
        }
    }
}
