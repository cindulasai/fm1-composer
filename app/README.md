# COMPOSER for the M-VAVE FM-1

COMPOSER is community firmware for the M-VAVE FM-1. It turns the FM-1 into a player for finished, multi-part
arrangements made on your computer. You keep a library of pieces on the FM-1, pick one, play it, and choose
the sound of each of its up to 8 channels on the device. Mute and solo channels as it plays and set the tempo.
Your choices are saved with the piece.

On the computer, `fm1c` turns a MIDI file into a piece and pushes it over USB. It can also have
[text2midi](https://github.com/cindulasai/text2midi_v2) compose one from a brief. Every reduction made to fit the
FM-1 is reported.

It is inspired by [SLOOP](https://github.com/isod89/sloop-fm1). It is a different instrument, a library player
rather than a live groovebox, and is built on the same community platform (Dinghy / Felucca).

> **Status:** developed and tested in the emulator [fm1-emu](https://github.com/joakimeriksson/fm1-emu)
> (including its on-chip timing mode). It has **not yet been run on a real FM-1**. Installing a community
> firmware on hardware is at your own risk.

## Quick start (in the emulator)

From the `app` folder of this repository (macOS on Apple silicon, or Linux; Python 3.11+):

```sh
python3 -m venv ../.venv && ../.venv/bin/pip install -q -r tools/requirements.txt
```

Then:

```sh
sh tools/get_emu.sh                                         # fm1-emu 0.5.1, checksum-verified, into ../vendor
gh run download -n composer-fwsc -D build                   # the firmware CI built (needs the GitHub CLI)
./fm1c emu                                                  # COMPOSER in fm1-emu, with USB
./fm1c convert song.mid -o song.f1p                         # in a second terminal
./fm1c push song.f1p                                        # "pushed 1, verified 1"
```

`fm1c emu` keeps the emulated FM-1's library in `~/.fm1c/emu-flash.bin`, so it is still there next time
(`--flash FILE` picks another).

## Playing

| Screen | Control | Does |
| --- | --- | --- |
| LIBRARY | SELECT | moves through the pieces, newest first |
| | PLAY | opens the piece and plays it |
| PLAYER | PLAY | stops / plays (from the start) |
| | KNOB1 (or SELECT) | picks a channel |
| | PRESETS | changes that channel's sound, heard at once |
| | KNOB2 | tempo, 30 to 300 BPM |
| | white keys F3 to F4 (1 to 8) | mute channel 1 to 8 |
| | hold SEL + white key | solo |
| | SAVE | keeps the sounds, mutes and tempo for this piece (when stopped; while playing it saves at the stop) |
| | HOME | stops and goes back to the library |
| | EDIT | opens the selected channel in EDIT |
| EDIT | SELECT | moves the cursor one grid step |
| | a note key | adds that note at the cursor (you hear it), or removes it if it is already there |
| | KNOB1 | grid: 1/32, 1/16, 1/8, 1/4 |
| | KNOB2 / KNOB3 | length / velocity of the note at the cursor (and of the next new one) |
| | OCT- / OCT+ | the keys an octave down / up (the roll follows) |
| | FX | undo (the last 64 edits) |
| | PLAY | plays from the cursor's bar / stops; edits wait until it is stopped |
| | SAVE | saves the edited piece in place of the old one, with your sounds, mutes and tempo |
| | HOME | back to PLAYER; leaving the piece with unsaved edits asks for HOME twice |

EDIT shows one channel as a piano roll: 16 grid steps across, the pitches of the 27 note keys down, the cursor's
step lit, notes in amber (a note begun before the page in grey). `*` after the channel's name means unsaved edits.

The player shows each channel's role (LEAD, BASS, DRUMS…), its sound and M / S, with the position as
bar:beat. There are 40 sounds from six Felucca engines plus COMPOSER's own drum kits. Each role starts with a
fitting one.

## fm1c

```
fm1c convert FILE.mid [-o OUT.f1p] [--name NAME] [--poly 1..16] [--force]
fm1c compose BRIEF.json [--seed N] [-o OUT.f1p] [--t2m PATH]     text2midi's t2m-cli, unchanged
fm1c push FILES... [--no-wait]                                     each piece read back to verify
fm1c list | delete ID | pull ID [-o OUT]
fm1c emu [--fwsc FILE] [--flash FILE]
```

By default fm1c finds the FM-1 by its USB name ("Felucca", as with every Felucca-based firmware), else
"FM-1". `--port PART-OF-NAME` or `$FM1C_PORT` choose another. Exit codes: 0 done, 1 the FM-1 refused (the
reason is printed), 2 a problem with the command, a file or the connection.

If you push while a piece is playing, the music keeps going. The new piece waits in memory and is saved when
playback stops. `fm1c` says "staged" and then "saved" (or exits at once with `--no-wait`).

## Limits, measured

- A piece: up to 8 channels, 32 KB (about 5,000 notes), 96 ticks per quarter. Longer MIDI is cut, and fm1c
  reports it.
- The library: 292 KB of the FM-1's flash, every piece whole-sector (4 KB) aligned. A power cut during a
  push or a save never leaves a broken piece: you have the library as it was before, or with the new piece.
  This is tested at every single flash write in the emulator.
- 16 voices are shared by the 8 channels, and a new note takes the oldest released voice first. The densest
  piece the format allows plays without an audio dropout in fm1-emu's on-chip timing mode (`FM1_STRICT`).
  That is a measurement in the emulator, not yet on hardware.

## Building and checking

CI (GitHub Actions) builds the `.fwsc` with JieLi's toolchain (`sh build.sh`, Linux x86-64). Everything else runs
on your computer:

```sh
sh check.sh
```

The gate runs, in order:
- C unit tests under the sanitizers.
- pytest: unit, property and hostile tests, plus end-to-end tests through host mode (the firmware compiled
  for this computer, driven by scripted keys and SysEx, checked by the screen text and the WAV it makes).
- The CI-built firmware in fm1-emu (`tests/emu_e2e.sh`): screens pixel-identical to host mode, pushes over
  emulated USB, USB audio, a power cut at every flash write, and on-chip timing. Without the emulator or a
  `build/composer.fwsc`, this step prints one `emu e2e: SKIP` line saying which is missing.

## Credits and licence

GPL-3.0-only, see `../LICENSING.md`. COMPOSER is built on these:
- Dinghy (zvenson)
- Felucca (Leo Kuroshita, Hügelton Instruments)
- SLOOP (isod89)
- Felucca-Engines (MIT)
- fm1-emu (Joakim Eriksson, MIT), used for testing

"M-VAVE" and "FM-1" belong to their owners; COMPOSER is not made or endorsed by them.
