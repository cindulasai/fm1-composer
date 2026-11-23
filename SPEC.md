# fm1-composer
seed: AI generates multi-channel music on the Mac (text2midi), pushes it to an M-VAVE FM-1 (emulator first, device over USB later); the FM-1 firmware keeps a library of pieces, user picks one, plays it, picks a sound per channel, mutes/solos. Inspired by SLOOP, not copied. Production quality for the community.
stack: C99 firmware on Dinghy 0.5 platform (GPL-3.0) + Felucca-Engines (MIT); Python 3 Mac tool `fm1c` (mido, python-rtmidi) · toolchain: available (host mode cc + pytest locally; JieLi build on GitHub Actions; fm1-emu 0.5.1 binary) · run: `fm1c emu` / `fm1c push piece.f1p` · check: `sh app/check.sh`

## Product
Firmware "COMPOSER" for the FM-1: a library of up to 8-channel pieces generated on the Mac, browsed and played on the device, each channel's sound chosen on the device. Mac tool `fm1c` composes (via text2midi, unchanged), fits pieces to the FM-1 and pushes them over USB-MIDI. For music lovers who want finished arrangements on a $65 synth.

## Scenarios
S1. `fm1c compose ~/Claude/Projects/text2midi_v2/suite/briefs/electronic-house-1.json -o out/electronic-house-1.f1p` → that file + report "8 channels, 412 notes, 0 reductions" (or each reduction listed).
S2. `fm1c emu` starts fm1-live with COMPOSER and USB; `fm1c push out/*.f1p` → "pushed 3, verified 3"; FM-1 screen LIBRARY lists the 3 names, newest first.
S3. On FM-1: SELECT turns to a piece, PLAY → PLAYER screen, 8 channel strips with role + sound name, playhead bar:beat moving; music plays at the piece's tempo.
S4. In PLAYER: KNOB1 picks channel, PRESETS turns its sound (heard at once); key F3..F4 (white keys 1–8) toggle mute; hold SEL + key = solo. SAVE stores sound choices; after power-off and back, piece reopens with those sounds.
S5. Push while playing: music keeps playing; piece staged, saved to flash at next stop; `fm1c push` prints "staged — saves when playback stops" then "saved" once stopped.
S6. Cable pulled / power cut mid-push or mid-save: library afterwards = exactly the pieces committed before; no corrupt entry listed; next push works.
S7. In PLAYER: EDIT opens the selected channel as a piano roll; SELECT moves the cursor, a key adds that note (or removes it), KNOB2/KNOB3 set length/velocity, FX undoes; PLAY plays from the cursor's bar; SAVE → the edited piece replaces the old one, `fm1c pull` shows the new notes.
S8. `./fm1c studio` opens COMPOSER Studio: left, text2midi's own window; I type "slow melancholic lo-fi in D minor, 70 bpm" and press Make it; text2midi opens the piece (parts, piano roll; Play previews it). Right, fm1-emu's page runs COMPOSER; I press Start, then Send to FM-1 →: a progress bar, then "On the FM-1: … (read back from its flash; N channels, M notes …)"; it is at the top of LIBRARY; PLAY (or Space) plays it. Its assumptions are not shown: text2midi's window does not show them (unchanged)

