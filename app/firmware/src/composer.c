/* SPDX-License-Identifier: GPL-3.0-only */
/* COMPOSER: an FM-1 firmware that keeps a library of up to 8-channel pieces composed on a computer
 * (fm1c pushes them over USB-MIDI), plays them, lets you pick each channel's sound.
 * Built on Dinghy (zvenson) / Felucca (Leo Kuroshita, Hugelton Instruments) / SLOOP (isod89): platform.c, everything below it.
 * Sound engines: Felucca-Engines (MIT), app/engines. The chip's rules: docs/RULES.md.
 *
 * LIBRARY: SELECT turns, PLAY opens the piece, playing it.
 * PLAYER: PLAY starts / stops; KNOB1 (or SELECT) picks a channel, PRESETS its sound (a drum channel: the 35 kits),
 * KNOB2 the tempo; white keys F3..F4 (1..8) mute a channel, holding SEL a key solos it; SAVE keeps the sounds, mutes,
 * tempo for this piece; HOME goes back to the library (and stops). A piece with sections: the keys above F4 (F#4..G5)
 * launch section 1..14 (OCT+ 15..24), at the next bar line; SEQ loops the playing section.
 * KEYS (US-24, live.c): ARP turns it on / off: the 27 keys then play the selected channel's sound in the piece's key
 * (OCT- / OCT+ move them, ENV cycles the auto-chords, KNOB3 the scale, KNOB4 the root). MIDI IN (USB or the jack)
 * plays on any screen but LIBRARY, as sent.
 * STEPS (US-25, steps.c): LFO opens it from PLAYER: the 8 channels x 16 steps from the playing section's start (4
 * pages, 64 steps); SELECT moves, KNOB1 the channel, a note key adds / removes that note in the step, KNOB2 / KNOB3
 * length (steps) / velocity, KNOB4 the channel's loop length, ALGORITHM its swing; edits also while playing.
 * REC (US-26, live.c): from PLAYER, a bar of clicks, then what is played live on the selected channel goes into it
 * (GLO: OVERDUB / REPLACE, ALGORITHM: the quantize grid); REC again, PLAY or leaving the PLAYER ends the pass. */
#define APP_NAME "COMPOSER"
static void app_idle(void);
#include <stdint.h>
static int app_sysex(const uint8_t *p, uint32_t n);
static volatile uint8_t midi_in_lost;           /* (SLOOP 2.8.3) a MIDI note message did not fit the ring (midi_uart.c) */
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
#include "steps.c"
#include "lib.c"
#include "xfer.c"
#include "ui.c"
static void post(uint32_t op, uint32_t ch, uint32_t a, uint32_t b);
#define LK_ON(ch, note, vel) post(SC_LIVE_ON, ch, note, vel)
#define LK_OFF(ch, note) post(SC_LIVE_OFF, ch, note, 0)
#include "live.c"                               /* the live keyboard, MIDI IN */

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
static uint32_t sloop, spage;                   /* PLAYER: section loop on; the sections the keys launch (14 a page) */
static char snext[24];                          /* "NEXT VERSE1" */
static uint32_t st_step, st_base, st_len = 1, st_vel = 100, st_play;   /* STEPS: cursor, start tick, next note; playhead */
static uint32_t rc_seen, rc_shown;              /* REC: the recorder's edits taken in; its state on screen */
#define SECT_KEY0 13u                           /* F#4: the first key above the mute keys */
#define SECT_KEYS 14u
static const char *status = "";
static char title[PIECE_NAME + 1];

static void app_audio(int32_t *out, uint32_t n)
{
    uint32_t i;
    for (i = 0; i + CTL <= n; i += CTL) {
        synth_apply();
        player_tick(CTL);
        rc_audio(out + 2u * i, CTL);            /* (the count-in's click) */
        synth_render(out + 2u * i, CTL);
    }
}

static int app_sysex(const uint8_t *p, uint32_t n) { return xfer_frame(p, n, fm1_ms, player_playing()); }

/* MIDI IN: notes on the piece's channels (live.c), as long as the synth's queue has room for what one may post (so a
 * call from a redraw never waits); a lost note message (the main loop only): every MIDI note ends */
