"""US-3: fm1c convert / compose through the real command (bin/fm1c)."""
import os, subprocess, sys
from pathlib import Path
import pytest
from conftest import APP
from midifix import make_midi

FM1C = APP / "fm1c"
T2M = Path(os.path.expanduser("~/Claude/Projects/text2midi_v2/target/release/t2m-cli"))
BRIEF = Path(os.path.expanduser("~/Claude/Projects/text2midi_v2/suite/briefs/electronic-house-1.json"))
sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import decode  # noqa: E402


def fm1c(*args, cwd=None):
    return subprocess.run([str(FM1C), *map(str, args)], capture_output=True, text=True, cwd=cwd)


def test_3_1_convert_writes_a_piece_and_reports(tmp_path):
    from test_fit import ten_parts
    mid = ten_parts(tmp_path)
    out = tmp_path / "ten.f1p"
    r = fm1c("convert", mid, "-o", out)
    assert r.returncode == 0, r.stderr
    assert "8 channels" in r.stdout and "3 reductions" in r.stdout and "role default sound for" in r.stdout   # (22.6)
    assert "fx" in r.stdout and "merged into the drums" in r.stdout
    p = decode(out.read_bytes())
    assert len(p.channels) == 8 and p.name == "ten"


@pytest.mark.skipif(not T2M.exists() or not BRIEF.exists(), reason="text2midi's t2m-cli not built here")
def test_3_2_compose_is_deterministic(tmp_path):
    a, b = tmp_path / "a.f1p", tmp_path / "b.f1p"
    r1 = fm1c("compose", BRIEF, "--seed", 1, "-o", a)
    r2 = fm1c("compose", BRIEF, "--seed", 1, "-o", b)
    assert r1.returncode == 0 and r2.returncode == 0, r1.stderr + r2.stderr
    assert a.read_bytes() == b.read_bytes()
    assert decode(a.read_bytes()).name == "electronic-house-1"


def hostile_cases(tmp):
    good = make_midi(tmp / "good.mid", [("lead", 0, [(0, 60, 100, 90)])])
    (tmp / "corrupt.mid").write_bytes(b"MThd\x00\x00\x00\x06garbage")
    empty = make_midi(tmp / "empty.mid", [("lead", 0, [])])
    late = make_midi(tmp / "late.mid", [("lead", 0, [(1000 * 4 * 480, 60, 100, 90)])])   # past bar 999
    big = tmp / "big.mid"
    with open(big, "wb") as f:
        f.truncate(10 * 1024 * 1024)
    (tmp / "dir.mid").mkdir()
    (tmp / "exists.f1p").write_bytes(b"keep")
    noexec = tmp / "t2m-noexec"
    noexec.write_text("#!/bin/sh\n")
    noexec.chmod(0o644)
    return {
        "missing file": (["convert", tmp / "missing.mid"], "no such file"),
        "directory": (["convert", tmp / "dir.mid"], "is a folder"),
        "corrupt MIDI": (["convert", tmp / "corrupt.mid", "-o", tmp / "c.f1p"], "not a readable MIDI file"),
        "empty MIDI": (["convert", empty, "-o", tmp / "e.f1p"], "has no notes"),
        "10 MB MIDI": (["convert", big, "-o", tmp / "b.f1p"], "larger than the 8 MB"),
        "output exists": (["convert", good, "-o", tmp / "exists.f1p"], "already exists"),
        "output folder missing": (["convert", good, "-o", tmp / "nope/x.f1p"], "no such folder"),
        "unknown flag": (["convert", good, "--frobnicate"], "unrecognized arguments"),
        "missing arg": (["convert"], "the following arguments are required"),
        "no command": ([], "a command is needed"),
        "bad poly": (["convert", good, "--poly", "0"], "invalid choice"),
        "t2m-cli missing": (["compose", BRIEF, "--t2m", tmp / "no-t2m"], "t2m-cli not found"),
        "brief missing": (["compose", tmp / "nobrief.json", "--t2m", "/bin/sh"], "no such brief"),
        "no notes left": (["convert", late, "-o", tmp / "late.f1p"], "no notes left to play"),
        "t2m-cli not runnable": (["compose", BRIEF, "--t2m", noexec], "t2m-cli not found"),
    }


@pytest.mark.parametrize("case", list(hostile_cases(Path("/tmp")).keys()) if False else [
    "missing file", "directory", "corrupt MIDI", "empty MIDI", "10 MB MIDI", "output exists", "output folder missing",
    "unknown flag", "missing arg", "no command", "bad poly", "t2m-cli missing", "brief missing", "t2m-cli not runnable", "no notes left"])