## Features
| ID | Feature (one testable line) | Since | State |
|----|-----------------------------|-------|-------|
| F1 | Firmware COMPOSER builds from Dinghy (own identity FM-1_95N, own store magic), host mode on macOS, `.fwsc` by CI | v1 | shipped |
| F2 | Piece format `.f1p` v1: header + ≤8 channels + note events + CRC32; Python writer, C reader, identical semantics | v1 | shipped |
| F3 | `fm1c convert`/`compose`: MIDI or text2midi brief → `.f1p` fitted to FM-1 limits, every reduction reported | v1 | shipped |
| F4 | Synth: 8 channels, shared voice pool (16 target, measured), Felucca engines + own drum kit, sound catalog with role defaults | v1 | shipped |
| F5 | Player: sample-accurate-to-0.73 ms sequencing, tempo ±, loop, mute/solo, playhead | v1 | shipped |
| F6 | Library: pieces in flash, atomic commit, delete, survives power cut at any write; capacity shown | v1 | shipped |
| F7 | Transfer: SysEx protocol (ID 0x7D) INFO/LIST/PUT/DEL/GET with chunk CRC + ack; `fm1c push/list/delete/pull`; staging while playing | v1 | shipped |
| F8 | UI: LIBRARY, PLAYER (8 strips; sound turned in place with PRESETS); per-piece sounds, mutes, tempo saved | v1 | shipped |
| F9 | Emulator e2e: real `.fwsc` in fm1-live — push over emulated USB, navigate, play, audio, flash cut, strict CPU | v1 | shipped |
| F10 | Note edit core: insert / delete / change notes of a piece in RAM; piece valid after every op; undo log | v2 | shipped |
| F12 | Pieces over the MIDI-in jack: SysEx on the TRS input reaches the same transfer as USB | v3 | shipped |
| F13 | Studio engine (WASM): text2midi's composer (copied, pinned) + fit + .f1p encode in Rust, byte-identical to `fm1c compose` | v3 | shipped |
| F14 | Studio page: an idea in plain English → AI (from the browser, user's key) → brief → piece; key in a git-ignored local file or the page's settings | v3 | shipped |
| F16 | Studio left = text2midi's own interface (Svelte UI copied unchanged @ pinned commit; its Tauri commands served in the browser by t2m-session → WASM + browser AI): gallery, prompt, takes, lock, undo, piano roll, preview audio | v4 | shipped |
| F17 | Studio right = fm1-emu's own page (unchanged, pinned) booting COMPOSER via its ?package= link; Studio is a virtual MIDI input there ("COMPOSER Studio"); Send to FM-1 = text2midi's MIDI → fit → .f1p → SysEx over that input → LIBRARY, plays | v4 | shipped |
| F15 | Side by side: fm1-emu's browser build (MIT, pinned) runs COMPOSER on the same page; the piece is pushed over the emulated MIDI jack, appears in LIBRARY, plays | v3 | shipped |
| F11 | EDIT screen: piano roll per channel; keys toggle notes at the cursor; grid, length, velocity, octave; play from the cursor's bar; SAVE replaces the piece | v2 | shipped |
| F18 | Effects from SLOOP (fx.c, pinned): per channel filter + drive, sends to reverb / delay / chorus, master limiter + DC block; set per channel on the device, saved with the piece | v5 | shipped |
| F19 | SLOOP's FM6 engine (6-op FM, DX7 patches) + its preset bank, copied unchanged where possible | v5 | shipped |
| F20 | SLOOP's PHYS (physical models), DIGITAL (4-op FM) and VOICE (formant) engines + presets | v5 | shipped |
| F21 | SLOOP's 32 synthesized drum kits (drum_synth.c) beside COMPOSER's 3 | v5 | shipped |
| F22 | text2midi's instruments, roles and mix reach the FM-1: each part gets a fitting sound and pan (fm1c and Studio, same bytes) | v5 | shipped |

## Design
piece (`app/firmware/src/piece.h`, `app/tools/fm1c/piece.py`) — interface: `piece_check(buf,len)->err`, `piece_view(buf)->{name,tempo_x10,len_ticks,nch,ch[i]{role,sound,vol,pan,nev}}`, `piece_events(view,ch)->iter{tick,dur,pitch,vel}`; py `encode(Piece)->bytes`, `decode(bytes)->Piece` — hides: binary layout (LE, 96 PPQ, delta ticks), CRC32, version checks.
fit (`app/tools/fm1c/fit.py`) — interface: `fit(midi_path, limits)->(Piece, Report)` — hides: role detection from track names, channel merge/drop by role priority, polyphony cap, range fold, bend→nearest key, quantize, length cap.
link (`app/tools/fm1c/link.py`) — interface: `Link(port).info()/list()/put(name,bytes)/delete(id)/get(id)` raising `LinkError(code,msg)` — hides: SysEx framing, 7-bit packing, chunking, CRC, retries, timeouts.
cli (`app/tools/fm1c/cli.py`, `emu.py`) — `fm1c compose|convert|push|list|delete|pull|emu` exit 0 ok / 1 device refused / 2 usage or file.
xfer (`app/firmware/src/xfer.c`) — interface: `xfer_frame(f,n,now,playing)->ours` (platform SysEx hook, ahead of the updater), `xfer_poll(now,playing)` (main loop) — hides: protocol state, staging buffer (pool, 32 KB), idempotent DATA, replies.
lib (`app/firmware/src/lib.c`) — interface: `lib_mount()`, `lib_count()`, `lib_entry(i)->{id,name,len}`, `lib_read(i,dst,max)->len|err`, `lib_put(name,buf,len)->id|err`, `lib_del(id)`, `lib_find(id)`, `lib_free()`, `lib_room()` — hides: whole-sector entries in 0x97000–0xDFFFF (73 sectors), 64-byte header with CRCs written last, delete = one word programmed, lazy erase of dirty sectors.
synth (`app/firmware/src/synth.c`) — interface: `synth_sound(ch,id)`, `synth_on(ch,pitch,vel)`, `synth_off(ch,pitch)`, `synth_all_off()`, `synth_render(out,n)`; `sounds[]` catalog — hides: voice pool + stealing, engines, drum kit, mixer, levels.
player (`app/firmware/src/player.c`) — interface: `player_load(view)`, `player_play/stop()`, `player_tempo(x10)`, `player_mute/solo(ch,on)`, `player_tick(nframes)` (audio ISR, per 32 frames), `player_pos()` — hides: event cursors, note-off queue, loop wrap, stuck-note safety.
edit (`app/firmware/src/edit.c`) — interface: `edit_insert(b,ch,tick,pitch,dur,vel)`, `edit_delete(b,ch,tick,pitch)`, `edit_set(b,ch,tick,pitch,dur,vel)` → ED_OK|ED_ARG|ED_FULL|ED_EXISTS|ED_NONE; `edit_undo(b)` — hides: event shifting, delta/rest re-encoding, channel offsets, length, CRC, undo log (64 ops).
studio engine (`app/studio/engine`, Rust → WASM) — interface: `prompt() -> system text`, `brief_from_reply(text) -> brief JSON | error`, `make(brief_json, seed) -> {midi, f1p, report[], channels, notes}` — hides: text2midi's crates (copied from text2midi_v2 @9540490, see PROVENANCE.md), SMF read, fit, encode.
studio page (`app/studio/web`) — interface: index.html = two frames (t2m/ = text2midi's window, emu/ = fm1-emu's page) + Send to FM-1; cable.js = Web MIDI input "COMPOSER Studio" in fm1-emu's page; fm1link.js = transfer SysEx + library reader — hides: pacing (2.5 B/ms), read-back from fm1-emu's IndexedDB
studio desk (`studio/src/desk.rs` + `t2m-ui/studio/*.js`) — interface: text2midi's Tauri commands (invoke name + args JSON → result JSON) — hides: t2m-session + t2m-audio state, browser AI (text2midi's providers), projects in localStorage
ui (`app/firmware/src/composer.c`, `ui.c`) — app_main/app_audio; screens LIBRARY, PLAYER, EDIT, drawn where dirty; prefs in blob 0 keyed by piece id (200, least recently saved replaced).

