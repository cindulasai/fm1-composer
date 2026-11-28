# Files copied from SLOOP

SLOOP 2.8.5, [isod89/sloop-fm1](https://github.com/isod89/sloop-fm1) at commit `bac9c05ea7627449405aa10e48da21d75b6c8c56`,
GPL-3.0-only unless a row says otherwise (SPEC.md decision 29). SLOOP's logo and Hügelton's artwork are not copied.
One row per file: COMPOSER's path, SLOOP's, the commit, the upstream file's git blob id (`git hash-object`; `tools/tests/test_fm6.py`
checks them), its licence, and whether COMPOSER changed it (each change is marked `COMPOSER:` in the file). COMPOSER's
own glue: `firmware/src/sloop_glue.c` (the engine pool, what SLOOP's `core.h` gives eng_fm6.c, eng_phys.c,
eng_digital.c and eng_formant.c, `ENG_FM6`, `ENG_PHYS`, `ENG_DIGITAL`, `ENG_FORMANT`, the presets in COMPOSER's layout),
`tools/gen_fm6_tables.py`, `tools/gen_sloop_tables.py`, the end of `firmware/src/drums.c`, and synth.c.

| COMPOSER path | upstream path | commit | git blob | licence | changed |
| --- | --- | --- | --- | --- | --- |
| app/engines/LICENSE-Apache-2.0-msfa.txt | LICENSES/Apache-2.0-msfa.txt | bac9c05 | 75b5bab21f7b4e150b4be5ca224663af08101483 | Apache-2.0 (text) | no |
| app/engines/eng_digital.c | firmware/src/eng_digital.c | bac9c05 | 8f3c0f83331b91119064c9347d50131e44cb5f5b | GPL-3.0-only | no |
| app/engines/eng_fm6.c | firmware/src/eng_fm6.c | bac9c05 | c4fb384a4c70d9148c0e47edc319d7a4bfebb83e | GPL-3.0-only | no |
| app/engines/eng_formant.c | firmware/src/eng_formant.c | bac9c05 | 75288f9a6dd81b91e74c6486c7a5b4d4af931203 | GPL-3.0-only | no |
| app/engines/eng_phys.c | firmware/src/eng_phys.c | bac9c05 | ecabff6f072b3e364ad31617f6f3c1d9924780d6 | GPL-3.0-only | no |
| app/engines/fm6_bank.c | firmware/src/fm6_bank.c | bac9c05 | 7b7df320d2bf9b0aca65099f97996ed898055ff5 | GPL-3.0-only | no (not built: SLOOP's flash bank) |
| app/engines/fm6_core.c | firmware/src/fm6_core.c | bac9c05 | e54f49518a1cd0405a6e496b1bb6df97e55230ea | Apache-2.0 | no |
| app/engines/phys_dsp.c | firmware/src/phys_dsp.c | bac9c05 | 008434fe347ae22d34617b3442a9bf7384fb7121 | MIT | no |
| app/engines/phys_symp.c | firmware/src/phys_symp.c | bac9c05 | 780abf4ba6a8e941087b05c68244851491f28db8 | MIT | no |
| app/firmware/src/drum_synth.c | firmware/src/drum_synth.c | bac9c05 | 303d4e0cf47499a1da66c704eb06e604ab235c7b | GPL-3.0-only | yes: 2 lines, a left shift of a negative value made defined (`shl`, the same bits) |
| app/tools/drumkit_levels.json | tools/drumkit_levels.json | bac9c05 | 6a12651dc9c1f240f87a1309ab37391729769ae5 | GPL-3.0-only (SLOOP `tools/`) | no |
| app/tools/gen_drumkits.py | tools/gen_drumkits.py | bac9c05 | 7d08bc87f100e5fbc88a8dc3b131d9cc5103bdcb | GPL-3.0-only | no |
| app/tools/gen_fm6_patches.py | tools/gen_fm6_patches.py | bac9c05 | 1a5f55e39ffb3c0758dfadfe5a530d439b5fb259 | GPL-3.0-only | no |
| app/tools/sloop_gen_tables.py | tools/gen_tables.py | bac9c05 | c232b2bc7590eff4bca4b8cb4319684788c4e6b4 | GPL-3.0-only | no (renamed) |
| app/assets/samples-cc0/ATTRIBUTION.txt | assets/samples-cc0/ATTRIBUTION.txt | bac9c05 | e31357650ce572c24dcdd3cbe3c3781d4244d9ab | CC0-1.0 (SLOOP's record of the sources) | yes: only its header and the lines of PIANO, STRGS, HORNS (the sets copied), a `COMPOSER:` note at its end |
| app/engines/eng_sample.c | firmware/src/eng_sample.c | bac9c05 | f9aadb4082cdcc5aaa2644e793235f130b6da09d | GPL-3.0-only | yes (US-35): SLOOP's user sample slots cut (the web editor's uploads into flash: SMP_USER_*, the slot header, usr_zone, smp_user_scan, smp_zone_ok, SLICE's slc_user_scan, the fm1_xip.h include; COMPOSER has no slots nor that flash): SET picks a built-in set only (SMP_NALL = SMP_NSETS), smp_zone reads SMP_ZONES only, sample_note_on's slot branch gone; marked `COMPOSER:`. The decoder, the zones, the loops and the render are SLOOP's lines |
| app/tools/gen_samples.py | tools/gen_samples.py | bac9c05 | a2affba762a497b7929400475ff134ce40fd6b34 | GPL-3.0-only | yes (US-35): only the CC0 sets PIANO, STRGS, HORNS are read; gen_waves.py (the Hügelton Sample Pack), SLOOP's other sets, the GM kit, SLICE's BREAK, user-slot names and the build cache are cut; STRGS / HORNS loop their steady body (kind `body`, so a held note sustains); the presets are named PIANO, STRINGS, HORNS; marked `COMPOSER:` |
| app/tools/sampleio.py | tools/sampleio.py | bac9c05 | 29eb0b775ebca5f9309404e5b88b4abe6dc6acea | GPL-3.0-only | no |
| app/assets/samples-cc0/PIANO/00_C3_m48.wav | assets/samples-cc0/PIANO/00_C3_m48.wav | bac9c05 | bf1d6b3e8a9094701b8ab1cc4e0e194dcd7116b3 | CC0-1.0 | no |
| app/assets/samples-cc0/PIANO/01_Gs3_m56.wav | assets/samples-cc0/PIANO/01_Gs3_m56.wav | bac9c05 | b5f4ac00706aa3360ec72ab7387d95f326f55d77 | CC0-1.0 | no |
| app/assets/samples-cc0/PIANO/02_E4_m64.wav | assets/samples-cc0/PIANO/02_E4_m64.wav | bac9c05 | e1e9193f60673a154d6df5b7b85d238e45080b84 | CC0-1.0 | no |
| app/assets/samples-cc0/PIANO/03_C5_m72.wav | assets/samples-cc0/PIANO/03_C5_m72.wav | bac9c05 | f9a2469abb7c727dd8d979dcbdae33ca7dc9a11a | CC0-1.0 | no |
| app/assets/samples-cc0/STRGS/00_G3_m55.wav | assets/samples-cc0/STRGS/00_G3_m55.wav | bac9c05 | 676de33e9e89fded5f379a212838cef910f2aa4a | CC0-1.0 | no |
| app/assets/samples-cc0/STRGS/01_E4_m64.wav | assets/samples-cc0/STRGS/01_E4_m64.wav | bac9c05 | 76ac5dff741a37f36a648cdc1acdd06d6e0050d2 | CC0-1.0 | no |
| app/assets/samples-cc0/STRGS/02_Cs5_m73.wav | assets/samples-cc0/STRGS/02_Cs5_m73.wav | bac9c05 | 98d31ec70dff3a00193cbd1273f9d995832032a6 | CC0-1.0 | no |
| app/assets/samples-cc0/HORNS/00_G3_m55.wav | assets/samples-cc0/HORNS/00_G3_m55.wav | bac9c05 | 077e52516340dc7e46c47152d78f28eb436bbeb9 | CC0-1.0 | no |
| app/assets/samples-cc0/HORNS/01_D4_m62.wav | assets/samples-cc0/HORNS/01_D4_m62.wav | bac9c05 | 45c3528ca422fe166ad7cc5937a6daa8c2df1990 | CC0-1.0 | no |
| app/assets/samples-cc0/HORNS/02_A4_m69.wav | assets/samples-cc0/HORNS/02_A4_m69.wav | bac9c05 | 7f5373d6ff10aecd967b424a213401b5dc1be6dd | CC0-1.0 | no |
| app/assets/samples-cc0/HORNS/03_E5_m76.wav | assets/samples-cc0/HORNS/03_E5_m76.wav | bac9c05 | c5fd61cd2779e3486f7351917542fbf31d17cdbe | CC0-1.0 | no |

Functions copied into a COMPOSER file (US-18; the file is COMPOSER's, so no one blob for it):

| COMPOSER path | upstream functions | SLOOP blobs @ bac9c05 | licence | changed |
| --- | --- | --- | --- | --- |
| app/firmware/src/fx.c | firmware/src/fx.c (track_dist, lim_block, dc_block, fx_buses, djf_block, djf_run, tflt_run) + firmware/src/dsp.c (MULQ12R) | 75fc88c4ff84ad487c9f46e8b8f03be8a5f09ab6 + 7c6a2d27f416ff542d3c98825d1acde8d59c6c22 | GPL-3.0-only | functions copied; signed left shifts written as multiplications; SLOOP's song settings fixed at its defaults; pool sections (marked COMPOSER) |
| app/firmware/src/live.c | firmware/src/seq.c (SCALE_MASK, NSCALES, KB_SILENT, scale_mask, kb_pitch, CHORD_DEG, chord_notes, chord_mod_of_key, vl_prev, scale_up, sort_notes, chord_play_notes, midi_sel_on, midi_sel_get, midi_sel_set, REC_LAT, rec_m, rec_target) + firmware/src/params.c (N_SCALE's names, as LK_SCALE_NAME) | 379606c94bf2cbe2fa8ed2bcc909b22891f32782 + 6e261464403074622db23b35b79aa11e65f88826 | GPL-3.0-only | US-24: copied line for line (seq.c lines 22-40, 43, 141-144, 196-236, 252-393, 2922-2935; `tools/tests/test_live.py` compares them) behind a shim (lk_track_t; track_t and trk_index as macros); vl_prev / vl_n are sized by synth.c's NPART, 8 channels (SLOOP: 3 parts), a `COMPOSER:` line says so. US-26: seq.c lines 473-495 (REC_LAT 512 samples and rec_target, SLOOP's latency compensation, with its comments) copied line for line (`tools/tests/test_record.py` compares them) behind a shim: `song.g[G_BPM]` is the tempo x10 (so a sample adds it, a tick is 275625 units), `trk_grid` gives REC's grid step under the playhead, `track_t` holds `seq_abs`; quantize OFF uses the step and its nudge (`rec_m`, 1/64 step) as ticks, as SLOOP's QUANT OFF does. The rest of live.c (keys, the piece's key, MIDI routing for 8 channels, midi_in_lost's release, the recorder: count-in, click, held notes, REPLACE, the pass log in edit.c) is COMPOSER's |
| app/firmware/src/player.c | firmware/src/seq.c (swing_units) | 379606c94bf2cbe2fa8ed2bcc909b22891f32782 | GPL-3.0-only | US-25: copied line for line (seq.c lines 396-400; `tools/tests/test_steps.py` compares them); its `clamp` is engines/dsp.c's (the same). It is fed 1/1000 ticks (a step = 24000), the result turned into the player's accumulator units. The rest of player.c's step code (loop lengths, the swung notes' queue) is COMPOSER's: SLOOP's step storage and seq_tick bring a second note store (research/sloop-reconcile.md §3) |
| app/firmware/src/synth.c | firmware/src/voice.c (SLIDE_T, glide_ticks, glide_set, mono_play, mono_pick, mono_remove; trk_note_on's and trk_note_off's MONO / LEGATO parts, as mono_on / mono_off; track_render's glide, as glide_tick) + firmware/src/ui.c (a MONO preset plays LEGATO, preset_extras: VOICE, GLIDE, GLMODE) | 530ce2f702e83ef7328b3989612e7baf60b35d12 + 5d33b9797ea3075249fc791de881662ed63f12fa | GPL-3.0-only | US-30: voice.c lines 255-281, 390-447, 496-509, 536-553 and 850-858 copied line for line (`tools/tests/test_voices.py` compares them) behind a shim: voice_room is COMPOSER's voice budget (synth_on's steal), trk_nvoice is 1 and unison_spread nothing (no UNISON, SPEC decision 48), `pend` is voice_t's unused `side`, `pitch_frac` its unused `mrnd` (Felucca's voice_t layout kept); lines 869-872 (the glide's fraction into the increment) written into synth_render. sloop_voice (sloop_glue.c, AUTO) is COMPOSER's, after ui.c line 657 and preset_extras. The rest (the piece's mode per channel, SLIDE through player.c, live notes on a MONO channel) is COMPOSER's; unlike SLOOP (SLIDE on MONO / LEGATO only), SLIDE also glides on POLY: the slid note's voice is handed to the next note, legato (synth_slide / slide_hand, SPEC decision 54) |
| app/firmware/src/edit.c | firmware/src/ui_layers.c (pattern_rotate, pattern_length, pattern_transpose; LY_ERASE's knob routing) | e629aa393a43b95b30cfad011c617271d7fd2ad1 | GPL-3.0-only | US-31: the logic, adapted (not line for line): SLOOP's steps are an array, COMPOSER's notes a delta-coded event list, so `edit_tool` rewrites the channel's events. As SLOOP: KNOB1 every step one later / earlier with the last wrapping to the first (here a grid step, wrapping in the loop or the bars of notes), KNOB2 x2 = the pattern again after itself up to 64 steps / 1/2, KNOB3 a semitone on synth parts only (`is_drum`: refused), the knob turns of one hold one undo (`layer_undo_mark`; here one snapshot in the pass log's room). Changed: transpose refuses a turn that would take a note past 0..127 (SLOOP clamps it); 1/2 drops the second half's notes (SLOOP keeps them past the length); the routing (held EDIT on the EDIT screen) is in composer.c |

Generated, not copied: `app/firmware/src/felucca_drumkits.h` (`tools/gen_drumkits.py`, unchanged, writes it);
`app/firmware/src/drum_tables.h` (`tools/gen_drum_tables.py`, ours: SLOOP's `tools/gen_tables.py` DECAY_K formula,
blob `c232b2bc7590eff4bca4b8cb4319684788c4e6b4`, line for line). Our glue: the end of `app/firmware/src/drums.c`
(US-21).
Generated from them (committed, `tools/tests/test_fm6.py` regenerates and compares):
`app/engines/felucca_fm6.h` (`tools/gen_fm6_patches.py`), `app/engines/fm6_tables.h` (`tools/gen_fm6_tables.py`, the
FM6 part of SLOOP's `gen_tables.py`); `app/engines/sloop_tables.h` (`tools/gen_sloop_tables.py`: ENV_LIN and ENV_EXP as
SLOOP's `gen_tables.py` writes them, for DIGITAL and VOICE; `tools/tests/test_engines.py` regenerates and compares).
US-20 (PHYS, DIGITAL, VOICE) copies five files unchanged and narrows nothing in them: every preset of the three engines
plays. COMPOSER does not play a preset's extras (`XP`: transpose, LFO; since US-30 their voice mode and glide are played,
sloop_voice), its sends or pattern hint (sloop_glue.c).
`phys_dsp.c` and `phys_symp.c` are MIT (their headers: DaisySP, Copyright (c) 2020 Electrosmith, Corp, Emilie Gillet;
Rings, Copyright (c) 2015 Emilie Gillet; the fixed-point ports 2026 Leo Kuroshita, Hügelton Instruments).
`fm6_core.c` is a port of msfa (Dexed's FM core): Copyright 2012 Google Inc., 2016-2025 Pascal Gauthier, C port
2026 Leo Kuroshita; Apache License 2.0 (`app/engines/LICENSE-Apache-2.0-msfa.txt`). The GPL-3.0-only files are
Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments. SLOOP's assets (logo, Hügelton art) are not
copied.

US-35 (SLOOP's SAMPLE engine, v11): `engines/eng_sample.c` is compiled by synth.c with SLOOP's engine_t layout under
another name, as sloop_glue.c does for PHYS (COMPOSER's ENG_SAMPLE, the glue in synth.c: the same functions and presets,
an `amp` hook that ends a held note of a set at sustain 0 once its decay has faded, `sampled` set so a retrigger starts the
recording again); it takes no engine-pool block. Generated from the copied files (committed, `tools/tests/test_samples.py`
regenerates and compares): `app/engines/felucca_samples.h` (`tools/gen_samples.py` from the eleven CC0 WAVs above:
59546 B of IMA ADPCM). The WAVs are SLOOP's CC0 files byte for byte (VCSL's Steinway B grand; VSCO-2 CE's violin + viola
sections spiccato, trumpet + tenor trombone staccato, retuned and cut by SLOOP's tools/gen_builtin_hiphop.py, not
copied); SLOOP's other sets, its CREDITS.txt and anything gen_waves.py makes (the Hügelton Sample Pack, all rights
reserved) are not copied.

## Through Dinghy (found by the reconciliation, 2026-11-23)

These came with Dinghy, the platform COMPOSER is built on (`app/firmware/src/platform.c` lines 2-3: "taken unchanged from
SLOOP/Felucca/sloopDX"), not copied by a COMPOSER story, so they had no row. Compared with SLOOP at `bac9c05`
(`research/sloop-reconcile.md` §1, craft-anything evidence in `research/craft/`). GPL-3.0-only like SLOOP. Dinghy's own
commit for them is not pinned (open: the reconciliation's unknown `unk_9401bf5ed47ae93f`).

| COMPOSER path | SLOOP path | relation |
| --- | --- | --- |
| app/firmware/src/libc.c | firmware/src/libc.c | identical |
| app/firmware/src/uac_fir.h | firmware/src/uac_fir.h | identical |
| app/tools/get_toolchain.sh | tools/get_toolchain.sh | identical |
| app/tools/lz4blk.py | tools/lz4blk.py | identical |
| app/firmware/src/midi_uart.c | firmware/src/midi_uart.c | changed: 15 lines (US-12 SysEx) and SLOOP 2.8.3's midi_in_lost (US-24: its 2 lines, the flag declared by composer.c), marked `COMPOSER:` |
| app/firmware/src/usb.c, lcd.c, gfx.c, ota.c, recovery.c, bootguard.h | firmware/src/ (same names) | derived: 6 to 186 lines differ (Dinghy's and COMPOSER's changes) |
| app/tools/build.py, fm1_install.py, fm1pkg_make.py | tools/ (same names) | derived: differ |

