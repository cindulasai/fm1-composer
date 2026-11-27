"""US-45: ENERGY and SWING macros in PLAYER (host mode): SEL held + KNOB3 = ENERGY (−50..+50, every note's velocity),
SEL held + KNOB4 = SWING (0..100, every channel at least this). Performance only: never saved, back to 0 on another
piece. The arithmetic: tests/steps_test.c (45.1)."""
import sys
from conftest import APP, wav_frames
from test_ui import at
from test_link import host_bin  # noqa: F401  (the fixture)
from test_new_song import pulled
from test_channels import run, BTN, FOOT

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Channel, Note, Piece, encode  # noqa: E402

KNOB3, KNOB4 = 4, 5


def macro(ms, enc, d):
    return [(ms, f"btndown {BTN['SEL']}"), (ms + 20, f"enc {enc} {d}"), (ms + 40, f"btnup {BTN['SEL']}")]


def lead(name="m", odd=False):
    """LEAD, 120 BPM, 8 bars: a note each beat at velocity 80, or (odd) one short note on the 2nd 1/16 of each beat"""
    notes = [Note(b * 96 + (24 if odd else 0), 69, 12 if odd else 80, 80) for b in range(32)]
    return encode(Piece(name, 1200, 8 * 384, [Channel("lead", notes)], [], None))


def rms(w, s0, s1):
    seg = w[int(s0 * 44100) * 2:int(s1 * 44100) * 2]
    return (sum(v * v for v in seg) / max(1, len(seg))) ** 0.5


def onsets(w, s0, s1, thr=1500):
    """seconds where the sound rises past thr after ≥ 30 ms under it"""
    out, quiet = [], 0
    for i in range(int(s0 * 44100), int(s1 * 44100)):
        a = abs(w[2 * i])
        if a > thr and quiet >= 1323:
            out.append(i / 44100)
        quiet = 0 if a > thr else quiet + 1
    return out


def test_45_2_energy_louder_and_swing_later_heard_and_shown(tmp_path, host_bin):
    loud = {}
    for e in (-50, 50):
        (log, wav), t0 = run(tmp_path, f"e{e + 50}", lead(), macro(0, KNOB3, e), 4000, play=True)
        assert (FOOT, f"ENERGY {'+' if e > 0 else '-'}50") in at(log, t0, t0 + 200)
        loud[e] = rms(wav_frames(wav), (t0 + 500) / 1000, (t0 + 3500) / 1000)
    assert loud[50] > 1.5 * loud[-50], loud
    starts = {}
    for s in (0, 100):
        (log, wav), t0 = run(tmp_path, f"s{s}", lead(odd=True), macro(0, KNOB4, s) if s else [], 4000, play=True)
        if s:
            assert (FOOT, "SWING 100") in at(log, t0, t0 + 200)
        starts[s] = onsets(wav_frames(wav), (t0 + 500) / 1000, (t0 + 3500) / 1000)
    assert min(len(starts[0]), len(starts[100])) >= 5, starts
    phase = {k: [x % 0.5 for x in v] for k, v in starts.items()}          # where in its beat (0.5 s) each note starts
    assert max(phase[0]) - min(phase[0]) < 0.003 and max(phase[100]) - min(phase[100]) < 0.003, phase
    assert 0.045 < phase[100][0] - phase[0][0] < 0.080, phase              # half a 1/16 at 120 BPM: 62.5 ms later


def test_45_3_ends_save_untouched_new_piece_resets_set_while_stopped(tmp_path, host_bin):
    steps = macro(0, KNOB3, 127) + macro(100, KNOB3, 127) + macro(200, KNOB4, -127) + macro(300, KNOB4, 127) + macro(400, KNOB4, 127)
    (log, _), t0 = run(tmp_path, "ends", lead(), steps, 600)
    shown = [txt for y, txt in at(log, t0) if y == FOOT]
    assert "ENERGY +50" in shown and "SWING 0" in shown and "SWING 100" in shown
    assert not any(t.startswith("ENERGY +5") and t != "ENERGY +50" for t in shown)
    saved = {}
    for name, extra in (("plain", []), ("macro", macro(0, KNOB3, -30) + macro(100, KNOB4, 70))):
        flash = tmp_path / f"{name}.bin"
        run(tmp_path, f"sv{name}", lead("same"), extra + [(300, f"btn {BTN['SAVE']}")], 500, flash=flash)
        (tmp_path / name).mkdir()
        saved[name] = pulled(tmp_path / name, host_bin, flash)["same"]
    assert saved["plain"] == saved["macro"]                                # the piece byte for byte as without macros
    # set while stopped: heard on PLAY
    (log, wav), t0 = run(tmp_path, "stopped", lead(), macro(0, KNOB3, -50) + [(100, f"btn {BTN['PLAY']}")], 3200)
    (log2, wav2), t1 = run(tmp_path, "stopped0", lead(), [(100, f"btn {BTN['PLAY']}")], 3200)
    assert (FOOT, "ENERGY -50") in at(log, t0, t0 + 99)
    assert rms(wav_frames(wav), (t0 + 600) / 1000, (t0 + 3000) / 1000) < 0.8 * rms(wav_frames(wav2), (t1 + 600) / 1000, (t1 + 3000) / 1000)
    # another piece opened (HOME, then PLAY on the library's piece): the macros start again from 0
    steps = macro(0, KNOB3, 20) + [(200, f"btn {BTN['HOME']}"), (400, f"btn {BTN['PLAY']}")] + macro(700, KNOB3, 1)
    (log, _), t0 = run(tmp_path, "reopen", lead(), steps, 900)
    assert (FOOT, "ENERGY +20") in at(log, t0, t0 + 199)
    assert (FOOT, "ENERGY +1") in at(log, t0 + 700, t0 + 899)
