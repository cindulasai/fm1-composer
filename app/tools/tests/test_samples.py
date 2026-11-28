"""US-35: piano, strings and horns from samples on the FM-1: SLOOP's SAMPLE engine (engines/eng_sample.c) playing the
sets tools/gen_samples.py makes from SLOOP's CC0 WAVs (engines/felucca_samples.h). Host WAVs through the firmware's own
player and synth (tests/render_piece.c); the C side (zones, loops, hostile calls) is tests/synth_test.c 35.1 / 35.4;
the strict-timing emu run is test_emu.py 35.3."""
import ast, hashlib, math, os, re, subprocess, sys, wave, zlib
from pathlib import Path
import mido
import pytest
from conftest import APP, run_host
from midifix import make_midi

sys.path.insert(0, str(APP / "tools"))
from fm1c import sounds  # noqa: E402
from fm1c.fit import fit  # noqa: E402
from fm1c.piece import Channel, Note, Piece, encode  # noqa: E402
from sampleio import read_any_wav, yin  # noqa: E402

CC0 = APP / "assets/samples-cc0"
HEADER = APP / "engines/felucca_samples.h"
SR = 44100
QUARTER_FS = 8192                                            # a click: a step over 1/4 full scale
SETS = {"PIANO": ("PIANO", 135), "STRGS": ("STRINGS", 136), "HORNS": ("HORNS", 137)}   # set dir -> (sound, id)


