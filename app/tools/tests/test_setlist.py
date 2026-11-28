"""US-46: setlists on the FM-1 (host mode). LIBRARY: ARP adds the selected piece to the set, GLO clears it, SEQ plays
it: each piece takes over at the end of the one before (a live send, no gap); the last plays once (SET DONE). Kept
across a power-off (blob 2)."""
import sys
from conftest import APP, run_host, wav_frames
from test_ui import push, script, at
from test_live import peak_at
from test_link import host_bin  # noqa: F401  (the fixture)
from test_new_song import fm1c

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Channel, Note, Piece, encode  # noqa: E402

BTN = {"GLO": 7, "HOME": 8, "PLAY": 12, "ARP": 10, "SEQ": 11}
SELECT = 0
FOOT = 222
PITCH = {"low": 57, "mid": 69, "high": 81}                   # A3, A4, A5: each piece one note, heard apart


def tune(name, bars=1):
    """LEAD, 120 BPM: bars of 2 s, a long note each beat"""
    return encode(Piece(name, 1200, bars * 384, [Channel("lead", [Note(b * 96, PITCH[name], 90, 100) for b in range(4 * bars)])], [], None))


def lib(tmp_path, name, steps, end, names=("low", "mid", "high"), flash=None):
    """pieces pushed (LIBRARY newest first: high, mid, low), then steps [(ms after t0, what)]: (log, wav), t0"""
    lines, t = [], 100
    for n in names:
        l, t = push(t, tune(n))
        lines += l
    t0 = t + 50
    lines += [f"{t0 + ms} {what}" for ms, what in steps]
    return run_host(script(tmp_path, name, lines, t0 + end), env={"HOST_FLASH": str(flash)} if flash else None), t0


def foot(log, a, b=10 ** 9):
    return [txt for y, txt in at(log, a, b) if y in (FOOT, 220)]             # (PLAYER 222, LIBRARY 220)


ADD3 = [(0, f"btn {BTN['ARP']}"), (50, f"enc {SELECT} 2"), (100, f"btn {BTN['ARP']}"),     # high, low, mid
        (150, f"enc {SELECT} -1"), (200, f"btn {BTN['ARP']}")]


def test_46_1_a_set_plays_back_to_back_and_stops_after_the_last(tmp_path):
    (log, wav), t0 = lib(tmp_path, "set1", ADD3 + [(300, f"btn {BTN['SEQ']}")], 300 + 7500)
    f = foot(log, t0, t0 + 299)
    assert [x for x in f if x.startswith("SET")] == ["SET 1: high", "SET 2: low", "SET 3: mid"], f
    assert f[-2].endswith("KB FREE")                                       # (moving the selection clears the message)
    later = foot(log, t0 + 300)
    assert "SET 1/3 high" in later and "SET 2/3 low" in later and "SET 3/3 mid" in later and later[-1] == "SET DONE", later
    w = wav_frames(wav)
    s = (t0 + 300) / 1000                                                    # each piece 2 s
    for k, name in enumerate(("high", "low", "mid")):
        ok, p = peak_at(w, s + 2 * k + 0.3, s + 2 * k + 1.7, PITCH[name])
        assert ok, (name, p)
    gap = [abs(x) for x in w[int((s + 1.98) * 44100) * 2:int((s + 2.1) * 44100) * 2]]   # the hand-over: sound continues
    assert max(gap) > 300
    tail = w[int((s + 6.8) * 44100) * 2:int((s + 7.4) * 44100) * 2]
    assert max(abs(x) for x in tail) < 50                                    # after the last: silence (it played once)


def test_46_2_kept_across_power_off_and_a_deleted_piece_drops_out(tmp_path):
    flash = tmp_path / "f.bin"
    lib(tmp_path, "keep1", ADD3, 400, flash=flash)
    (log, _), t0 = run_host(script(tmp_path, "keep2", [f"200 btn {BTN['SEQ']}"], 1500), env={"HOST_FLASH": str(flash)}), 200
    assert "SET 1/3 high" in foot(log, 200)
    (log, _), _ = run_host(script(tmp_path, "keep3", [f"200 btn {BTN['GLO']}", f"400 btn {BTN['SEQ']}"], 900), env={"HOST_FLASH": str(flash)}), 0
    assert foot(log, 200, 399)[:1] == ["SET CLEARED"] and "SET IS EMPTY" in foot(log, 400)


def test_46_3_full_twice_empty_stop_mid_set(tmp_path):
    steps = [(0, f"btn {BTN['SEQ']}")] + [(100 + 40 * k, f"btn {BTN['ARP']}") for k in range(17)]
    (log, _), t0 = lib(tmp_path, "full", steps, 1000)
    f = foot(log, t0)
    assert f[0] == "SET IS EMPTY" and "SET 16: high" in f and f[-1] == "SET FULL (16)", f   # the same piece many times: fine
    steps = ADD3 + [(300, f"btn {BTN['SEQ']}"), (1300, f"btn {BTN['PLAY']}"), (1500, f"btn {BTN['HOME']}"), (1700, f"btn {BTN['SEQ']}")]
    (log, wav), t0 = lib(tmp_path, "stop", steps, 2500)
    assert "STOPPED" in foot(log, t0 + 1300, t0 + 1499)
    assert "SET 1/3 high" in foot(log, t0 + 1700)                            # from the start again


def test_46_2_a_piece_deleted_from_the_library_leaves_the_set(tmp_path, host_bin):
    flash = tmp_path / "f.bin"
    lib(tmp_path, "del1", ADD3, 400, flash=flash)
    port = f"exec:{host_bin} {flash}"
    low = next(l.split()[0].lstrip("#") for l in fm1c("list", "--port", port).stdout.splitlines() if l.startswith("#") and " low " in l + " ")
    assert fm1c("delete", low, "--port", port).returncode == 0
    (log, _) = run_host(script(tmp_path, "del2", [f"200 btn {BTN['SEQ']}"], 4800), env={"HOST_FLASH": str(flash)})
    f = foot(log, 200)
    assert "1 PIECE LEFT THE SET" in f and "SET 2/2 mid" in f and f[-1] == "SET DONE", f


def test_46_3_a_corrupt_saved_set_starts_empty(tmp_path):
    flash = tmp_path / "f.bin"
    lib(tmp_path, "bad1", ADD3, 400, flash=flash)
    data = bytearray(flash.read_bytes())
    i = data.find(b"SET1"[::-1]) if data.find(b"SET1"[::-1]) >= 0 else data.find((0x31544553).to_bytes(4, "little"))
    assert i >= 0
    data[i + 4:i + 8] = (999).to_bytes(4, "little")                           # n = 999: more than a set holds
    flash.write_bytes(bytes(data))
    (log, _) = run_host(script(tmp_path, "bad2", [f"200 btn {BTN['SEQ']}"], 600), env={"HOST_FLASH": str(flash)})
    assert "SET IS EMPTY" in foot(log, 200)
