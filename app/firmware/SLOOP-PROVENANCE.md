# Code copied from SLOOP

From SLOOP 2.8.5, [isod89/sloop-fm1](https://github.com/isod89/sloop-fm1) at commit `bac9c05`, GPL-3.0-only
(SPEC decision 29). One line per file: our path, SLOOP's path, the commit, the upstream file's git blob id
(`git hash-object`), its licence, and whether we changed it (each change is a line marked `COMPOSER:`).

| Our path | SLOOP path | Commit | Blob | Licence | Changed |
| --- | --- | --- | --- | --- | --- |
| `app/firmware/src/drum_synth.c` | `firmware/src/drum_synth.c` | bac9c05 | `303d4e0cf47499a1da66c704eb06e604ab235c7b` | GPL-3.0-only | yes: 2 lines, a left shift of a negative value made defined (`shl`, the same bits) |
| `app/tools/gen_drumkits.py` | `tools/gen_drumkits.py` | bac9c05 | `7d08bc87f100e5fbc88a8dc3b131d9cc5103bdcb` | GPL-3.0-only | no |
| `app/tools/drumkit_levels.json` | `tools/drumkit_levels.json` | bac9c05 | `6a12651dc9c1f240f87a1309ab37391729769ae5` | GPL-3.0-only (SLOOP `tools/`) | no |

Generated, not copied: `app/firmware/src/felucca_drumkits.h` (`tools/gen_drumkits.py`, unchanged, writes it);
`app/firmware/src/drum_tables.h` (`tools/gen_drum_tables.py`, ours: SLOOP's `tools/gen_tables.py` DECAY_K formula,
blob `c232b2bc7590eff4bca4b8cb4319684788c4e6b4`, line for line). Our glue: the end of `app/firmware/src/drums.c`
(US-21).
