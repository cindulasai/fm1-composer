# SLOOP 2.8.5 ↔ COMPOSER reconciliation (craft-anything)

2026-11-23. SLOOP `research/sloop-src` @ `bac9c05ea7627449405aa10e48da21d75b6c8c56` (checked with `git rev-parse`).
COMPOSER at `build/m1-m2` @ `9540490`. Tool: craft-anything-cli 0.6.1, Deep mode. The project and evidence live in
`research/craft/` (git-ignored by `research/` in `.gitignore:19`): `reconcile.cany`, `evidence-bundle.json` (34 records,
1 unknown), `craft-report.md` (findings fnd_1–fnd_5), and the raw output in `out/`.
Paths: `S/` = `research/sloop-src/firmware/src/`, `C/` = `app/firmware/src/`. Line numbers are 1-based.

## Evidence keys (craft-anything ledger)

| key | evidence id | what |
| --- | --- | --- |
| E-ENG | ev_5d313788b97350448979c3742a4f5a4c468a56ccce8db1e2550c6d89c6fc9bb6 | core-archives-compare `S/` → `app/engines` |
| E-FW | ev_a07d8bd169d22b3f8279259953cd746fc580cf3f8cc73d977ca83476dc11c449 | core-archives-compare `S/` → `C/` |
| E-TOOLS | ev_f1a63ff85d835d0a1335a29046dc53e4b2d6a5d1c52cf9335098cc403e01bb8a | core-archives-compare SLOOP `tools/` → `app/tools` |
| E-LIC | ev_8979f38279c9a3cc40e9aa9cf512412290e5a7f9721ec9e72f177f4125bb85ef | core-archives-compare `LICENSES/` → `app/engines` |
| E-GCMP | ev_9d7cd4c2b97c54482e9f98d69bbdc91a5f0b8c9a494bbd934936c6cea34db679 | app-graphs-compare (empty graphs for C, see the tool notes) |
| E-SCL | ev_5fae76a85bca16f25418ac3474accfc994ad0aed433a6b19553fda33250e9550 | `S/seq.c:22-41` SCALE_MASK |
| E-KB | ev_c47abbc11b74c93414bf681c64fecc0afe81e5295758057056253ffbdc596d86 | `S/seq.c:182-251` kb_scaled, kb_pitch, kb_map |
| E-CH | ev_df6639ad3d9e36f9fcf83c744248bccfa61c4018c3d182984187cfbc51c445f4 | `S/seq.c:252-394` chords, CHORD+, VLEAD |
| E-SW | ev_c09b9d73dd7c8834123db51d5cb057884c403e2761441d40435880069cd68a42 | `S/seq.c:395-444` swing and grid, trk_len |
| E-ADD | ev_5fcf2b40b2933c430c1f2502bbc5e73e032c10b119c7798959602f03c6b508e8 | `S/seq.c:507-534` step_add |
| E-INP | ev_b8986252562dee794720bf58d9b9876c8562fab3b8fdcdf814cdaf4f2c7567b9 | `S/seq.c:1889-1964` input_on / input_off |
| E-KEY | ev_67111862d306446f7cfe663eeca94ed896d091f80066c39e9677521ccbe0718e | `S/seq.c:2196-2320` key_down |
| E-STEP | ev_1c4e8553962f7691d6661fb3cb2ad6cfea736b52aeae4c47252c582ddb8bc49e | `S/seq.c:2643-2742` step_vel, seq_step |
| E-TICK | ev_a2b91645ca681527de5cc92407a56548e23bc5a727468a268f8267e55c625be4 | `S/seq.c:2804-2902` micro_units, seq_tick |
| E-MRT | ev_999caab9cae33e2455ebb88072ec9dd0f279c1c35516dac65382fac00596133d | `S/seq.c:2903-2953` midi_own / midi_route |
| E-CC | ev_8225ac30169cb0b57f66158ff0db8fde3e31a833f1c666f615bff9111a2cba4e | `S/seq.c:2975-3130` CC map, MIDI LEARN, sustain |
| E-CLK | ev_4ff02785e9cb0dfa41d75df7c32fd2a7df03317d5d8487c5df1cf1e063bf1225 | `S/seq.c:3131-3270` clock in / out |
| E-MIN | ev_dc2d30e914c37515bde4ddf66b6052e76ab3ec081133a258d8883810db468049 | `S/seq.c:3437-3560` MIDI IN dispatch |
| E-STT | ev_ad46f54afe036765ee14556ccb2d721d1eb66dad48502f5c1f61a61aa60b6cc2 | `S/core.h:187-233` step_t, dstep_t, NLOCK |
| E-CORE | ev_aec9b3bdc270f450583379668c8c27af81110fcbcd0762ae31d6cac9b1b91d92 | `S/core.h:344-373` clock units, DIV |
| E-PAR | ev_f002066f8448facdf7ba44a76268bba0e6b724a0df8a36afa000e89ffd404479 | `S/params.c:95-134` ranges |
| E-VOI | ev_b6f06d7686058998888670f7dbb7cc2645d89f774ab9dab8732a2715ac2e9161 | `S/voice.c:1-128` the voice budget |
| E-LEN | ev_7767b96f9d0ac1eb358513d60120f1b5d9e70d8577d28ef146d0eb5118a78c56 | `S/ui.c:195-216` note_len, note_set_len |
| E-LVL | ev_fbbac456708e456053a6c32957a2defc5f9e10168e72488ab95bdbc2809bb1db | `S/drums.c:176-185` lvl_vel, vel_lvl |
| C-MIDI | ev_881a8c8eea75baac724372a75e7db066d5275c78529640790071386a30266c46 | `C/composer.c:71-80` MIDI in plays nothing |
| C-SYN | ev_98b73bc95fa056e5746f03dc58b7a5ebc02721b539de239f9c3de4686f8f93e3 | `C/synth.c:1-12` budget and steal order |
| C-PLAT | ev_ab1a62cb46bc20d38ca5c004552a161d28427fcde8e9d8df7baabe8403867b66 | `C/platform.c:1-5` Dinghy, "taken unchanged from SLOOP / Felucca / sloopDX" |
| C-PIECE | ev_aec6329edf02d6b0bbb6c34ea253f408c96905039122d73200242a58232af646 | `C/piece.h:1-22` format, 96 PPQ, no key / scale field |
| C-V6 | ev_446b0777524566acf653b19b683d63a970aca22d3097eee3d08aac85ac98ba9e | `stories/v6.md` |
| C-DEC | ev_3849b8c42c8d1a141b22647d2081d845934cb87fb691aed56ff056ae734a42f4 | `SPEC.md:109-141` decisions 33–40, Backlog |