## Test plan
| Layer | Tool + driver | Covers |
|-------|---------------|--------|
| unit | C host binaries `app/tests/*_test.c` (cc), pytest `app/tools/tests` | piece, lib (RAM flash sim), xfer, player timing, synth voice pool, fit rules |
| e2e | host mode `app/tests/host_e2e.sh` (scripted keys/MIDI/SysEx → screen-text log + WAV); `fm1c` via subprocess | every UI + CLI criterion; screen text, WAV, files re-read |
| hostile | pytest tables: CLI args, MIDI files, piece bytes; C: corrupt SysEx, corrupt flash | battery per surface |
| scenarios | `app/tests/scenarios.sh` S1–S6 on host mode; S2/S5/S6 also on emulator | replay |
| property | Hypothesis ≥1000: encode→C reader dump equals input; lib random op sequences + random power cuts keep invariants | round trip, atomicity |
| perf | fm1-live FM1_STRICT `--bench` | 16 voices × engines ≤ 60% of 5.8 ms audio budget, no starved halves |
| e2e-emu | `app/tests/emu_e2e.sh`: CI `.fwsc` + fm1-live `--shot --usb` FM1_USB_SEND/FM1_BENCH_KEYS/--flash/FM1_FLASH_CUT | F9; skipped with notice when no `.fwsc`/fm1-live |

