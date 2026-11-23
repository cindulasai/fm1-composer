"""US-19: SLOOP's FM6 engine. The copies are SLOOP's files unchanged (blob ids), SLOOP's own fm6_core.c renders what
COMPOSER's does (19.2), and the PLAYER reaches the FM6 sounds (19.4). The C side (19.1, 19.3, 19.5, 19.6) is
tests/synth_test.c."""
import hashlib, importlib.util, math, os, random, re, struct, subprocess, sys
from pathlib import Path

import pytest
from conftest import APP, run_host, screen, wav_frames

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, Channel, Note, encode  # noqa: E402

PROV = APP / "firmware/SLOOP-PROVENANCE.md"
SLOOP_COMMIT = "bac9c05ea7627449405aa10e48da21d75b6c8c56"


def blob(path):
    data = Path(path).read_bytes()
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def provenance():
    """[(composer path, upstream path, blob, changed)] from SLOOP-PROVENANCE.md"""
    rows = re.findall(r"^\| (app/\S+) \| (\S+) \| bac9c05 \| ([0-9a-f]{40}) \| [^|]+ \| ([^|]+) \|$", PROV.read_text(), re.M)
    return [(c, u, b, ch.strip()) for c, u, b, ch in rows]


def sloop_src():
    """SLOOP's source at the pinned commit: $SLOOP_SRC, else research/sloop-src of this checkout or of the main
    checkout of this worktree (research/ is not in git); None when there is none"""
    cands = [os.environ.get("SLOOP_SRC")] if os.environ.get("SLOOP_SRC") else []
    cands.append(APP.parent / "research/sloop-src")
    r = subprocess.run(["git", "-C", str(APP), "rev-parse", "--path-format=absolute", "--git-common-dir"],
                       capture_output=True, text=True)
    if r.returncode == 0:
        cands.append(Path(r.stdout.strip()).parent / "research/sloop-src")
    for c in cands:
        if c and (Path(c) / "firmware/src/fm6_core.c").is_file():
            return Path(c)
    return None


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


def test_19_2_copies_are_sloops_files_unchanged():
    rows = provenance()
    assert {c for c, *_ in rows} >= {"app/engines/fm6_core.c", "app/engines/eng_fm6.c", "app/engines/fm6_bank.c"}
    for comp, up, b, changed in rows:
        assert changed.startswith("no"), comp
        assert blob(APP.parent / comp) == b, f"{comp} is not SLOOP's {up} @ bac9c05"
    head = (APP / "engines/fm6_core.c").read_text().splitlines()[0]
    assert head.startswith("/* SPDX-License-Identifier: Apache-2.0"), "the licence header is kept"


