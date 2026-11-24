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

Functions copied into a COMPOSER file (US-18; the file is COMPOSER's, so no one blob for it):

| COMPOSER path | upstream functions | SLOOP blobs @ bac9c05 | licence | changed |
| --- | --- | --- | --- | --- |
| app/firmware/src/fx.c | firmware/src/fx.c (track_dist, lim_block, dc_block, fx_buses, djf_block, djf_run, tflt_run) + firmware/src/dsp.c (MULQ12R) | 75fc88c4ff84ad487c9f46e8b8f03be8a5f09ab6 + 7c6a2d27f416ff542d3c98825d1acde8d59c6c22 | GPL-3.0-only | functions copied; signed left shifts written as multiplications; SLOOP's song settings fixed at its defaults; pool sections (marked COMPOSER) |
| app/firmware/src/live.c | firmware/src/seq.c (SCALE_MASK, NSCALES, KB_SILENT, scale_mask, kb_pitch, CHORD_DEG, chord_notes, chord_mod_of_key, vl_prev, scale_up, sort_notes, chord_play_notes, midi_sel_on, midi_sel_get, midi_sel_set, REC_LAT, rec_m, rec_target) + firmware/src/params.c (N_SCALE's names, as LK_SCALE_NAME) | 379606c94bf2cbe2fa8ed2bcc909b22891f32782 + 6e261464403074622db23b35b79aa11e65f88826 | GPL-3.0-only | US-24: copied line for line (seq.c lines 22-40, 43, 141-144, 196-236, 252-393, 2922-2935; `tools/tests/test_live.py` compares them) behind a shim (lk_track_t; track_t and trk_index as macros); vl_prev / vl_n are sized by synth.c's NPART, 8 channels (SLOOP: 3 parts), a `COMPOSER:` line says so. US-26: seq.c lines 473-495 (REC_LAT 512 samples and rec_target, SLOOP's latency compensation, with its comments) copied line for line (`tools/tests/test_record.py` compares them) behind a shim: `song.g[G_BPM]` is the tempo x10 (so a sample adds it, a tick is 275625 units), `trk_grid` gives REC's grid step under the playhead, `track_t` holds `seq_abs`; quantize OFF uses the step and its nudge (`rec_m`, 1/64 step) as ticks, as SLOOP's QUANT OFF does. The rest of live.c (keys, the piece's key, MIDI routing for 8 channels, midi_in_lost's release, the recorder: count-in, click, held notes, REPLACE, the pass log in edit.c) is COMPOSER's |
| app/firmware/src/player.c | firmware/src/seq.c (swing_units) | 379606c94bf2cbe2fa8ed2bcc909b22891f32782 | GPL-3.0-only | US-25: copied line for line (seq.c lines 396-400; `tools/tests/test_steps.py` compares them); its `clamp` is engines/dsp.c's (the same). It is fed 1/1000 ticks (a step = 24000), the result turned into the player's accumulator units. The rest of player.c's step code (loop lengths, the swung notes' queue) is COMPOSER's: SLOOP's step storage and seq_tick bring a second note store (research/sloop-reconcile.md §3) |

Generated, not copied: `app/firmware/src/felucca_drumkits.h` (`tools/gen_drumkits.py`, unchanged, writes it);
`app/firmware/src/drum_tables.h` (`tools/gen_drum_tables.py`, ours: SLOOP's `tools/gen_tables.py` DECAY_K formula,
blob `c232b2bc7590eff4bca4b8cb4319684788c4e6b4`, line for line). Our glue: the end of `app/firmware/src/drums.c`
(US-21).
Generated from them (committed, `tools/tests/test_fm6.py` regenerates and compares):
`app/engines/felucca_fm6.h` (`tools/gen_fm6_patches.py`), `app/engines/fm6_tables.h` (`tools/gen_fm6_tables.py`, the
FM6 part of SLOOP's `gen_tables.py`); `app/engines/sloop_tables.h` (`tools/gen_sloop_tables.py`: ENV_LIN and ENV_EXP as
SLOOP's `gen_tables.py` writes them, for DIGITAL and VOICE; `tools/tests/test_engines.py` regenerates and compares).
US-20 (PHYS, DIGITAL, VOICE) copies five files unchanged and narrows nothing in them: every preset of the three engines
plays. COMPOSER does not play a preset's extras (`XP`: transpose, LFO, glide), its sends or pattern hint (sloop_glue.c).
`phys_dsp.c` and `phys_symp.c` are MIT (their headers: DaisySP, Copyright (c) 2020 Electrosmith, Corp, Emilie Gillet;
Rings, Copyright (c) 2015 Emilie Gillet; the fixed-point ports 2026 Leo Kuroshita, Hügelton Instruments).
`fm6_core.c` is a port of msfa (Dexed's FM core): Copyright 2012 Google Inc., 2016-2025 Pascal Gauthier, C port
2026 Leo Kuroshita; Apache License 2.0 (`app/engines/LICENSE-Apache-2.0-msfa.txt`). The GPL-3.0-only files are
Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments. SLOOP's assets (logo, Hügelton art) are not
copied.

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

