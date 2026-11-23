# Files copied from SLOOP

SLOOP 2.8.5, [isod89/sloop-fm1](https://github.com/isod89/sloop-fm1) at commit `bac9c05ea7627449405aa10e48da21d75b6c8c56`
(SPEC.md decision 29). Each file is byte for byte the upstream file (its git blob id below; `tools/tests/test_fm6.py`
checks the ids). COMPOSER's adaptations are in its own files: `firmware/src/sloop_glue.c` (the engine pool, what
SLOOP's `core.h` gives eng_fm6.c, COMPOSER's `ENG_FM6`), `tools/gen_fm6_tables.py`, and synth.c.

| COMPOSER path | upstream path | commit | git blob | licence | changed |
| --- | --- | --- | --- | --- | --- |
| app/engines/fm6_core.c | firmware/src/fm6_core.c | bac9c05 | e54f49518a1cd0405a6e496b1bb6df97e55230ea | Apache-2.0 | no |
| app/engines/eng_fm6.c | firmware/src/eng_fm6.c | bac9c05 | c4fb384a4c70d9148c0e47edc319d7a4bfebb83e | GPL-3.0-only | no |
| app/engines/fm6_bank.c | firmware/src/fm6_bank.c | bac9c05 | 7b7df320d2bf9b0aca65099f97996ed898055ff5 | GPL-3.0-only | no (not built: SLOOP's flash bank) |
| app/engines/LICENSE-Apache-2.0-msfa.txt | LICENSES/Apache-2.0-msfa.txt | bac9c05 | 75b5bab21f7b4e150b4be5ca224663af08101483 | Apache-2.0 (text) | no |
| app/tools/gen_fm6_patches.py | tools/gen_fm6_patches.py | bac9c05 | 1a5f55e39ffb3c0758dfadfe5a530d439b5fb259 | GPL-3.0-only | no |
| app/tools/sloop_gen_tables.py | tools/gen_tables.py | bac9c05 | c232b2bc7590eff4bca4b8cb4319684788c4e6b4 | GPL-3.0-only | no (renamed) |

Generated from them (committed, `tools/tests/test_fm6.py` regenerates and compares):
`app/engines/felucca_fm6.h` (`tools/gen_fm6_patches.py`), `app/engines/fm6_tables.h` (`tools/gen_fm6_tables.py`, the
FM6 part of SLOOP's `gen_tables.py`).

`fm6_core.c` is a port of msfa (Dexed's FM core): Copyright 2012 Google Inc., 2016-2025 Pascal Gauthier, C port
2026 Leo Kuroshita; Apache License 2.0 (`app/engines/LICENSE-Apache-2.0-msfa.txt`). The GPL-3.0-only files are
Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments. SLOOP's assets (logo, Hügelton art) are not
copied.