## 1. Copied-code check (SLOOP-PROVENANCE.md, every row)

The git blob of each COMPOSER file was compared with SLOOP's (`git hash-object`) and with the declared id. Each pair was
also diffed by sha256 (E-ENG, E-FW, E-TOOLS, E-LIC) and, where a file differs, line by line.

| COMPOSER | upstream | result | evidence |
| --- | --- | --- | --- |
| app/engines/LICENSE-Apache-2.0-msfa.txt | LICENSES/Apache-2.0-msfa.txt | unchanged (renamed; blob 75b5bab = declared) | E-LIC moved |
| app/engines/eng_digital.c | eng_digital.c | unchanged (8f3c0f8) | E-ENG unchanged |
| app/engines/eng_fm6.c | eng_fm6.c | unchanged (c4fb384) | E-ENG |
| app/engines/eng_formant.c | eng_formant.c | unchanged (75288f9) | E-ENG |
| app/engines/eng_phys.c | eng_phys.c | unchanged (ecabff6) | E-ENG |
| app/engines/fm6_bank.c | fm6_bank.c | unchanged (7b7df32), not built, as declared | E-ENG |
| app/engines/fm6_core.c | fm6_core.c | unchanged (e54f495) | E-ENG |
| app/engines/phys_dsp.c | phys_dsp.c | unchanged (008434f) | E-ENG |
| app/engines/phys_symp.c | phys_symp.c | unchanged (780abf4) | E-ENG |
| C/drum_synth.c | drum_synth.c | changed as declared: 2 lines, `C/drum_synth.c:293` and `:315` (`shl`, each marked `COMPOSER:`); nothing else differs | E-FW changed; diff |
| app/tools/drumkit_levels.json | tools/drumkit_levels.json | unchanged (6a12651) | E-TOOLS |
| app/tools/gen_drumkits.py | tools/gen_drumkits.py | unchanged (7d08bc8) | E-TOOLS |
| app/tools/gen_fm6_patches.py | tools/gen_fm6_patches.py | unchanged (1a5f55e) | E-TOOLS |
| app/tools/sloop_gen_tables.py | tools/gen_tables.py | unchanged (renamed; c232b2b) | E-TOOLS moved |
| C/fx.c (functions) | fx.c + dsp.c | changed as declared. Function bodies diffed: track_dist, lim_block, djf_block, djf_run and MULQ12R are identical; dc_block (2 lines), fx_buses (3 lines) and tflt_run (1 line) change only `<<` into `*` (declared). The glue (`C/fx.c:7-9`, marked `COMPOSER:`) fixes SLOOP's song FX settings at their defaults and runs FILTER as a low-pass only, "127 open" | fn diff (`S/fx.c:30,78,130,205,375,401,418` vs `C/fx.c:73,121,170,187,299,325,342`) |