def zones():
    """SMP_ZONES of the generated header: [(set dir, root note, lo, hi)] in set order"""
    text = HEADER.read_text()
    rows = re.findall(r"^    \{(\d+), (\d+), (\d+), (\d+), (\d+), (-?\d+), (-?\d+), (\d+), (\d+), (\d+), (\d)\},$", text, re.M)
    sets = re.findall(r'^    \{"(\w+)", (\d+), (\d+)\},$', text, re.M)
    out = []
    for name, z0, nz in sets:
        for r in rows[int(z0):int(z0) + int(nz)]:
            out.append((name, int(r[5]) // 16, int(r[8]), int(r[9])))
    return out


def source(setname, root):
    """the CC0 WAV of a zone"""
    return next((CC0 / setname).glob(f"*_m{root}.wav"))


def render(tmp_path, notes_by_sound, secs, name="s"):
    """one channel per (sound id, [Note]) through render_piece: the left channel's samples"""
    exe = tmp_path / "render_piece"
    if not exe.exists():
        subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                        str(APP / "tests/render_piece.c"), "-lm"], check=True)
    chans = [Channel("chords", notes, sound=snd, vol=127) for snd, notes in notes_by_sound]
    f1p, out = tmp_path / f"{name}.f1p", tmp_path / f"{name}.wav"
    length = max(n.tick + n.dur for _, ns in notes_by_sound for n in ns)
    f1p.write_bytes(encode(Piece(name, 1200, -(-length // 384) * 384, chans)))
    r = subprocess.run([str(exe), str(f1p), str(out), str(secs)], capture_output=True, text=True, check=True)
    with wave.open(str(out)) as w:
        data = w.readframes(w.getnframes())
    left = [int.from_bytes(data[i:i + 2], "little", signed=True) for i in range(0, len(data), 4)]
    return left, r.stdout


def tick_s(t):
    return t * 0.5 / 96                                       # 120 BPM, 96 PPQ


def hz(x, sr, at, secs=0.08):
    """the pitch at `at` seconds: YIN (sampleio, as SLOOP's tools measure) over `secs` of sound. The sets are blends
    (HORNS a trumpet over a trombone an octave down, STRINGS violins over violas a fifth down): their period can be
    an octave under the named root, so the same span of sound is measured in the WAV and in the render"""
    a = int(at * sr)
    return yin(x[a:a + int(secs * sr)], sr, fmax=2000, fmin=50)


def cents(a, b, name):
    """a over b in cents; HORNS folded by whole octaves (its trumpet and trombone an octave apart, the preset's DRV and
    CUT tip YIN from one to the other: e.g. 97.65 Hz rendered for the WAV's 195.72, -3.7 cents from it)"""
    c = 1200 * math.log2(a / b)
    return c - 1200 * round(c / 1200) if name == "HORNS" and abs(round(c / 1200)) <= 1 else c


# ---- 35.1 ----
def test_35_1_a_note_at_a_zones_root_renders_its_source_pitch(tmp_path):
    zs = zones()
    assert [z[0] for z in zs] == ["PIANO"] * 4 + ["STRGS"] * 3 + ["HORNS"] * 4
    notes = {sid: [] for _, sid in SETS.values()}
    for k, (name, root, _, _) in enumerate(zs):                # one note a second, each zone's root, 0.6 s long
        notes[SETS[name][1]].append((k, root))
    chans = [(sid, [Note(k * 192, root, 115, 100) for k, root in ns]) for sid, ns in notes.items()]
    left, _ = render(tmp_path, chans, len(zs) + 1)
    worst = 0.0
    for k, (name, root, _, _) in enumerate(zs):
        sr, src = read_any_wav(source(name, root))
        want = hz(src, sr, 0.15)
        got = hz(left, SR, tick_s(k * 192) + 0.15)
        c = cents(got, want, name)
        worst = max(worst, abs(c))
        assert abs(c) <= 10, f"{name} m{root}: {got:.2f} Hz rendered, the WAV {want:.2f} Hz ({c:+.1f} cents)"
    print(f"35.1 11 zones at their roots: worst {worst:.1f} cents from the source WAV")


def test_35_1_a_note_between_zones_is_pitched_from_the_nearest(tmp_path):
    zs = zones()
    cases = []                                               # (set, note, nearest root): one above, one below each root
    for (n1, r1, _, _), (n2, r2, _, _) in zip(zs, zs[1:]):
        if n1 == n2:
            cases += [(n1, r1 + 2, r1), (n1, r2 - 2, r2)]
    by = {}
    for k, (name, note, root) in enumerate(cases):
        by.setdefault(SETS[name][1], []).append(Note(k * 192, note, 115, 100))
    left, _ = render(tmp_path, list(by.items()), len(cases) + 1)
    worst = 0.0
    for k, (name, note, root) in enumerate(cases):
        sr, src = read_any_wav(source(name, root))
        r = 2 ** ((root - note) / 12)                       # (the render plays the recording r times as long)
        want = hz(src, sr, 0.15) / r
        got = hz(left, SR, tick_s(k * 192) + 0.15 * r, 0.08 * r)
        c = cents(got, want, name)
        worst = max(worst, abs(c))
        assert abs(c) <= 10, f"{name} note {note} (from m{root}): {got:.2f} Hz, want {want:.2f} ({c:+.1f} cents)"
    print(f"35.1 {len(cases)} notes between zones, pitched from the nearest root: worst {worst:.1f} cents")


def test_35_1_held_strings_and_horns_loop_past_their_end_without_a_click(tmp_path):
    """each STRINGS / HORNS zone's root held 2.5 s (its recording is 0.2 s: ten loops and more): no step over 1/4 full
    scale while it is held, still heard at the end"""
    zs = [z for z in zones() if z[0] != "PIANO"]
    by = {}
    for k, (name, root, _, _) in enumerate(zs):
        by.setdefault(SETS[name][1], []).append(Note(k * 768, root, 480, 100))
    left, _ = render(tmp_path, list(by.items()), 4 * len(zs) + 1)
    worst = 0
    for k, (name, root, _, _) in enumerate(zs):
        a, b = int((tick_s(k * 768) + 0.05) * SR), int((tick_s(k * 768) + 2.45) * SR)
        step = max(abs(left[i + 1] - left[i]) for i in range(a, b))
        tail = left[b - SR // 2:b]
        rms = (sum(v * v for v in tail) / len(tail)) ** 0.5
        worst = max(worst, step)
        assert step < QUARTER_FS, f"{name} m{root}: a step of {step} while held"
        assert rms > 300, f"{name} m{root}: not heard after 2 s held (rms {rms:.0f})"
    print(f"35.1 STRINGS / HORNS held 2.5 s: largest step {worst} (click {QUARTER_FS})")


def test_35_1_piano_decays_and_ends(tmp_path):
    left, out = render(tmp_path, [(135, [Note(0, 60, 96 * 2 * 10, 110)])], 12)     # C4 held 10 s
    rms = lambda a, b: (sum(v * v for v in left[int(a * SR):int(b * SR)]) / ((b - a) * SR)) ** 0.5   # noqa: E731
    early, mid, late = rms(0.05, 0.3), rms(1.5, 2.0), rms(8.0, 9.5)
    assert early > 1000 and mid * 3 < early and late < 30, (early, mid, late)
    assert "voices dropped 0" in out


# ---- 35.2 ----
V10_GM = (8, 8, 8, 8, 72, 72, 11, 11, 73, 73, 24, 77, 77, 77, 19, 11, 78, 78, 26, 16, 28, 28, 13, 28, 79, 79, 11, 11, 11,
          30, 30, 24, 74, 21, 21, 74, 5, 5, 10, 32, 17, 17, 17, 17, 6, 11, 11, 74, 14, 14, 6, 6, 25, 25, 25, 15, 75, 75,
          75, 12, 75, 75, 12, 15, 13, 13, 13, 13, 13, 13, 13, 29, 29, 29, 29, 29, 29, 29, 29, 29, 31, 3, 29, 34, 30, 25,
          23, 7, 76, 4, 6, 25, 17, 20, 76, 18, 37, 4, 73, 4, 19, 38, 24, 38, 20, 11, 11, 79, 77, 13, 17, 13, 24, 27, 77,
          27, 74, 74, 39, 36, 39, 36, 37, 38, 38, 36, 37, 39)   # (sounds.py's GM table as v10 shipped it)


def test_35_2_the_three_sounds_in_the_catalog_and_fm1cs_table(tmp_path):
    from test_ui import catalog
    names = catalog(tmp_path)
    assert names[135:] == ["PIANO", "STRINGS", "HORNS"] == list(sounds.NAMES[135:])
    assert (sounds.PIANO, sounds.STRINGS, sounds.HORNS) == (135, 136, 137)
    rs = (APP / "studio/engine/studio/src/sounds.rs").read_text()          # (decision 34: one table, sounds.rs a copy)
    assert re.findall(r'"([^"]+)"', re.search(r"NAMES: \[&str; 138\] = \[(.*?)\];", rs, re.S)[1]) == list(sounds.NAMES)
    gm = [int(v) for v in re.findall(r"\d+", re.search(r"const GM: \[u8; 128\] = \[(.*?)\];", rs, re.S)[1])]
    assert gm == list(sounds.GM)


def test_35_2_gm_pianos_strings_and_brass_play_them_every_other_program_as_in_v10():
    want = {**{p: 135 for p in range(0, 8)}, **{p: 136 for p in range(40, 52)}, **{p: 137 for p in range(56, 64)}}
    for p in range(128):
        assert sounds.sound("Track", p, "chords") == want.get(p, V10_GM[p]), p
    assert sounds.sound("chords – keys.piano", None, "chords") == 135
    assert sounds.sound("Piano", None, "chords") == 135
    assert sounds.sound("pad – strings.ensemble", None, "pad") == 136
    assert sounds.sound("Strings", None, "pad") == 136
    assert sounds.sound("brass", 61, "lead") == 137                        # (brass section)
    assert sounds.sound("drums", 0, "drums") == 0 and sounds.sound("drums", 48, "drums") == 0     # kits stay kits


def test_35_2_fit_picks_them_and_never_counts_them_as_fm6(tmp_path):
    """text2midi's piano, string and brass parts get PIANO, STRINGS, HORNS; four sampled parts beside three FM6 ones:
    no FM6 swap (only FM6 takes one of the 3 engine blocks)"""
    notes = [(480 * k, 60, 240, 90) for k in range(4)]
    pc = lambda ch, p: [mido.Message("program_change", channel=ch, program=p)]     # noqa: E731
    parts = [("chords – keys.piano", 0, notes), ("pad – strings.ensemble", 1, notes), ("lead", 2, notes, pc(2, 56)),
             ("counter", 3, notes, pc(3, 61)), ("bass – bass.upright", 4, notes), ("arp – vibes.vibraphone", 5, notes),
             ("Track 7", 6, notes, pc(6, 24))]
    piece, rep = fit(str(make_midi(tmp_path / "s.mid", parts)))
    got = sorted((c.role, sounds.NAMES[c.sound]) for c in piece.channels)
    assert got == sorted([("chords", "PIANO"), ("pad", "STRINGS"), ("lead", "HORNS"), ("counter", "HORNS"),
                          ("bass", "ROUND BASS"), ("arp", "WOOD BARS"), ("chords", "NYLON PICK")]), got
    assert not [ln for ln in rep.lines if "FM6" in ln], rep.lines


def test_35_2_the_sampled_names_on_the_player_screen(tmp_path):
    from test_ui import BTN, at, push, script
    notes = [Note(t * 96, 60 + t % 3, 80, 100) for t in range(16)]
    data = encode(Piece("sampled", 1200, 4 * 384, [Channel(r, notes, sound=s) for r, s in
                                                     (("chords", 135), ("pad", 136), ("lead", 137))]))
    lines, t = push(100, data)
    log, wav = run_host(script(tmp_path, "us35_names", lines + [f"{t + 50} btn {BTN['PLAY']}"], t + 1500))
    s = at(log, t + 50)
    for c, name in enumerate(["PIANO", "STRINGS", "HORNS"]):
        assert (64 + c * 19, name) in s, (name, sorted(s))


# a piece of v10's sounds only (lead, bass, drums, pad, guitar: none of the three), fitted and rendered 4 s: the CRC of
# its WAV as v10's firmware code rendered it (recorded from the tree before US-35)
V10_PIECE_CRC = 0x86BE318F


def test_35_2_a_piece_without_those_instruments_renders_as_in_v10(tmp_path):
    notes = [(240 * k, 48 + (k * 5) % 24, 200, 70 + k % 40) for k in range(32)]
    kicks = [(240 * k, 36 + (k % 2) * 2, 60, 110) for k in range(32)]
    pc = lambda ch, p: [mido.Message("program_change", channel=ch, program=p)]     # noqa: E731
    parts = [("lead", 0, notes, pc(0, 80)), ("bass", 1, [(t, p - 12, d, v) for t, p, d, v in notes], pc(1, 33)),
             ("drums", 9, kicks), ("pad", 2, notes[::4], pc(2, 89)), ("chords", 3, notes[::2], pc(3, 24))]
    piece, _ = fit(str(make_midi(tmp_path / "v10.mid", parts)))
    assert not {c.sound for c in piece.channels} & {135, 136, 137}
    exe, f1p, out = tmp_path / "render_piece", tmp_path / "v10.f1p", tmp_path / "v10.wav"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                    str(APP / "tests/render_piece.c"), "-lm"], check=True)
    f1p.write_bytes(encode(piece))
    subprocess.run([str(exe), str(f1p), str(out), "4"], capture_output=True, text=True, check=True)
    crc = zlib.crc32(out.read_bytes())
    print(f"35.2 v10 piece render {crc:08X}")
    assert crc == V10_PIECE_CRC, f"{crc:08X}"


# ---- 35.3 ----
def test_35_3_flash_with_the_samples_leaves_200_kb_free():
    """decision 27 / 36: measured from the CI build (tools/build.py writes build/composer.bin, the app image it
    packages, and build/composer.dis; the gate runs after it in CI): the image within the 568 KB code space with at
    least 200 KB free, SMP_DATA's size from the disassembly's symbols. Locally, without that build: a skip"""
    img, dis = APP / "build/composer.bin", APP / "build/composer.dis"
    if not img.exists() or not dis.exists():
        pytest.skip("needs the CI build's build/composer.bin + build/composer.dis (JieLi toolchain, Linux CI only)")
    sys.path.insert(0, str(APP / "tools"))
    import fm1pkg_make
    slot, size = fm1pkg_make.APP_SLOT, img.stat().st_size
    labels = [(int(a, 16), n) for a, n in re.findall(r"^([0-9a-f]{8}) <([^>]+)>:$", dis.read_text(), re.M)]
    at = [k for k, (_, n) in enumerate(labels) if n == "SMP_DATA"]
    if not at:
        pytest.skip("the CI build in build/ predates the samples (no SMP_DATA in composer.dis)")
    smp = labels[at[0] + 1][0] - labels[at[0]][0] if at[0] + 1 < len(labels) else 0
    free = slot - size
    print(f"35.3 flash: image {size} B of {slot} B, {free / 1024:.1f} KB free; SMP_DATA {smp} B "
          f"(felucca_samples.h: {re.search(r'SMP_DATA\[(\d+)\]', HEADER.read_text())[1]} B)")
    assert smp >= 59000 and free >= 200 * 1024, (size, free, smp)


# ---- 35.5 ----
def test_35_5_the_header_is_the_generators_output():
    if sys.version_info < (3, 12):
        pytest.skip("gen_samples.py's float sums give its bytes on Python >= 3.12 (sum() compensated)")
    import gen_samples
    text, summary = gen_samples.build()
    assert text == HEADER.read_text(), "engines/felucca_samples.h is stale: python3 tools/gen_samples.py " \
                                       "engines/felucca_samples.h"
    print(f"35.5 {summary}")


def test_35_5_every_sample_byte_from_sloops_cc0_sets(monkeypatch):
    """the generator reads only the CC0 WAVs in assets/samples-cc0 (every one of them, each with its ATTRIBUTION line
    naming VSCO-2 CE or VCSL), never gen_waves.py's output (the Hügelton Sample Pack, all rights reserved)"""
    import gen_samples
    read = []
    real = gen_samples.read_any_wav
    monkeypatch.setattr(gen_samples, "read_any_wav", lambda p: read.append(Path(p)) or real(p))
    monkeypatch.setattr(gen_samples, "_wavs", {})
    gen_samples.build()
    wavs = sorted(CC0.glob("*/*.wav"))
    assert sorted(set(read)) == wavs and len(wavs) == 11
    assert {p.parent.name for p in wavs} == {"PIANO", "STRGS", "HORNS"}
    assert sorted(p.name for p in CC0.iterdir()) == ["ATTRIBUTION.txt", "HORNS", "PIANO", "STRGS"]
    attr = (CC0 / "ATTRIBUTION.txt").read_text()
    assert "CC0 1.0" in attr
    for p in wavs:
        line = re.search(rf"^{p.parent.name}/{p.name}  <-  (.*)$", attr, re.M)
        assert line and re.search(r"sgossner/(VSCO-2-CE|VCSL)", line[1]), p
    code = ast.parse((APP / "tools/gen_samples.py").read_text())
    strings = [n.value for n in ast.walk(code) if isinstance(n, ast.Constant) and isinstance(n.value, str)]
    calls = [n for n in ast.walk(code) if isinstance(n, ast.Call)]
    assert not [s for s in strings[1:] if re.search(r"gen_waves|genwav|hugelton|hügelton", s, re.I)], "a Hügelton input"
    assert not [c for c in calls if getattr(c.func, "attr", "") in ("run", "call", "Popen", "system")], "no subprocess"
    assert {s for s in strings if s in ("assets", "samples-cc0")} == {"assets", "samples-cc0"}


def test_35_5_licensing_and_readme_credit_the_samples_and_sloops_engine():
    lic, readme = (APP.parent / "LICENSING.md").read_text(), (APP / "README.md").read_text()
    for doc, name in ((lic, "LICENSING.md"), (readme, "app/README.md")):
        for word in ("VSCO-2 CE", "VCSL", "CC0", "SLOOP", "eng_sample.c", "GPL-3.0"):
            assert word in doc, (name, word)
    assert "assets/samples-cc0" in lic and "Hügelton Sample Pack" in lic
    head = (APP / "engines/eng_sample.c").read_text().splitlines()[:2]
    assert head == ["/* SPDX-License-Identifier: GPL-3.0-only",
                    " * Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments */"]