static void midi_in(int main_loop)
{
    uint8_t st, d1, d2;
    while (sq_w - sq_r <= 32u - 2u && midi_get(&st, &d1, &d2))
        if (scr) lk_midi(st, d1, d2, sel, pl_v.nch);
    if (main_loop && midi_in_lost) {
        midi_in_lost = 0;
        lk_midi_off();
    }
}

static void app_idle(void)
{
    if (idle_ok) midi_in(0);
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

static void sound_next(int32_t d)               /* PRESETS: the selected channel's next sound, heard at once */
{
    snd[sel] = (uint8_t)sound_turn(sel, d);
    post(SC_SOUND, sel, snd[sel], 0);           /* (0: the role kept) */
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
    lk_keys_off();
    lk_midi_off();
    lk_open(&pl_v);
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
    sloop = spage = 0;
    player_section_loop(0);
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

static void keys_status(void)                   /* "KEYS D DOR TRIAD OCT+1" / "KEYS OFF" */
{
    static char b[40];
    if (lk.on) lk_label(b, sizeof b);
    else ui_fmt(b, sizeof b, "KEYS OFF");
    set_status(b);
}

static void seq_toggle(void)                    /* SEQ: the section loop */
{
    if (!pl_v.nsect) set_status("NO SECTIONS");
    else { sloop ^= 1u; player_section_loop((int)sloop); set_status(sloop ? "SECTION LOOP ON" : "SECTION LOOP OFF"); }
    dirty |= D_SECT;
}

static void play_save(uint32_t pressed)         /* PLAY, SAVE: as in PLAYER */
{
    if (pressed & 1u << BTN_PLAY) {
        if (player_playing()) { player_stop(); set_status("STOPPED"); }
        else { player_play(); set_status("PLAYING"); }
    }
    if (pressed & 1u << BTN_SAVE) {
        if (player_playing()) { save_pending = 1; set_status("SAVES WHEN STOPPED"); }
        else save_all();
    }
}

/* REC: what the recorder did shows (the piece edited, the footer, the REC mark) */
static void rc_sync(void)
{
    if (rc.ops != rc_seen) { rc_seen = rc.ops; edited = 1; discard = 0; }
    if (rc.msg) { set_status(rc.msg); rc.msg = 0; }
    if (rc.mode != rc_shown) { rc_shown = rc.mode; dirty |= D_POS; }
}

static const uint8_t WHITE_KEY[8] = {0, 2, 4, 6, 7, 9, 11, 12};   /* F3 G3 A3 B3 C4 D4 E4 F4: fm1_in.notes bits */

static void player_input(uint32_t pressed, uint32_t keys)
{
    int32_t d;
    uint32_t k;
    if (rc.mode && (pressed & (1u << BTN_PLAY | 1u << BTN_REC))) {   /* the pass (or its count-in) ends */
        rc_end(play_buf, ED_OK);
        pressed &= ~(1u << BTN_REC);
    }
    if (pressed & 1u << BTN_REC) rc_arm(sel);
    if (pressed & 1u << BTN_GLO) { rc.replace ^= 1u; rc_label(); }
    if ((d = fm1_enc_take(ENC_ALGORITHM))) { rc.quant = (uint32_t)clamp((int32_t)rc.quant + d, 0, 4); rc_label(); }
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
        lk_keys_off();
        set_status(player_playing() ? "STOP TO EDIT" : "");
        return;
    }
    if (pressed & 1u << BTN_LFO) {              /* STEPS, from the playing section's start */
        uint32_t s = player_section();
        st_base = s < pl_v.nsect ? piece_sect_start(&pl_v, s) : 0u;
        st_step = 0;
        scr = 4;
        dirty = D_ALL;
        lk_keys_off();
        fm1_enc_take(ENC_ALGORITHM);
        fm1_enc_take(ENC_KNOB4);
        set_status("");
        return;
    }
    if (pressed & 1u << BTN_FX) {               /* the selected channel's effects */
        scr = 3;
        fxrow = 0;
        dirty = D_ALL;
        lk_keys_off();
        return;
    }
    if (pressed & 1u << BTN_ARP) {              /* KEYS */
        lk.on ^= 1u;
        if (!lk.on) lk_keys_off();
        keys_status();
    }
    if (lk.on) {
        int32_t ds = fm1_enc_take(ENC_KNOB3), dr = fm1_enc_take(ENC_KNOB4);
        uint32_t ch0 = (uint32_t)lk.t.p[P_CHORD], oct0 = (uint32_t)lk.oct, sc0 = lk_scale(), r0 = (uint32_t)lk.t.p[P_ROOT];
        if (ds) lk_key((uint32_t)lk.t.p[P_ROOT], (uint32_t)(((int32_t)lk_scale() + ds) % 17 + 17) % 17u);
        if (dr) lk_key((uint32_t)(((int32_t)lk.t.p[P_ROOT] + dr) % 12 + 12), lk_scale());
        if (pressed & 1u << BTN_ENV) lk.t.p[P_CHORD] = (int16_t)((lk.t.p[P_CHORD] + 1) % 6);
        if (pressed & 1u << BTN_OCT_UP && lk.oct < 3) lk.oct++;
        if (pressed & 1u << BTN_OCT_DOWN && lk.oct > -3) lk.oct--;
        if (ch0 != (uint32_t)lk.t.p[P_CHORD] || oct0 != (uint32_t)lk.oct || sc0 != lk_scale() || r0 != (uint32_t)lk.t.p[P_ROOT])
            keys_status();
    } else {
        fm1_enc_take(ENC_KNOB3);                /* (EDIT's: not kept for it) */
        fm1_enc_take(ENC_KNOB4);
    }
    if ((d = fm1_enc_take(ENC_KNOB1) + fm1_enc_take(ENC_SELECT))) {
        sel = (uint32_t)(((int32_t)sel + d) % (int32_t)pl_v.nch + (int32_t)pl_v.nch) % pl_v.nch;
        dirty |= D_STRIPS;
    }
    if ((d = fm1_enc_take(ENC_PRESETS))) { sound_next(d); dirty |= D_STRIPS; }
    if ((d = fm1_enc_take(ENC_KNOB2))) {
        int32_t t = (int32_t)tempo_x10 + d * 10;
        tempo_x10 = (uint32_t)(t < 300 ? 300 : t > 3000 ? 3000 : t);
        set_tempo(tempo_x10);
        dirty |= D_HEAD;
    }
    if (pressed & 1u << BTN_SEQ) seq_toggle();
    if (lk.on) {                                /* KEYS: the keys play, nothing else */
        lk_keys(fm1_in.notes, keys, sel);
        return;
    }
    if ((pressed & (1u << BTN_OCT_DOWN | 1u << BTN_OCT_UP)) && pl_v.nsect > SECT_KEYS) {
        spage = pressed & 1u << BTN_OCT_UP ? 1u : 0u;
        set_status(spage ? "KEYS: SECTIONS 15-24" : "KEYS: SECTIONS 1-14");
    }
    for (k = 0; k < SECT_KEYS; k++) {           /* launch: at the next bar line; stopped, it plays from its start */
        uint32_t s = spage * SECT_KEYS + k;
        char n[PIECE_SNAME + 1];
        if (!(keys >> (SECT_KEY0 + k) & 1u) || s >= pl_v.nsect) continue;
        player_launch(s);
        piece_sect_name(&pl_v, s, n);
        ui_fmt(snext, sizeof snext, "%s %s", player_playing() ? "NEXT" : "PLAYING", n);
        if (!player_playing()) player_play();
        set_status(snext);
        dirty |= D_SECT;
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
    play_save(pressed);
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

static void previews_end(void)                  /* EDIT, STEPS: a preview ends with its key */
{
    uint32_t k;
    for (k = 0; preview >> k; k++)
        if ((preview >> k & 1u) && !(fm1_in.notes >> k & 1u)) { post(SC_NOTE_OFF, sel, ed_low() + k, 0); preview &= ~(1u << k); }
}

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
    if ((d = fm1_enc_take(ENC_PRESETS))) sound_next(d);
    previews_end();
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

/* STEPS: a change went through edit.c (r), the player told */
static void steps_now(int r)
{
    static const char *const WHY[6] = {"", "PAST THE END", "PIECE FULL", "", "EMPTY STEP", "4 NOTES A STEP"};
    if (r == ED_OK) { edited = 1; discard = 0; dirty |= D_ROLL | D_HEAD | D_FOOT; set_status(""); }
    else set_status(WHY[r]);
}

static int32_t clampi(int32_t v, int32_t lo, int32_t hi) { return v < lo ? lo : v > hi ? hi : v; }

/* STEPS: every change also while playing (player_hold / player_edited around it); one gesture, one undo */
static void steps_input(uint32_t pressed, uint32_t keys)
{
    piece_note_t cn[ST_CHORD];
    const uint8_t *rec;
    uint32_t k, n, t0;
    int32_t d, dl, dv;
    int r;
    play_save(pressed);
    if (pressed & 1u << BTN_SEQ) seq_toggle();
    if ((d = fm1_enc_take(ENC_SELECT))) {
        st_step = (uint32_t)clampi((int32_t)st_step + d, 0, (int32_t)PIECE_MAX_STEPS - 1);
        dirty |= D_ROLL | D_HEAD | D_FOOT;
    }
    if ((d = fm1_enc_take(ENC_KNOB1))) {
        sel = (uint32_t)(((int32_t)sel + d) % (int32_t)pl_v.nch + (int32_t)pl_v.nch) % pl_v.nch;
        dirty |= D_ROLL | D_HEAD | D_FOOT;
    }
    if ((d = fm1_enc_take(ENC_PRESETS))) sound_next(d);
    if (pressed & (1u << BTN_OCT_DOWN | 1u << BTN_OCT_UP)) {
        ed_octs += (pressed & 1u << BTN_OCT_UP ? 1 : 0) - (pressed & 1u << BTN_OCT_DOWN ? 1 : 0);
        ed_octs = clampi(ed_octs, -3, 3);
    }
    previews_end();
    t0 = st_base + st_step * ST_TICKS;
    n = steps_cell(play_buf, sel, t0, cn);
    dl = fm1_enc_take(ENC_KNOB2);
    dv = fm1_enc_take(ENC_KNOB3);
    if (dl || dv) {                             /* length / velocity: of the step's notes, also of new ones */
        st_len = (uint32_t)clampi((int32_t)(n ? (cn[0].dur + ST_TICKS - 1u) / ST_TICKS : st_len) + dl, 1, (int32_t)PIECE_MAX_STEPS);
        st_vel = (uint32_t)clampi((int32_t)(n ? cn[0].vel : st_vel) + dv * 4, 1, 127);
        if (n) { edit_mark(); player_hold(); r = steps_set(play_buf, sel, t0, st_len, st_vel); player_edited(); steps_now(r); }
        dirty |= D_FOOT;
    }
    rec = play_buf + PIECE_HDR + sel * PIECE_CHREC;
    dl = fm1_enc_take(ENC_KNOB4);
    dv = fm1_enc_take(ENC_ALGORITHM);
    if (dl || dv) {                             /* the channel's loop length (0: none), its swing */
        edit_mark();
        player_hold();
        r = dl ? edit_chan(play_buf, sel, PIECE_CH_STEPS, (uint32_t)clampi(rec[PIECE_CH_STEPS] + dl, 0, (int32_t)PIECE_MAX_STEPS)) : ED_OK;
        if (dv && r == ED_OK) r = edit_chan(play_buf, sel, PIECE_CH_SWING, (uint32_t)clampi(rec[PIECE_CH_SWING] + dv, 0, (int32_t)PIECE_MAX_SWING));
        player_edited();
        steps_now(r);
    }
    for (k = 0; k < ROLL_KEYS; k++) {           /* a key: its note in the step goes, else it starts there */
        uint32_t p = ed_low() + k, was = n;
        if (!(keys >> k & 1u)) continue;
        edit_mark();
        player_hold();
        r = steps_key(play_buf, sel, t0, p, st_len, st_vel);
        player_edited();
        n = steps_cell(play_buf, sel, t0, cn);
        if (n > was) { post(SC_NOTE_ON, sel, p, st_vel); preview |= 1u << k; }
        steps_now(r);
    }
    if (pressed & 1u << BTN_FX) {               /* undo */
        uint32_t ch, t;
        player_hold();
        r = edit_undo(play_buf, &ch, &t);
        player_edited();
        if (r == ED_OK) { sel = ch; edited = 1; dirty |= D_ROLL | D_HEAD | D_FOOT; set_status("UNDONE"); }
        else set_status("NOTHING TO UNDO");
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
    uint32_t last_pos = 0xFFFFFFFFu, last_beat = 0xFFFFFFFFu, was_playing = 0, held = 0, last_sect = 0;
    synth_init();
    lib_mount();
    if (blob_load(0, &prefs, sizeof prefs) || prefs.magic != PREF_MAGIC) prefs_reset();
    if (blob_load(1, &fxprefs, sizeof fxprefs) || fxprefs.magic != FXP_MAGIC) fxprefs_reset();
    for (;;) {
        uint32_t rel, pressed, keys, playing;
        platform_poll();
        midi_in(1);
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
        if (scr == 1u && rc.mode && (pressed & (1u << BTN_HOME | 1u << BTN_EDIT | 1u << BTN_LFO | 1u << BTN_FX)))
            rc_end(play_buf, ED_OK);            /* leaving the PLAYER ends a pass, before HOME asks about edits */
        rc_sync();
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
            } else if (scr == 4) {
                if (pressed & (1u << BTN_HOME | 1u << BTN_LFO)) { scr = 1; dirty = D_ALL; }
                else steps_input(pressed, keys);
            } else if (pressed & 1u << BTN_HOME) {
                lk_keys_off();                  /* (only the PLAYER plays the keys) */
                if (edited && !discard) { discard = 1; set_status("UNSAVED EDITS: HOME AGAIN TO DISCARD"); }
                else {
                    player_stop();
                    while (player_playing()) platform_poll();
                    lk_midi_off();
                    scr = 0;
                    dirty = D_ALL;
                }
            } else {
                player_input(pressed, keys);
            }
        }
        rc_poll(play_buf);
        rc_sync();
        if (was_playing && !playing) {
            if (save_pending) { save_pending = 0; save_all(); }
            else if (scr) set_status("STOPPED");
        }
        was_playing = playing;
        if (scr == 1u && (player_section() | player_queued() << 8) != last_sect) {   /* a section began, or was queued */
            last_sect = player_section() | player_queued() << 8;
            dirty |= D_SECT;
        }
        if (scr == 4u) {                        /* STEPS: the selected channel's playing step */
            uint32_t ps = playing ? player_local(sel) / ST_TICKS + 1u : 0u;
            if (ps != st_play) { st_play = ps; dirty |= D_POS; }
        }
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
            } else if (scr == 4) {
                piece_note_t cn[ST_CHORD];
                uint32_t at[PIECE_MAX_CH], ch, n = steps_cell(play_buf, sel, st_base + st_step * ST_TICKS, cn);
                for (ch = 0; ch < PIECE_MAX_CH; ch++) at[ch] = playing ? player_local(ch) : 0xFFFFFFFFu;
                ui_steps_t u = {status, play_buf, role, at, cn, sel, st_base, st_step, n,
                                n ? (cn[0].dur + ST_TICKS - 1u) / ST_TICKS : st_len, n ? cn[0].vel : st_vel, edited};
                ui_steps(dirty, &u);
            } else if (scr == 3) {
                ui_fx_t f = {status, sel, role[sel], snd[sel], fxrow, fxv[sel]};
                ui_fx(dirty, &f);
            } else {
                ui_player_t p = {title, status, pl_v.nch, sel, mute, solo, tempo_x10, player_pos(), role, snd,
                                 &pl_v, player_section(), player_queued(), sloop, rc.mode};
                ui_player(dirty, &p);
            }
            idle_ok = 0;
            dirty = 0;
        }
    }
}
