"""US-12: a piece pushed over the TRS MIDI-in jack (host mode feeds it at 31250 baud, 3 bytes a millisecond), as
the browser emulator does: it reaches the same transfer as USB, the library lists it, PLAY plays it."""
import struct, sys
from conftest import APP, run_host, wav_frames
from test_ui import piece, script, at, rms

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402


def jack_push(t, data):
    """script lines putting one piece's frames on the jack from t ms: (lines, ms the bytes take)"""
    frames = [link.frame(3, struct.pack("<I", len(data)))]
    frames += [link.frame(4, struct.pack("<I", off) + data[off:off + 256]) for off in range(0, len(data), 256)]
    frames.append(link.frame(5))
    lines = [f"{t + i} jack F0 {f.hex()} F7" for i, f in enumerate(frames)]
    return lines, sum(len(f) + 2 for f in frames) // 3 + len(frames) + 50


def test_12_2_a_piece_over_the_jack_is_listed_and_plays(tmp_path):
    data = piece("jacked", nch=4, bars=4)
    lines, took = jack_push(100, data)
    t = 100 + took
    lines += [f"{t + 100} btn 12"]
    log, wav = run_host(script(tmp_path, "jack1", lines, t + 1500))
    lib = at(log, 0, t + 99)
    assert (4, "1 PIECE") in lib and (28, "> jacked") in lib and (4, "jacked") in at(log, t + 100)
    assert not [ln for ln in log.splitlines() if "SYSEX not ours" in ln]
    f = wav_frames(wav)
    s = (t + 300) / 1000
    assert rms(f, s, s + 0.5) > 300
    assert took > len(data) / 3                 # (it really went at the jack's speed)
