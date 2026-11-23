# Files copied from SLOOP

SLOOP 2.8.5, [isod89/sloop-fm1](https://github.com/isod89/sloop-fm1) at commit `bac9c05ea7627449405aa10e48da21d75b6c8c56`,
GPL-3.0-only unless a row says otherwise (SPEC.md decision 29). SLOOP's logo and Hügelton's artwork are not copied.
One row per file: COMPOSER's path, SLOOP's, the commit, the upstream file's git blob id (`git hash-object`; `tools/tests/test_fm6.py`
checks them), its licence, and whether COMPOSER changed it (each change is marked `COMPOSER:` in the file). COMPOSER's
own glue: `firmware/src/sloop_glue.c` (the engine pool, what SLOOP's `core.h` gives eng_fm6.c, `ENG_FM6`),
`tools/gen_fm6_tables.py`, the end of `firmware/src/drums.c`, and synth.c.

| COMPOSER path | upstream path | commit | git blob | licence | changed |
| --- | --- | --- | --- | --- | --- |
| app/engines/LICENSE-Apache-2.0-msfa.txt | LICENSES/Apache-2.0-msfa.txt | bac9c05 | 75b5bab21f7b4e150b4be5ca224663af08101483 | Apache-2.0 (text) | no |
| app/engines/eng_fm6.c | firmware/src/eng_fm6.c | bac9c05 | c4fb384a4c70d9148c0e47edc319d7a4bfebb83e | GPL-3.0-only | no |
| app/engines/fm6_bank.c | firmware/src/fm6_bank.c | bac9c05 | 7b7df320d2bf9b0aca65099f97996ed898055ff5 | GPL-3.0-only | no (not built: SLOOP's flash bank) |
| app/engines/fm6_core.c | firmware/src/fm6_core.c | bac9c05 | e54f49518a1cd0405a6e496b1bb6df97e55230ea | Apache-2.0 | no |
| app/firmware/src/drum_synth.c | firmware/src/drum_synth.c | bac9c05 | 303d4e0cf47499a1da66c704eb06e604ab235c7b | GPL-3.0-only | yes: 2 lines, a left shift of a negative value made defined (`shl`, the same bits) |
| app/tools/drumkit_levels.json | tools/drumkit_levels.json | bac9c05 | 6a12651dc9c1f240f87a1309ab37391729769ae5 | GPL-3.0-only (SLOOP `tools/`) | no |
| app/tools/gen_drumkits.py | tools/gen_drumkits.py | bac9c05 | 7d08bc87f100e5fbc88a8dc3b131d9cc5103bdcb | GPL-3.0-only | no |
| app/tools/gen_fm6_patches.py | tools/gen_fm6_patches.py | bac9c05 | 1a5f55e39ffb3c0758dfadfe5a530d439b5fb259 | GPL-3.0-only | no |
| app/tools/sloop_gen_tables.py | tools/gen_tables.py | bac9c05 | c232b2bc7590eff4bca4b8cb4319684788c4e6b4 | GPL-3.0-only | no (renamed) |

Functions copied into a COMPOSER file (US-18; the file is COMPOSER's, so no one blob for it):

| COMPOSER path | upstream functions | SLOOP blobs @ bac9c05 | licence | changed |
| --- | --- | --- | --- | --- |
| app/firmware/src/fx.c | firmware/src/fx.c (track_dist, lim_block, dc_block, fx_buses, djf_block, djf_run, tflt_run) + firmware/src/dsp.c (MULQ12R) | 75fc88c4ff84ad487c9f46e8b8f03be8a5f09ab6 + 7c6a2d27f416ff542d3c98825d1acde8d59c6c22 | GPL-3.0-only | functions copied; signed left shifts written as multiplications; SLOOP's song settings fixed at its defaults; pool sections (marked COMPOSER) |

Generated, not copied: `app/firmware/src/felucca_drumkits.h` (`tools/gen_drumkits.py`, unchanged, writes it);
`app/firmware/src/drum_tables.h` (`tools/gen_drum_tables.py`, ours: SLOOP's `tools/gen_tables.py` DECAY_K formula,
blob `c232b2bc7590eff4bca4b8cb4319684788c4e6b4`, line for line). Our glue: the end of `app/firmware/src/drums.c`
(US-21).
Generated from them (committed, `tools/tests/test_fm6.py` regenerates and compares):
`app/engines/felucca_fm6.h` (`tools/gen_fm6_patches.py`), `app/engines/fm6_tables.h` (`tools/gen_fm6_tables.py`, the
FM6 part of SLOOP's `gen_tables.py`).
`fm6_core.c` is a port of msfa (Dexed's FM core): Copyright 2012 Google Inc., 2016-2025 Pascal Gauthier, C port
2026 Leo Kuroshita; Apache License 2.0 (`app/engines/LICENSE-Apache-2.0-msfa.txt`). The GPL-3.0-only files are
Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments. SLOOP's assets (logo, Hügelton art) are not
copied.
