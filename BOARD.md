# Board — fm1-composer
| ID | Story | Inc | Status | Tries |
|----|-------|-----|--------|-------|
| US-1 | Walking skeleton: COMPOSER builds, host mode runs, gate exists | 1 | done | 0 |
| US-2 | Piece format .f1p | 1 | done | 0 |
| US-3 | fm1c convert / compose | 1 | done | 0 |
| US-4 | Synth: 8 channels, voice pool, sounds | 1 | done | 0 |
| US-5 | Player | 1 | done | 0 |
| US-6 | Library in flash | 1 | done | 0 |
| US-7 | Transfer | 1 | done | 0 |
| US-8 | UI | 1 | done-self | 0 |
| US-9 | Emulator end to end | 1 | done | 0 |
| US-10 | Note edit core | 1 | done | 1 |
| US-11 | EDIT screen | 1 | done | 1 |
| US-12 | Pieces over the MIDI-in jack | 1 | done | 0 |
| US-13 | Studio engine (WASM) | 1 | done | 0 |
| US-14 | Studio page: idea → AI → piece | 1 | done-self | 0 |
| US-15 | Side by side: the emulator on the page | 1 | done-self | 0 |
| US-16 | Left: text2midi's own interface | 1 | done | 0 |
| US-17 | Right: fm1-emu's own page + MIDI cable | 1 | done | 0 |
| US-18 | Effects from SLOOP | 1 | done | 0 |
| US-19 | SLOOP's FM6 engine + DX7 patches | 1 | done | 2 |
| US-20 | PHYS, DIGITAL, VOICE engines | 1 | done | 0 |
| US-21 | 32 synthesized drum kits | 1 | done | 0 |
| US-22 | text2midi's instruments, roles, mix | 1 | done | 0 |
| US-23 | Sections: launch, loop, queue | 1 | done | 0 |
| US-24 | Live keyboard in key, auto-chords, MIDI IN | 1 | done | 1 |
| US-25 | Step sequencer on 8 tracks | 1 | done | 0 |
| US-26 | Record live play into a part | 1 | done | 0 |
| US-27 | Studio as a static site | 1 | done | 0 |
| US-28 | Pages from CI, deploy only when public | 1 | done | 2 |
| US-29 | Send survives a stalled emulator | 1 | proven | 0 |

## Impediments
- US-14, US-15: done-self: v3 Adversary FAILs were on criteria replaced by v4 (US-16, US-17, done); their remaining criteria (14.4 key file, 14.5 server, 15.5 emulator pin) were not re-reviewed fresh