def test_3_4_hostile_inputs_exit_2_cleanly(case, tmp_path):
    args, msg = hostile_cases(tmp_path)[case]
    before = sorted(p.name for p in tmp_path.iterdir())
    r = fm1c(*args)
    assert r.returncode == 2, (case, r.stdout, r.stderr)
    assert msg in r.stderr and "Traceback" not in r.stderr
    assert len(r.stderr.strip().splitlines()) == 1
    assert sorted(p.name for p in tmp_path.iterdir()) == before          # nothing half-written
    if case == "output exists":
        assert (tmp_path / "exists.f1p").read_bytes() == b"keep"


def test_3_4_force_replaces_and_paths_with_spaces(tmp_path):
    d = tmp_path / "my pieces é"
    d.mkdir()
    mid = make_midi(d / "a song.mid", [("lead", 0, [(0, 60, 100, 90)])])
    out = d / "a song.f1p"
    out.write_bytes(b"old")
    assert fm1c("convert", mid, "-o", out).returncode == 2
    r = fm1c("convert", mid, "-o", out, "--force")
    assert r.returncode == 0, r.stderr
    assert decode(out.read_bytes()).name == "a song"


def test_3_4_default_output_name_in_cwd(tmp_path):
    mid = make_midi(tmp_path / "tune.mid", [("bass", 0, [(0, 40, 100, 90)])])
    r = fm1c("convert", "tune.mid", cwd=tmp_path)
    assert r.returncode == 0 and (tmp_path / "tune.f1p").exists()


def fake_t2m(tmp_path, fail=False):
    """A stand-in t2m-cli: `export <brief> --out DIR ... --name piece` writes DIR/piece.mid."""
    src = make_midi(tmp_path / "src.mid", [("bass – b", 0, [(0, 40, 100, 90)])])
    f = tmp_path / "t2m"
    body = 'echo "error: brief is broken" >&2; exit 1' if fail else \
        'while [ $# -gt 0 ]; do [ "$1" = --out ] && out=$2; shift; done; cp "%s" "$out/piece.mid"' % src
    f.write_text("#!/bin/sh\n" + body + "\n")
    f.chmod(0o755)
    return f


def test_3_2_compose_force_and_existing_output(tmp_path):
    brief = tmp_path / "song.json"; brief.write_text("{}")
    t2m = fake_t2m(tmp_path)
    out = tmp_path / "song.f1p"
    r = fm1c("compose", brief, "--t2m", t2m, "-o", out)
    assert r.returncode == 0, r.stderr
    assert decode(out.read_bytes()).name == "song"
    r = fm1c("compose", brief, "--t2m", t2m, "-o", out)
    assert r.returncode == 2 and "already exists" in r.stderr
    r = fm1c("compose", brief, "--t2m", t2m, "-o", out, "--force", "--name", "Renamed")
    assert r.returncode == 0 and decode(out.read_bytes()).name == "Renamed"


def test_3_4_compose_reports_text2midi_failure(tmp_path):
    brief = tmp_path / "song.json"; brief.write_text("{}")
    r = fm1c("compose", brief, "--t2m", fake_t2m(tmp_path, fail=True), "-o", tmp_path / "x.f1p")
    assert r.returncode == 2 and "text2midi could not compose" in r.stderr and "brief is broken" in r.stderr
    assert not (tmp_path / "x.f1p").exists()


def test_3_2_compose_names_the_output_after_the_brief(tmp_path):
    brief = tmp_path / "night drive.json"; brief.write_text("{}")
    r = fm1c("compose", brief, "--t2m", fake_t2m(tmp_path), cwd=tmp_path)
    assert r.returncode == 0, r.stderr
    assert decode((tmp_path / "night drive.f1p").read_bytes())


def test_3_1_outputs_are_ordinary_files(tmp_path):
    import os, stat
    mid = make_midi(tmp_path / "a.mid", [("lead", 0, [(0, 60, 240, 90)])])
    old = os.umask(0o022)
    try:
        r = fm1c("convert", mid, "-o", tmp_path / "a.f1p")
    finally:
        os.umask(old)
    assert r.returncode == 0 and stat.S_IMODE((tmp_path / "a.f1p").stat().st_mode) == 0o644


def test_3_4_help_carries_the_claims():
    r = fm1c("--help")
    assert "16 voices (measured in fm1-emu" in r.stdout and "Inspired by SLOOP" in r.stdout