**Every declared row matches** (craft finding fnd_1). The provenance's "functions copied unchanged" in fx.c holds, apart from
the declared shift-to-multiply edits.

**Undeclared, outside the provenance (via Dinghy).** `C/libc.c`, `C/uac_fir.h`, `app/tools/get_toolchain.sh` and
`app/tools/lz4blk.py` are **byte-identical to SLOOP's** (cmp). `C/midi_uart.c` is SLOOP's file with 15 lines changed
(US-12 SysEx; marked `COMPOSER:`). `C/usb.c`, `lcd.c`, `gfx.c`, `ota.c`, `recovery.c`, `bootguard.h` and
`app/tools/build.py`, `fm1_install.py`, `fm1pkg_make.py` share SLOOP's names and differ by 6 to 186 lines (E-FW, E-TOOLS).
`C/platform.c:2-3` says the Dinghy platform was "taken unchanged from SLOOP / Felucca / sloopDX / OberVave" (C-PLAT), and
SPEC F1 and decision 2 name Dinghy. No pinned Dinghy commit or file list exists, so these GPL files have no row in any
provenance. This is open as `unk_9401bf5ed47ae93f`.
Not SLOOP copies: `app/engines/{dsp,eng_analog,eng_lofi,eng_noise,eng_phase,eng_trio}.c` carry `SPDX: MIT`; SLOOP's carry
GPL-3.0-only. They are Felucca-Engines, as SPEC F4 says.
Side finding: COMPOSER's `midi_uart.c` lacks SLOOP 2.8.3's `midi_in_lost` (`S/midi_uart.c:75-76`). When the ring is full,
SLOOP releases every note MIDI holds (`S/seq.c:3547-3559`), so no note sticks; COMPOSER does not. US-24.5 needs this.

## 2. Feature table

Status: **cp** copied · **cp+** copied+changed · **AI** rebuilt for AI pieces · **v6** planned in v6 · **BL** Backlog ·
**NT** not taken (reason) · **DIFF** behaves differently · **GAP** not in SPEC (Features or Backlog; see §4).

