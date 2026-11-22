# fm1-composer
seed: AI generates multi-channel music on the Mac (text2midi), pushes it to an M-VAVE FM-1 (emulator first, device over USB later); the FM-1 firmware keeps a library of pieces, user picks one, plays it, picks a sound per channel, mutes/solos. Inspired by SLOOP, not copied. Production quality for the community.
stack: C99 firmware on Dinghy 0.5 platform (GPL-3.0) + Felucca-Engines (MIT); Python 3 Mac tool `fm1c` (mido, python-rtmidi) · toolchain: available (host mode cc + pytest locally; JieLi build on GitHub Actions; fm1-emu 0.5.1 binary) · run: `fm1c emu` / `fm1c push piece.f1p` · check: `sh app/check.sh`

## Product
Firmware "COMPOSER" for the FM-1: a library of up to 8-channel pieces generated on the Mac, browsed and played on the device, each channel's sound chosen on the device. Mac tool `fm1c` composes (via text2midi, unchanged), fits pieces to the FM-1 and pushes them over USB-MIDI. For music lovers who want finished arrangements on a $65 synth.

## Scenarios
S1. `fm1c compose ~/Claude/Projects/text2midi_v2/suite/briefs/electronic-house-1.json` → `out/electronic-house-1.f1p` + report "8 channels, 412 notes, 0 reductions" (or each reduction listed).
S2. `fm1c emu` starts fm1-live with COMPOSER and USB; `fm1c push out/*.f1p` → "pushed 3, verified 3"; FM-1 screen LIBRARY lists the 3 names, newest first.
S3. On FM-1: SELECT turns to a piece, PLAY → PLAYER screen, 8 channel strips with role + sound name, playhead bar:beat moving; music plays at the piece's tempo.
S4. In PLAYER: KNOB1 picks channel, PRESETS turns its sound (heard at once); key F3..C4 (white keys 1–8) toggle mute; hold SEL + key = solo. SAVE stores sound choices; after power-off and back, piece reopens with those sounds.
S5. Push while playing: music keeps playing; piece staged, saved to flash at next stop; `fm1c push` prints "staged — saves when playback stops" then "saved" once stopped.
S6. Cable pulled / power cut mid-push or mid-save: library afterwards = exactly the pieces committed before; no corrupt entry listed; next push works.

## Features
| ID | Feature (one testable line) | Since | State |
|----|-----------------------------|-------|-------|
| F1 | Firmware COMPOSER builds from Dinghy (own identity FM-1_95N, own store magic), host mode on macOS, `.fwsc` by CI | v1 | planned |
| F2 | Piece format `.f1p` v1: header + ≤8 channels + note events + CRC32; Python writer, C reader, identical semantics | v1 | planned |
| F3 | `fm1c convert`/`compose`: MIDI or text2midi brief → `.f1p` fitted to FM-1 limits, every reduction reported | v1 | planned |
| F4 | Synth: 8 channels, shared voice pool (16 target, measured), Felucca engines + own drum kit, sound catalog with role defaults | v1 | planned |
| F5 | Player: sample-accurate-to-0.73 ms sequencing, tempo ±, loop, mute/solo, playhead | v1 | planned |
| F6 | Library: pieces in flash, atomic commit, delete, survives power cut at any write; capacity shown | v1 | planned |
| F7 | Transfer: SysEx protocol (ID 0x7D) INFO/LIST/PUT/DEL/GET with chunk CRC + ack; `fm1c push/list/delete/pull`; staging while playing | v1 | planned |
| F8 | UI: LIBRARY, PLAYER (8 strips), SOUND picker; per-piece sound choices saved | v1 | planned |
| F9 | Emulator e2e: real `.fwsc` in fm1-live — push over emulated USB, navigate, play, audio, flash cut, strict CPU | v1 | planned |

## Design
piece (`app/firmware/src/piece.h`, `app/tools/fm1c/piece.py`) — interface: `piece_check(buf,len)->err`, `piece_view(buf)->{name,tempo_x10,len_ticks,nch,ch[i]{role,sound,vol,pan,nev}}`, `piece_events(view,ch)->iter{tick,dur,pitch,vel}`; py `encode(Piece)->bytes`, `decode(bytes)->Piece` — hides: binary layout (LE, 96 PPQ, delta ticks), CRC32, version checks.
fit (`app/tools/fm1c/fit.py`) — interface: `fit(midi_path, limits)->(Piece, Report)` — hides: role detection from track names, channel merge/drop by role priority, polyphony cap, range fold, bend→nearest key, quantize, length cap.
link (`app/tools/fm1c/link.py`) — interface: `Link(port).info()/list()/put(name,bytes)/delete(id)/get(id)` raising `LinkError(code,msg)` — hides: SysEx framing, 7-bit packing, chunking, CRC, retries, timeouts.
cli (`app/tools/fm1c/cli.py`) — `fm1c compose|convert|push|list|delete|pull|emu` exit 0 ok / 1 device refused / 2 usage or file.
xfer (`app/firmware/src/xfer.c`) — interface: `xfer_byte(b)` (from platform SysEx tap), `xfer_poll()` (main loop) — hides: protocol state, staging buffer (pool, 32 KB), replies.
lib (`app/firmware/src/lib.c`) — interface: `lib_mount()`, `lib_count()`, `lib_entry(i)->{id,name,len}`, `lib_read(i,dst,max)->len|err`, `lib_put(name,buf,len)->id|err`, `lib_del(id)`, `lib_find(id)`, `lib_free()`, `lib_room()` — hides: whole-sector entries in 0x97000–0xDFFFF (73 sectors), 64-byte header with CRCs written last, delete = one word programmed, lazy erase of dirty sectors.
synth (`app/firmware/src/synth.c`) — interface: `synth_sound(ch,id)`, `synth_on(ch,pitch,vel)`, `synth_off(ch,pitch)`, `synth_all_off()`, `synth_render(out,n)`; `sounds[]` catalog — hides: voice pool + stealing, engines, drum kit, mixer, levels.
player (`app/firmware/src/player.c`) — interface: `player_load(view)`, `player_play/stop()`, `player_tempo(x10)`, `player_mute/solo(ch,on)`, `player_tick(nframes)` (audio ISR, per 32 frames), `player_pos()` — hides: event cursors, note-off queue, loop wrap, stuck-note safety.
ui (`app/firmware/src/composer.c`) — app_main/app_audio; screens LIBRARY, PLAYER, SOUND; prefs in blobs keyed by piece id.

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

## Claims
"better than SLOOP" → narrowed: not a claim; different product (library player vs live groovebox). README: "inspired by SLOOP".
"16 voices" → narrowed to the measured number in README.

## Backlog
- Note editing on device (piano roll per channel) — Phase 2
- Describe-in-words on the Mac (text2midi LLM path) from `fm1c`
- Own 4-op FM engine (FM sounds on an FM synth)
- Custom FM-1-shaped generator if fit reductions are frequent
- Song chaining / playlist; MIDI out of the sequence; clock sync
- SMK-37 build; Firmware Hub listing; real-device test pass (owner)

## Versions
- v1 — Phase 1: library player + fm1c (US-1–US-9)