## Decisions
| # | Decision | By | Material |
|---|----------|----|----------|
| 1 | Firmware for FM-1, not a phone app; emulator (fm1-emu) first, device later | user | yes |
| 2 | Base: Dinghy platform + Felucca-Engines; own app code; SLOOP inspiration only | user | yes |
| 3 | 8 channels per piece | user | yes |
| 4 | Generator: text2midi run unchanged (`t2m-cli export --profile approx`); custom FM-1 generator only if fit proves insufficient | user | yes |
| 5 | Phase 1 device scope: browse, play, sound per channel, mute/solo, tempo, save; note editing = Phase 2 | user | yes |
| 6 | Never flash a real FM-1 without explicit go-ahead; repo private | user | yes |
| 7 | JieLi build on GitHub Actions (disk 99% full; no local VM/Docker) | council | no |
| 8 | Own SysEx tap with ID 0x7D ahead of the updater's SysEx; updater untouched | council | yes |
| 9 | Push while playing → stage in RAM, commit at stop (flash erase silences audio ~50 ms/sector) | council | no |
| 10 | Library in 0x97000–0xDFFFF (292 KB, 73 sectors = all of FL_DATA; 0xE0000 is the updater's staging); blobs = per-piece prefs; settings = global | council | yes |
| 11 | Piece max 32 KB, 96 PPQ, ≤ 4096 notes/channel; longer → truncated with report | council | no |
| 12 | Drums: own small synth kit (GM notes 35–59 subset) since Felucca-Engines has none | council | no |
| 13 | Identity FM-1_95N (unused in hub list as of 2026-11-22); verify before public release | council | yes |
| 14 | fm1c finds the FM-1 by its USB name "Felucca" (shared by Felucca-based firmwares; the updater relies on it, so unchanged), else "FM-1" | council | no |
| 15 | DATA idempotent: an identical repeat of the last chunk is re-acknowledged; fm1c retries DATA (a lost frame or answer costs one retry) | council | no |
| 20 | Studio = static browser page, no back end: AI called from the browser (MiniMax, Anthropic, DeepSeek, OpenAI allow it: CORS checked 2026-11-22) | user | yes |
| 21 | Emulator on the page = fm1-emu's web build (MIT, commit 588db9e988ea), unmodified, fetched + built by tools/get_web_emu.sh | council | yes |
| 22 | Push in the page over the emulated MIDI jack (the web build has no USB); firmware accepts transfer SysEx on the jack too | council | yes |
| 25 | Reuse, never recreate: Studio = text2midi's UI (left) + fm1-emu's page (right), both unchanged, joined by a virtual MIDI cable; only glue is ours (invoke shim, MIDI shim, Send button); custom device pane and own form removed | user | yes |
| 23 | While private: the user's MiniMax key in app/studio/web/local/ai.json (git-ignored, made by tools/studio_key.sh); a test fails if any tracked file holds it; public page = backlog | user | yes |
| 24 | Engine copies text2midi_v2's crates (core, compose, midi, intent) unchanged; text2midi_v2 itself untouched | user | yes |
| 17 | Edit while stopped only (bytes shift under the player); PLAY in EDIT plays from the cursor's bar | council | no |
| 18 | SAVE of an edited piece = put new entry, then delete old (power cut between: both kept, nothing lost); prefs move to the new id | council | yes |
| 19 | EDIT controls: SELECT cursor, KNOB1 grid 1/32..1/4, KNOB2 length, KNOB3 velocity, OCT-/+ octave, keys toggle, FX undo | council | no |
| 16 | platform_poll hands the updater only a frame the app saw and declined (fixes a frame lost between the two, found under FM1_STRICT) | council | no |
| 26 | Device shape: arrangement player + live play + a step sequencer on all 8 tracks (exceeds SLOOP's 3+1); sound first (v5), live play + sequencer next (v6) | user | yes |
| 27 | Samples: build synthesized engines first, measure flash, add sampled instruments only if the library keeps ≥ 200 KB (code space: 97.5 KB of 568 KB used at v4) | user | yes |
| 28 | One core, 16 voices: SLOOP's second-core path stays off until a real-device test (known crashes on some units) | user | yes |
| 29 | SLOOP 2.8.5 (isod89/sloop-fm1 @ bac9c05, GPL-3.0) code copied unchanged where possible, pinned + provenance; our glue only; its assets (logo, Hügelton art) not copied | user | yes |
| 30 | Float: tested in the emulator before any float code; until then integer only (RULES.md) | user | no |
| 31 | v4 pieces keep playing; every new sound plays the densest piece with 0 starved halves in fm1-emu's on-chip timing (FM1_STRICT) | user | yes |
| 32 | Sound ids: v4's 0–39 unchanged, SLOOP's 32 kits 40–71, FM6 72–79; new engines always appended. PRESETS on DRUMS/PERC turns through kits only, elsewhere skips them | council | yes |
| 33 | SLOOP's fm6_core.c kept byte for byte: sanitizers allow signed left shifts and wrap-around (as SLOOP's own test run); fm6_bank.c copied, not built (its flash bank: FM6 plays COMPOSER's 8 presets) | council | no |
| 34 | Sound from text2midi's instrument, else the part's first GM program, else the role default (sounds.py = sounds.rs, one table). Only 3 FM6 channels at once (3 engine blocks): fit swaps a 4th+ FM6 part for a near non-FM6 sound and reports it | council | no |
| 35 | Engine pool: 8 blocks of 8.6 KB (PHYS's state), so PHYS / DIGITAL / VOICE play on all 8 channels; FM6 stays capped at 3 (ENG_FM6_MAX). Pool section 319.5 KB of 344 KB (CI build): the rest of v5/v6 must fit in ~24 KB of pool. SLOOP's XP extras, sends and pattern hints of these presets are not played | council | yes |
| 36 | Measured at v5 (decision 27): code image 150.6 KB of 568 KB, the library keeps its 292 KB (separate flash), so ~430 KB is free for sampled instruments; v5 ships without samples (135 synthesized sounds), sampled instruments go to the Backlog. Float (decision 30): none needed, all v5 DSP is integer | council | no |

## Claims
"better than SLOOP" → goal, not yet a claim: README states measured facts only (channels, engines, sounds, effects vs SLOOP 2.8.5's published numbers). README: "inspired by SLOOP".
"16 voices" → narrowed to the measured number in README.

## Backlog
- Sampled instruments (piano, strings, choir) in the ~430 KB of free code flash (decision 36), if a sound test shows the synthesized ones fall short
- v6 (next, decided): live play + step sequencer on 8 tracks: launch sections at the next bar, keyboard in the piece's key with auto-chords, live MIDI in, energy + swing knobs, send from Studio while playing, step programming/recording per track
- Sampled instruments (CC0 piano, strings, horns from SLOOP) + GRAIN: after v5's flash measurement (decision 27)
- Second core for more voices: after a real-device test (decision 28)
- Versus SLOOP (isod89/sloop-fm1, a mature live groovebox): COMPOSER wins on AI arrangements from plain English, an 8-part arrangement player, and Studio in the browser. To close the gaps, most valuable first: one-click install from the browser; more sounds and engines (FM6 / DX7 patches, physical models); a setlist that chains pieces; piano-roll editing in Studio before pushing; per-channel filter and effects; MIDI out of the playing piece
- Public Studio page: free / demo AI keys or an in-browser model (no key); public repo or other free static host — user decides
- Desktop app (Studio packaged)
- Studio: Web MIDI push to a real FM-1 over USB; audio preview before pushing
- Describe-in-words on the Mac (text2midi LLM path) from `fm1c`
- Own 4-op FM engine (FM sounds on an FM synth)
- Custom FM-1-shaped generator if fit reductions are frequent
- Song chaining / playlist; MIDI out of the sequence; clock sync
- SMK-37 build; Firmware Hub listing; real-device test pass (owner)

## Versions
- v1 — Phase 1: library player + fm1c (US-1–US-9)
- v2 — Phase 2: note editing on the device (US-10–US-11)
- v3 — COMPOSER Studio in the browser, emulator side by side (US-12–US-15)
- v4 — Studio = text2midi's UI + fm1-emu's page, reused unchanged, MIDI cable between (US-16–US-17)
- v5 — SLOOP's sound engine on 8 channels: effects, FM6, PHYS/DIGITAL/VOICE, 32 kits; text2midi's instruments mapped (US-18–US-22) — verified
