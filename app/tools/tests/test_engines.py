"""US-20: SLOOP's PHYS, DIGITAL and VOICE engines. The copies are SLOOP's files unchanged (their blob ids are checked
for every row of SLOOP-PROVENANCE.md by test_fm6.py::test_19_2_copies_are_sloops_files_unchanged), the tables they read
are SLOOP's generator's, and the PLAYER reaches their sounds (20.4). The C side (20.1, 20.2, 20.3, 20.5, 20.6) is
tests/synth_test.c; the FM1_STRICT run of 20.6 is test_emu.py."""
import re, subprocess, sys
from pathlib import Path

from conftest import APP, run_host, screen, wav_frames
from test_fm6 import provenance, push                     # (it puts tools/ on the path: fm1c)
from test_ui import catalog
from fm1c.piece import Piece, Channel, Note, encode  # noqa: E402

COPIES = {"app/engines/eng_phys.c": "GPL-3.0-only", "app/engines/phys_dsp.c": "MIT", "app/engines/phys_symp.c": "MIT",
          "app/engines/eng_digital.c": "GPL-3.0-only", "app/engines/eng_formant.c": "GPL-3.0-only"}


def test_20_1_copies_listed_with_their_licences():
    rows = {c: (u, ch) for c, u, _, ch in provenance()}
    text = (APP / "firmware/SLOOP-PROVENANCE.md").read_text()
    for comp, lic in COPIES.items():
        assert comp in rows and rows[comp][0] == "firmware/src/" + Path(comp).name and rows[comp][1] == "no", comp
        assert re.search(r"^\| " + re.escape(comp) + r" \|[^\n]*\| " + re.escape(lic) + r" \| no \|$", text, re.M), comp
        assert (APP.parent / comp).read_text().splitlines()[0] == f"/* SPDX-License-Identifier: {lic}", comp


def test_20_1_generated_sloop_tables_in_sync(tmp_path):
    out = tmp_path / "sloop_tables.h"
    r = subprocess.run([sys.executable, str(APP / "tools/gen_sloop_tables.py"), str(out)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    assert out.read_text() == (APP / "engines/sloop_tables.h").read_text()
    assert "ENV_LIN[128]" in out.read_text() and "ENV_EXP[128]" in out.read_text()


BTN_PLAY, ENC_PRESETS, SAW_LEAD, KITS = 12, 6, 3, 32


def test_20_4_presets_reach_phys_digital_voice_strip_names_wav_sounds(tmp_path):
    names = catalog(tmp_path)
    firsts = {"PHYS": 80, "DIGITAL": 80 + 26, "VOICE": 80 + 26 + 22,         # (v5 ids: after FM6's 72..79)
              "SAMPLE": 80 + 26 + 22 + 7}                                     # (v11, US-35: PIANO STRINGS HORNS)
    assert len(names) == 80 + 26 + 22 + 7 + 3
    assert [names[i] for i in firsts.values()] == ["BELL TREE", "RHODES", "CHOIR AAH", "PIANO"]
    notes = [Note(t * 96, 60 + (t % 4) * 2, 90, 110) for t in range(64)]
    data = encode(Piece("eng", 1200, 64 * 96, [Channel("lead", notes)]))
    lines, t = push(100, data)
    turns, at, here = [], t + 300, SAW_LEAD
    for eng, sid in firsts.items():                                           # a lead turns past the 32 kits
        turns.append((at, eng, sid, f"{at} enc {ENC_PRESETS} {sid - here - (KITS if here < 40 <= sid else 0)}"))
        here, at = sid, at + 1000
    p = tmp_path / "engines.txt"
    p.write_text("\n".join(lines + [f"{t + 50} btn {BTN_PLAY}"] + [c for *_, c in turns] + [f"{at + 200} end"]) + "\n")
    log, wav = run_host(p)
    s = screen(log)
    f = wav_frames(wav)
    for ms, eng, sid, _ in turns:
        assert (64, names[sid]) in [(y, txt) for m, y, txt in s if ms <= m < ms + 1000], f"the strip shows {eng}'s {names[sid]}"
        x = f[int((ms + 300) / 1000 * 44100) * 2:int((ms + 900) / 1000 * 44100) * 2]
        assert max(abs(v) for v in x) > 1000 and sum(v * v for v in x) / len(x) > 1e4, f"{eng} {names[sid]} is heard"