def test_19_2_generated_fm6_headers_in_sync(tmp_path):
    for script, target in (("gen_fm6_patches.py", "engines/felucca_fm6.h"), ("gen_fm6_tables.py", "engines/fm6_tables.h")):
        out = tmp_path / Path(target).name
        r = subprocess.run([sys.executable, str(APP / "tools" / script), str(out)], capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        assert out.read_text() == (APP / target).read_text(), target


def voices():
    """155-byte voices: the init voice, the factory patches, and random voices inside every byte's range"""
    g = load("gen_fm6_patches", APP / "tools/gen_fm6_patches.py")
    vs = [g.INIT] + [v for _, v in g.PATCHES]
    opmax = [99] * 11 + [3, 3, 7, 3, 7, 99, 1, 31, 99, 14]
    vmax = [99] * 8 + [31, 7, 1, 99, 99, 99, 99, 1, 5, 7, 48]
    rnd = random.Random(19)
    for _ in range(24):
        v = [rnd.randint(0, opmax[i % 21]) for i in range(126)] + [rnd.randint(0, m) for m in vmax]
        vs.append(v + [rnd.randint(32, 126) for _ in range(10)])
    assert all(len(v) == 155 for v in vs)
    return vs, len(g.PATCHES) + 1


def build_ref(tmp_path, name, *flags):
    exe = tmp_path / name
    r = subprocess.run(["cc", "-O1", "-Wall", "-Werror", "-Wno-unused-function", "-o", str(exe), str(APP / "tests/fm6_ref.c"),
                        *flags], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return exe


def test_19_2_sloops_fm6_core_renders_what_composers_does(tmp_path):
    src = sloop_src()
    if src is None:
        pytest.skip("19.2: SLOOP's source is not here (research/sloop-src or $SLOOP_SRC: isod89/sloop-fm1 @ bac9c05); "
                    "the copies are still checked against its blob ids (test_19_2_copies_are_sloops_files_unchanged)")
    pinned = {u: b for _, u, b, _ in provenance()}
    for up in ("firmware/src/fm6_core.c", "tools/gen_tables.py"):
        assert blob(src / up) == pinned[up], f"{src} is not SLOOP @ bac9c05 ({up})"
    gen = tmp_path / "sloop-gen"
    gen.mkdir()
    for script, out in (("gen_tables.py", "felucca_tables.h"), ("gen_fm6_patches.py", "felucca_fm6.h")):
        r = subprocess.run([sys.executable, str(src / "tools" / script), str(gen / out)], capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
    theirs = build_ref(tmp_path, "ref_sloop", "-DFM6_REF_SLOOP", f"-I{src / 'firmware/src'}", f"-I{gen}")
    ours = build_ref(tmp_path, "ref_composer", f"-I{APP / 'engines'}")
    vs, nfactory = voices()
    data = b"".join(bytes(v) for v in vs)
    a = subprocess.run([str(theirs)], input=data, capture_output=True, check=True).stdout.decode().split("\n")
    b = subprocess.run([str(ours)], input=data, capture_output=True, check=True).stdout.decode().split("\n")
    assert len(a) == len(vs) + 1 and a == b, [(i, x, y) for i, (x, y) in enumerate(zip(a, b)) if x != y][:5]
    for line in a[:nfactory]:
        h, peak, nz = line.split()
        assert int(peak) > 1 << 20 and int(nz) > 10000, f"factory voice renders: {line}"


# ------------------------------------------------------------------ 19.4 --- #
BTN_PLAY, ENC_PRESETS = 12, 6
FM6_FIRST = 40                                           # (19.3: v4's 40 sounds, then FM6)
SAW_LEAD = 3


def push(t, data):
    out = [f"{t} sysex {link.frame(3, struct.pack('<I', len(data))).hex()}"]
    for off in range(0, len(data), 256):
        t += 2
        out.append(f"{t} sysex {link.frame(4, struct.pack('<I', off) + data[off:off + 256]).hex()}")
    t += 2
    out.append(f"{t} sysex {link.frame(5).hex()}")
    return out, t + 10


def centroid(frames, s0, s1):
    """the spectral centroid (Hz) of the left channel between s0 and s1 s: DFT bins every 25 Hz, 50 Hz .. 8 kHz"""
    x = frames[int(s0 * 44100) * 2:int(s1 * 44100) * 2:2]
    fs = [50 + 25 * k for k in range(320)]
    mags = []
    for f in fs:
        w = 2 * math.pi * f / 44100
        mags.append(math.hypot(sum(v * math.cos(w * i) for i, v in enumerate(x)), sum(v * math.sin(w * i) for i, v in enumerate(x))))
    return sum(f * m for f, m in zip(fs, mags)) / (sum(mags) or 1.0)


def test_19_4_presets_reach_fm6_strip_names_it_and_it_sounds_different(tmp_path):
    notes = [Note(t * 96, 60 + (t % 4) * 2, 90, 110) for t in range(64)]
    data = encode(Piece("fm", 1200, 64 * 96, [Channel("lead", notes)]))
    lines, t = push(100, data)
    runs = {}
    for name, extra in (("fm6_saw", []), ("fm6_tine", [f"{t + 300} enc {ENC_PRESETS} {FM6_FIRST - SAW_LEAD}"])):
        p = tmp_path / f"{name}.txt"
        p.write_text("\n".join(lines + [f"{t + 50} btn {BTN_PLAY}"] + extra + [f"{t + 3000} end"]) + "\n")
        runs[name] = run_host(p)
    log, wav = runs["fm6_tine"]
    s = [(y, txt) for ms, y, txt in screen(log) if ms >= t + 300]
    assert (64, "TINE EP") in s, "the strip shows the FM6 patch's name"
    assert (64, "SAW LEAD") in [(y, txt) for ms, y, txt in screen(runs["fm6_saw"][0]) if ms >= t + 50]
    a, b = wav_frames(runs["fm6_saw"][1]), wav_frames(wav)
    s0, s1 = (t + 800) / 1000, (t + 800) / 1000 + 0.1
    ea = sum(v * v for v in a[int(s0 * 44100) * 2:int(s1 * 44100) * 2])
    eb = sum(v * v for v in b[int(s0 * 44100) * 2:int(s1 * 44100) * 2])
    assert ea > 0 and eb > 0, "both play"
    ca, cb = centroid(a, s0, s1), centroid(b, s0, s1)
    assert max(ca, cb) > 1.3 * min(ca, cb), f"the FM6 sound's spectrum is the ANALOG one's (centroids {ca:.0f} / {cb:.0f} Hz)"