| # | SLOOP feature | status | COMPOSER / difference | evidence |
| --- | --- | --- | --- | --- |
| 1 | FM6 6-op FM + DX7 patches | cp | F19; bank file copied, not built (8 presets) | E-ENG; SPEC.md:39,109 |
| 2 | PHYS, DIGITAL, VOICE engines | cp | F20 | E-ENG; SPEC.md:40 |
| 3 | ANALOG, PHASE, LOFI, TRIO, NOISE, WHEEL | NT | newer Felucca-Engines MIT copies used instead | E-ENG changed; headers |
| 4 | SAMPLE, GRAIN (SLICE not built in SLOOP) | BL | sampled instruments + GRAIN | SPEC.md:123,125 |
| 5 | preset extras XP (LFO, glide, transpose), preset sends, pattern hint | NT | decision 35: not played | SPEC.md:111; `C/sloop_glue.c:20-21` |
| 6 | 32 user presets, sound pages (ENV/LFO/EDIT knobs) | DIFF | presets only, chosen per channel and saved per piece (F8); no parameter editing | SPEC.md:28 |
| 7 | voice modes MONO/LEGATO/UNISON, glide, PRIO, ALLOC | GAP | always POLY | `C/sloop_glue.c:37` |
| 8 | voice budget | DIFF | 16 voices over 8 channels; steal: released → same channel's oldest → oldest. SLOOP: 8 over 3 parts; steal: released → extra unison → held non-lowest (bass kept) | C-SYN; E-VOI |
| 9 | DIST, track FILTER, chorus/delay/reverb sends, limiter, DC | cp+ | F18; FILTER low-pass only; song FX fixed at defaults | `C/fx.c:4-11` |
| 10 | INSERT (8 types), SLICER | GAP | none | `S/fx.c:622-698` |
| 11 | master DUST, DUCK, DJ FILT | GAP | none | SLOOP.md:369-377; `S/fx.c:336-338` |
| 12 | TEXTURE beds, REV/ECHO THROW | GAP | none | `S/core.h` G_TX*; SLOOP.md:371 |
| 13 | 27 punch-in FX | GAP | none (`rev_frz` kept in fx.c, no punch.c) | SLOOP.md:381-407; `C/fx.c:68` |
| 14 | 32 synthesized drum kits | cp | F21 (+2 lines, §1) | E-FW |
| 15 | 5 sampled + user kits | BL | samples | SPEC.md:125 |
| 16 | hi-hat choke | GAP | none in `C/drums.c` | `S/drums.c:252` |
| 17 | drum levels G/S/N/H; per-lane ratchet | DIFF | notes carry velocity 1-127; ratchets out (decision 39) | E-LVL; SPEC.md:115 |
| 18 | 64 steps per track | v6 | US-25 (F25) | C-V6 25.1 |
| 19 | per-track LEN 1-64, polymeter | v6 | US-25.2 | C-V6; E-SW `trk_len` |
| 20 | per-track DIV (1/4…1/32, 8T, 16T, 1/2, 1, 2 bars) | DIFF | F25 fixes the grid at 1/16 | E-PAR `P_SDIV`; C-V6 25.1 |
| 21 | chords ≤4 per step | v6 | US-25.1 | E-STT |
| 22 | ties / note LEN | v6 | US-25.1 "length in steps"; a piece note has a dur, so no TIE steps are needed | E-STEP; E-LEN |
| 23 | slide (glide into the next step) | GAP | needs glide (#7) | E-STEP `SF_SLIDE` |
| 24 | accent | DIFF | velocity only (SLOOP's ACC = 127) | E-STEP `step_vel` |
| 25 | per-step velocity | v6 | US-25.1 | E-STEP |
| 26 | GATE (track %) | DIFF | each note keeps its own dur | E-STEP `gate` |
| 27 | ratchets x1-x4 | NT | decision 39 | SPEC.md:115 |
| 28 | swing 0-100 (MPC; track + global) | v6 | US-25.3; the same formula (§3.6) | E-SW; C-V6 25.3 |
| 29 | probability | NT | SLOOP has no per-step probability, only ARP PROB (`S/seq.c:1769`); arp not in SPEC | E-PAR `P_APROB` |
| 30 | fill conditions | NT | decision 39 | E-STT `FC_*` |
| 31 | parameter locks (24 per track) | NT | decision 39 | E-STT `NLOCK` |
| 32 | nudge ±32/64 step | AI | piece notes keep free ticks (96 PPQ) | C-PIECE; E-TICK |
| 33 | arpeggiator (14 modes + OFF, HOLD) | GAP | none | `S/params.c:38-39` |
| 34 | note repeat / roll | GAP | none | SLOOP.md:325 |
| 35 | step entry from a MIDI keyboard (the pen) | GAP | US-25 covers the FM-1 keys only | E-MIN `step_in_push` |
| 36 | undo / redo | DIFF | 64-op undo log (F10); SLOOP keeps one level | SPEC.md:30; `S/seq.c:448-468` |
| 37 | pattern tools: SHIFT, ×2 / ½, TRANSPOSE; erase while playing | GAP | none | SLOOP.md:315-323 |
| 38 | 16 scales (CHR + 15) | v6 | US-24.1 says **12** scales: a mismatch to settle | E-SCL; C-V6 24.1 |
| 39 | KEYS OFF / SNAP / WHITE | v6 | US-24.1: scale on white keys + chromatic; SNAP not named | E-KB |
| 40 | key of the song (SEL + key sets the root) | AI | key and scale from text2midi's brief or estimated (24.1); the piece has no key field yet | C-PIECE; C-V6 24.1 |
| 41 | chord modes TRIAD/7TH/9TH/SUS4/POWER | v6 | US-24.2: triad or 7th only | E-CH; C-V6 24.2 |
| 42 | CHORD+ modifiers, VLEAD, STRUM | GAP | none | E-CH; `S/voice.c:580` |
| 43 | octave buttons; TRN ±24 | v6 | US-24.4 OCT−/+; TRN not named | E-KB |
| 44 | KBD HOLD, sustain CC64, pitch bend | GAP | none | E-INP; E-CC |
| 45 | MIDI IN notes | DIFF (v6) | US-24: MIDI notes follow the key and chords; **SLOOP plays MIDI notes raw** (fnd_2) | E-MIN; E-INP; C-MIDI |
| 46 | MIDI channel routing per track, OTHER = SEL / OFF, IN = CH n / CLOCK | GAP | F24: MIDI plays the selected channel | E-MRT; E-MIN |
| 47 | CC map, MIDI LEARN (16), PC + Bank, CC11 expression | GAP | none | E-CC |
| 48 | MIDI clock in (USB / TRS) | BL | "clock sync" | E-CLK; SPEC.md:134 |
| 49 | MIDI clock out | BL | "clock sync" | E-CLK; SPEC.md:134 |
| 50 | MIDI out of the keys and the sequencer | BL | "MIDI out of the sequence" | `S/seq.c:92-139`; SPEC.md:127,134 |
| 51 | REMOTE CC buttons, FX channel | GAP | none | E-MIN |
| 52 | sections A–D launched on the next bar | AI | US-23 done (stories all [x]), ≤24 sections taken from text2midi's form; SPEC F23 still says "planned" | C-V6 23.x; SPEC.md:43 |
| 53 | quick chain ≤8, song of 16 steps, song recording | DIFF | queues one next section; no song recording | SLOOP.md:508-518; C-V6 23.3 |
| 54 | 4 project slots, autosave | DIFF | flash library of pieces with explicit SAVE (F6, F8); no autosave | SPEC.md:26 |
| 55 | live record, overdub, quantised | v6 | US-26.1/26.2 | C-V6 |
| 56 | latency compensation (REC_LAT 512 samples, "as heard") | GAP | US-26 does not name it | `S/seq.c:476-495` |
| 57 | REC QUANT OFF | v6 | US-26.1 "(or not)" | `S/seq.c:496-505` |
| 58 | free take (the loop sets the tempo), count-in, click | GAP | none | `S/seq.c:1374`, `:3288-3327` |
| 59 | hold REC to clear a track | GAP | none | SLOOP.md:364 |
| 60 | TRACKS performance screen; layers | AI | PLAYER with 8 strips; no hold-layers | SPEC.md:28 |
| 61 | mute / solo | AI | F5 / F8, 8 channels | SPEC.md:25 |
| 62 | tempo; tap tempo | DIFF | tempo ± per piece; no tap tempo | SPEC.md:25; SLOOP.md:357 |
| 63 | NOTES piano roll | AI | F11 EDIT piano roll | SPEC.md:37 |
| 64 | DRUMS grid page | v6 | US-25.4 STEPS screen (8 × 16) | C-V6 |
| 65 | 12 visualisers | GAP | none (low value) | SLOOP.md:595 |
| 66 | web editor (live mirror, Web MIDI) | AI | Studio: text2midi's UI + fm1-emu (F16, F17) | SPEC.md:34-35 |
| 67 | browser installer | BL | "one-click install from the browser" | SPEC.md:127 |
| 68 | backup / restore | AI | `fm1c list/pull` (F7) | SPEC.md:27 |
| 69 | USB audio input (44.1 / 48 kHz) | cp+ | via Dinghy (`FELUCCA_UAC 1`, `uac_fir.h` identical); undeclared, §1 | `C/platform.c:111`; E-FW |
| 70 | second core | BL | decision 28 | SPEC.md:104,126 |

**Counts (70 rows):** copied 3 · copied+changed 2 · rebuilt for AI pieces 8 · planned in v6 13 · Backlog 7 ·
not taken with a SPEC reason 6 · behaves differently 11 (#45 also lands in v6) · GAP (not taken, no reason in SPEC) 20.
By row: cp {1,2,14}; cp+ {9,69}; AI {32,40,52,60,61,63,66,68}; v6 {18,19,21,22,25,28,38,39,41,43,55,57,64};
BL {4,15,48,49,50,67,70}; NT {3,5,27,29,30,31}; DIFF {6,8,17,20,24,26,36,45,53,54,62};
GAP {7,10,11,12,13,16,23,33,34,35,37,42,44,46,47,51,56,58,59,65}.

### Earlier notes checked against evidence

- `research/sloop.md` §2.1 "15 arp modes": N_AMODE has 15 names **including OFF**, so 14 modes (`S/params.c:38-39`).
- §2.1 Sequencer "per-step velocity and LEN": LEN is a note's span in the NOTES editor (`S/ui.c:195-238`), not a field
  of a step; a step has note[4], n, time, flags, vel, lvl, rat (E-STT).
- §4.2 "COMPOSER today: no sends, no reverb, no limiter, no filter": out of date since v5 (F18, row 9).
- §2.1 "8 voices shared, drums own 6": confirmed (`S/core.h:9`, `S/drums.c:10`).
- `research/community.md` row 12 "probability … mostly met by top firmwares": SLOOP has no per-step probability (row 29).
- Lineage claim (SLOOP's engines are older Felucca GPL copies, COMPOSER's newer MIT copies): confirmed by the licence
  headers and E-ENG (6 changed files).

## 3. v6 extraction (US-24, US-25)

COMPOSER's model: a piece holds note events (tick, dur, pitch, vel) at 96 PPQ; a 1/16 step = 24 ticks (C-PIECE). Steps
edit those notes (decision 38). SLOOP's model: each track holds `step_t step[64]` (10 bytes; E-STT) and plays them from
one transport grid (`seq_tick`, E-TICK). So **SLOOP's playback and step storage (seq_step, seq_tick, step_add, the TIE /
SF_CONT machinery) cannot be copied**: they would bring a second note store. The pure pitch and time math can be copied
with the bodies unchanged, behind a small type shim, as was done for fx.c (fnd_5).

| topic | SLOOP behaviour (file:function:line) | copy into COMPOSER as-is? |
| --- | --- | --- |
| 3.1 Scales | `S/seq.c:SCALE_MASK:22-39`: 12-bit masks, bit i = semitone i above ROOT. In order: CHR 0xFFF; MAJ {0,2,4,5,7,9,11}; MIN {0,2,3,5,7,8,10}; DOR {0,2,3,5,7,9,10}; MIX {0,2,4,5,7,9,10}; PEN {0,2,4,7,9}; MPEN {0,3,5,7,10}; HARM {0,2,3,5,7,8,11}; PHRY {0,1,3,5,7,8,10}; LYD {0,2,4,6,7,9,11}; LOC {0,1,3,5,6,8,10}; MEL {0,2,3,5,7,9,11}; BLUES {0,3,5,6,7,10}; WHOLE {0,2,4,6,8,10}; DIMHW {0,1,3,4,6,7,9,10}; DIMWH {0,2,3,5,6,8,9,11}. Names `S/params.c:47-48`. `scale_mask():141-144` clamps the index. Note: ROOT and SCALE are per track (`S/params.c:103-104`), but the SEL layer sets them for all three parts at once (SLOOP.md:345,350) | **Yes, unchanged** (a const table). It gives 16 scales, not US-24.1's 12: settle which. `scale_mask` needs `P_SCALE` from a shim struct |
| 3.2 Auto-chord rules | `S/seq.c:chord_notes:262-284`: CHORD_DEG = OFF{0}, TRIAD{0,2,4}, 7TH{0,2,4,6}, 9TH{0,2,6,8} (1-3-7-9, no 5th), SUS4{0,3,4}, POWER = semitones {0,7,12}. Degrees count up the scale from note n (a 48-step guard; notes ≥128 dropped). SCALE = CHR means the chord comes from MIN (`:264`). CHORD+ `chord_mod_of_key:291-301` (black keys: F# minor↔major third, G# +7th, A# sus4, C# +9th, D# inversion) and `chord_play_notes:323-393`: the modifiers; the 9th replaces the 5th when four notes are already there; sorted; VLEAD picks the inversion (0..k-1) × octave (−1..+1) with the least total semitone motion from the part's last chord, inside 24..108; ≤4 notes. Revoiced live as modifiers change: `chord_revoice:2129-2169` | **chord_notes, scale_up (303-313), sort_notes (314-321): yes, unchanged** behind the shim (`p[P_CHORD]`, `p[P_SCALE]`, `p[P_ROOT]`). **chord_play_notes: body unchanged, but `vl_prev[NPART][4]` must have 8 parts** (`:302`; NPART = 3) and `trk_index` must return a COMPOSER channel. chord_revoice is tied to `kb_*` key state and input_on: rewrite |
| 3.3 Notes mapped to keys | 27 keys, key k → note 53+k (F3..G5) (`S/seq.c:kb_pitch:203`). `kb_pitch:200-236`: **OFF** = 53+k+12·oct+TRN. **SNAP** (QNT 1, chord off) = rounded **down** into the scale (≤12 steps). **WHITE** (QNT 2, or any chord mode) = the white keys walk scale degrees with C4 (k=7) = 60+ROOT, octaves counted by the scale's size (5/6/7/8/12 notes, no repeated degree); black keys silent (`KB_SILENT`; with a chord on they are CHORD+ modifiers, `key_down:2281-2284`). Then +12·oct+TRN, clamped 0..127. TRN −24..24 (`S/params.c:106`). `kb_map:238-250` sends drum and GM-kit tracks to lanes instead | **kb_pitch: yes, body unchanged** behind the shim (p[P_QUANT/P_CHORD/P_SCALE/P_ROOT/P_TRANS]; octave passed in). kb_map: no (ENGINES, drum lanes, SLICE are SLOOP's) |
| 3.4 MIDI IN | `S/seq.c:events_block:3437-3546`: channel → track via `midi_own:2905-2914` (DRCH, default 10 → drums; CH1..CH3 → parts 1-3), else the selected track unless OTHER = OFF (`midi_track:2916-2920`). `midi_route:2936-2952` and `midi_sel_on[16][64]` (nibbles) remember, per channel and note, which track the note-on went to, so the note-off reaches it after a track change. 0x90 with vel 0 = note-off (`:3482`, `:3530`). IN = CH n: that channel only, on the selected track (`:3456-3488`); IN = CLOCK: notes ignored, note-offs still pass (`:3520`). **Notes go to `input_on(t, d1, d2)` raw: no scale, no chord** (`:3539`; E-INP 1889-1935); only the FM-1 keys use kb_pitch / chord_play_notes (E-KEY). Velocity is kept. CC / PC / bend / sustain `:3490-3517`. Lost message → every MIDI-held note released (`:3547-3559`). Running status in the UART parser (`S/midi_uart.c:52`; COMPOSER's copy has it, `C/midi_uart.c:53`) | **Behaviour differs from F24**: SLOOP does not scale or chord MIDI notes; COMPOSER plans to. Copy the routing idea: `midi_sel_on` and its get/set (`2924-2935`) can be copied unchanged; midi_own / midi_route need COMPOSER's 8 channels instead of `trk[]`/TSEL. Port midi_in_lost (§1). The rest is new code over `C/composer.c:73-77` (C-MIDI) |
| 3.5 Voice reservation | **None in SLOOP.** `S/voice.c:75-126`: one NVOICE = 8 budget over 3 parts (drums have their own 6); `voice_victim:96-126` takes the oldest released voice, then the oldest extra UNISON voice, then (hard only) a held POLY voice that is not the part's lowest note; `voice_room:178-196`; a voice taken from another part fades over one block (stage 4). Keys, MIDI and the sequencer share the budget (fnd_3) | **Nothing to copy** for "4 of 16 reserved" (decision 40, US-24.3): new code in `C/synth.c` steal_pick (C-SYN). Worth copying as an idea: never take a part's lowest held note (the bass) |
| 3.6 Swing formula | `S/seq.c:swing_units:397-400` = clamp(pct,0,100)·u/200 (100 → half a step = MPC 75%). `grid_swing:402-418`: odd steps start `sw` late, so an even step lasts u+sw and an odd one u−sw. pct = track SWG + global SWING (`trk_grid:438-442`; both 0..100, default 0, `S/params.c:109,158`; the sum is clamped). Swing only on 1/4, 1/8, 1/16, 1/32 (`swings:436`); off on triplets and whole-beat DIVs. Ratchets and the recording stay on the unswung grid (`S/seq.c:2812-2817`) | **swing_units, grid_swing: yes, unchanged** (pure integer). US-25.3 already states this formula. Apply it in the player's units (60·10·44100 per tick, `C/player.c:21`), not in ticks: at 24 ticks a step, ticks give only 4% swing resolution |
| 3.7 Step and chord limits | `S/core.h`: NSTEP 64 (`:15`); step_t 10 B = note[4], n (0 = empty), time NOTE/TIE/REST, flags (ACCENT 1, SLIDE 2, CONT0..3 8-64, LEG 0x80), vel (0 → 96), lvl (2 bits a note: NORM/GHOST/SOFT/HARD), rat (2 bits a note: x1..x4) (`:188-213`); micro −32..31 (1/64 step, `:222-223`); NLOCK 24 (`:224`). Overdub `step_add:509-533` adds up to 4 notes (when full, the last is replaced; MONO parts keep 1). Velocity `step_vel:2643-2647`: ACCENT → 127, else vel or 96, then lvl_vel GHOST 42 / SOFT 72 / HARD 127 (`S/drums.c:176-180`). MIDI vel → level `vel_lvl:182-185` (<56 G, <88 S, <116 N, else H) | **No** (a storage format). Keep the limits: ≤4 notes a step, ≤64 steps, vel 1..127. COMPOSER's limits are 32 KB a piece and ≤4096 notes a channel (decision 11) |
| 3.8 Tie and length | A TIE step continues the NOTE before it. `seq_step:2652-2742`: gate = slen·GATE/128 (GATE 1..127, default 64 = 50%, `S/params.c:110`). A NOTE followed by a TIE (or by a CONT/LEG step) is held (`seq_hold`); at the TIE step `seq_off = gate + slen/2` (`:2662`), so at the default GATE a tied note ends exactly at the end of its last TIE step, while a lone note lasts 50% of its step. SLIDE holds into the next step with a glide (`:2710-2711`). Ratchets divide the gate (`:2703-2709`). Note LEN in the editor: `note_len` counts the TIEs, `note_set_len` writes RESTs/TIEs (`S/ui.c:195-215`); a 2.8.3 POLY note's LEN runs on under later notes as SF_CONT (`S/seq.c:587-773`) | **No.** In the piece model a note's dur is its length, so "length in steps" = dur / 24 and a tie is just a longer dur; no TIE or CONT flags are needed. Choose the gate rule on purpose: SLOOP's default gives L·24−12 ticks for L = 1 but L·24 for L ≥ 2 |
| 3.9 Per-track length | `trk_len:443` = P_SLEN (1..64, default 16, `S/params.c:107`). Step index = grid step `abs % len` (`seq_tick:2854`), and abs comes from the one transport clock (`trk_grid`), so every LEN stays in phase (polymeter). A DIV change takes effect at the next step (`:2831-2836`). LEN is part of undo (`S/seq.c:463`); the cursor wraps at LEN (`S/ui.c:289-304`) | **The rule only** (index = global step mod LEN, from the section or loop start). COMPOSER's player walks event cursors (`C/player.c`), so each channel's wrap is new player code |

**Verdict for US-24/25.** Copyable unchanged, behind a type shim: SCALE_MASK + scale_mask, kb_pitch, chord_notes,
scale_up, sort_notes, chord_mod_of_key, swing_units, grid_swing, midi_sel_get/set. chord_play_notes needs one change
(NPART 3 → 8 for VLEAD's memory). Rewrite: kb_map, chord_revoice, MIDI routing (8 channels), voice reservation (none in
SLOOP), and everything about steps (storage, TIE, gate, playback, per-track wrap). The shim is a struct with
`int16_t p[]` indexed by SLOOP's P_ROOT/P_SCALE/P_QUANT/P_TRANS/P_CHORD/P_VLEAD; COMPOSER's `track_t` is Felucca-Engines'
and has none of these (`C/sloop_glue.c:88` already mirrors some SLOOP P_ ids for this reason).

## 4. Gaps: SLOOP behaviours COMPOSER lacks that are not in SPEC (Features or Backlog)

Ranked by value for AI-composed pieces against cost (S ≤ 1 day, M a few days, L a week or more).

| rank | gap | value for AI pieces | cost | evidence |
| --- | --- | --- | --- | --- |
| 1 | MIDI IN routed per channel to all 8 channels (a DAW plays COMPOSER as an 8-part expander), with note-off memory and lost-message release | high: an AI piece can be played or reharmonised from a DAW, part by part | S | E-MRT; E-MIN 3547-3559 |
| 2 | Auto-chord voice leading (VLEAD) + CHORD+ modifiers + SUS4/9TH/POWER | high: live chords sit near the piece's own voicings instead of jumping root-position | S (copy, §3.2) | E-CH 323-393 |
| 3 | Recording "as heard": REC_LAT latency compensation, nearest-step rule, count-in + click | high for US-26: without it, notes land late in the piece | S | `S/seq.c:476-495`, `:3288-3327` |
| 4 | MONO / LEGATO voices + glide + SLIDE | high: text2midi's bass and lead parts are monophonic; SLOOP's presets with glide extras play wrong today | M | `S/voice.c:264-294,392-455`; `C/sloop_glue.c:37` |
| 5 | Part tools: transpose, shift, ×2 / ½ a channel | med-high: re-key or vary an AI part on the device without the Mac | S-M (edit.c batch) | SLOOP.md:315-323 |
| 6 | Per-track DIV incl. 8T / 16T and slow 1/2–2 bars (F25 fixes 1/16) | med: triplet and swing styles in AI pieces fit no 1/16 grid | M | E-PAR `N_SDIV`; E-CORE |
| 7 | Master DUCK (kick sidechain) / DUST / DJ filter | med: genre feel for house/lo-fi pieces | S-M (copy duck_block etc.) | `S/fx.c:336-338` |
| 8 | MIDI CC map, sustain, pitch bend, PC + Bank, MIDI LEARN | med: live expression over the piece | M | E-CC |
| 9 | LFO and preset extras (XP) | med: FM6/PHYS/VOICE presets lose vibrato and tremolo (decision 35 only says "not played") | M | `C/sloop_glue.c:20-21` |
| 10 | Hi-hat choke | med-low: realism of AI drum parts | S | `S/drums.c:252` |
| 11 | Arpeggiator + KBD HOLD on the live keyboard | low-med (pieces already carry written arpeggios) | M | `S/seq.c:1535-1775` |
| 12 | Song recording of live launches; chains of ≤8 sections | low-med | M | SLOOP.md:508-518 |
| 13 | 27 punch-in FX | low-med (performance) | L | SLOOP.md:381-407 |
| 14 | Step entry from a MIDI keyboard (the pen) | low-med | S | E-MIN `step_in_push` |
| 15 | Tap tempo; autosave of edits; hold REC to clear | low | S each | SLOOP.md:357,361-366 |
| 16 | INSERT, SLICER, TEXTURE, throws, visualisers, free take | low | M-L | rows 10, 12, 58, 65 |

## Tool notes (craft-anything-cli 0.6.1, early alpha)

Worked: `core-archives-compare` (4 runs; folder vs folder, sha256 added/changed/moved; it gives no list of unchanged
names, only a count), `app-source-import` (3 runs: file inventory with sha256 only; "0 script(s) analysed" for C),
`core-text-read` (25 evidence records), `core-finding-record` (5), `core-unknown-add` (1), `core-evidence-export`,
`core-report-write`, `core-session-show`.
Failed or misleading: `app-graphs-compare` refuses app-source-import evidence ("needs js_app_map"); given two different C
folders by path it returns `identical_graph: true` with 0 modules (E-GCMP): a false "identical". `app-feature-follow`
on SLOOP's web editor (inline JS in one HTML file) finds nothing for "swing", even with kind=any, match=contains.
`core-feature-find` refuses text files ("not native machine code"). The function-level checks were therefore done with
git blob ids, cmp, diff and a function-body differ.
Not run, as instructed: setup, rebuild-*, assets-*, ai-*.
