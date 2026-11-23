"""US-18: the effects on the device, through host mode: the FX screen, each role's starting effects, saving them
with the piece, and what was saved before (v4) or saved wrong."""
from pathlib import Path
from conftest import run_host, screen, screen_full
from test_ui import push, script, piece, at, library_with
from conftest import wav_frames

BTN = {"FX": 2, "SEL": 3, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB2": 3, "PRESETS": 6}
ROWS = ["REVERB", "DELAY", "CHORUS", "FILTER", "RESO", "DRIVE"]
BLOB = 4088                                    # host.c: a blob's place in HOST_FLASH (used, length, data)


def fx_page(log, ms0, ms1=10 ** 9):
    """the last FX screen drawn between ms0 and ms1: (header, picked row, {row: value})"""
    s = at(log, ms0, ms1)
    head = [t for y, t in s if y == 4 and t.startswith("FX ")]
    vals, picked = {}, None
    for k, name in enumerate(ROWS):
        y = 40 + 28 * k
        for yy, t in s:
            if yy != y:
                continue
            if t.lstrip(">").strip() == name:
                if t.startswith(">"):
                    picked = name
            elif t.isdigit():
                vals[name] = int(t)
    return (head[-1] if head else None), picked, vals


def blob1(flash):
    data = Path(flash).read_bytes()
    return len(data) - 9 * BLOB + 2 * BLOB      # (the library, the settings, blob 0, then blob 1)


def test_18_6_fx_screen_rows_knobs_home_save_reboot(tmp_path):
    flash = str(tmp_path / "flash.bin")
    keys = [(50, f"btn {BTN['PLAY']}"), (300, f"btn {BTN['FX']}"), (400, f"enc {ENC['SELECT']} 1"),
            (500, f"enc {ENC['KNOB1']} 30"), (600, f"enc {ENC['KNOB2']} 1"), (700, f"enc {ENC['KNOB2']} -1"),
            (800, f"btn {BTN['HOME']}"), (900, f"btn {BTN['PLAY']}"), (1000, f"btn {BTN['SAVE']}")]
    (log, wav), t = library_with(tmp_path, ["wet"], keys, 1800, name="fx1", env={"HOST_FLASH": flash})
    head, picked, v = fx_page(log, t + 300, t + 399)
    assert head == "FX 1 LEAD" and picked == "REVERB" and set(v) == set(ROWS)
    assert v["FILTER"] == 127 and v["RESO"] == 0 and v["DRIVE"] == 0
    assert fx_page(log, t + 400, t + 499)[1] == "DELAY"
    col = {txt: c for ms, y, x, c, txt in screen_full(log) if t + 400 <= ms <= t + 499 and y in (40, 68)}
    assert col.get(">DELAY") == 0xFD20 and col.get(" REVERB") == 0xFFFF, col    # the picked row in amber, the rest white
    after = fx_page(log, t + 500, t + 599)[2]["DELAY"]
    assert after == v["DELAY"] + 30
    assert fx_page(log, t + 600, t + 699)[0] == "FX 2 BASS"
    assert fx_page(log, t + 700, t + 799)[0] == "FX 1 LEAD"
    s = at(log, t + 800, t + 899)
    assert (64, ">1") in s and (64, "LEAD") in s, "HOME: back to PLAYER"
    assert (222, "SAVED") in at(log, t + 1000) and "blob 1 saved" in log
    (log0, wav0), _ = library_with(tmp_path, ["wet"], keys[:4], 1800, name="fx0")
    a, b = wav_frames(wav0), wav_frames(wav)
    turn = int((t + 500) / 1000 * 44100) * 2
    assert a[:turn - 2000] == b[:turn - 2000] and a[turn + 2000:turn + 44100] != b[turn + 2000:turn + 44100], "heard"
    log2, _ = run_host(script(tmp_path, "fx2", [f"100 btn {BTN['PLAY']}", f"400 btn {BTN['FX']}"], 900),
                       env={"HOST_FLASH": flash})
    assert fx_page(log2, 400)[2]["DELAY"] == after, "the saved effects come back after a power cycle"


def test_18_7_each_role_starts_with_its_effects(tmp_path):
    keys = [(50, f"btn {BTN['PLAY']}"), (300, f"btn {BTN['FX']}")] + [(400 + 100 * k, f"enc {ENC['KNOB2']} 1") for k in range(4)]
    (log, _), t = library_with(tmp_path, ["roles"], keys, 1200)
    want = {"FX 1 LEAD": None, "FX 2 BASS": 0, "FX 3 DRUMS": 0, "FX 4 PAD": None, "FX 5 CHORDS": None}
    for k, name in enumerate(want):
        head, _, v = fx_page(log, t + 300 + 100 * k, t + 399 + 100 * k)
        assert head == name
        assert v["FILTER"] == 127 and v["DRIVE"] == 0 and v["RESO"] == 0
        if want[name] == 0:
            assert v["REVERB"] == 0, name
        else:
            assert v["REVERB"] > 0, name


def test_18_8_v4_prefs_crafted_values_and_mashing(tmp_path):
    flash = str(tmp_path / "flash.bin")
    keys = [(50, f"btn {BTN['PLAY']}"), (600, f"enc {ENC['KNOB1']} 1"), (650, f"enc {ENC['PRESETS']} 5"),
            (800, "keys 0x10"), (850, "keys 0x0"), (900, f"btn {BTN['PLAY']}"), (1000, f"btn {BTN['SAVE']}")]
    (log, _), t = library_with(tmp_path, ["old"], keys, 1500, name="v4a", env={"HOST_FLASH": flash})
    chosen = [txt for y, txt in at(log, t + 700, t + 899) if y == 64 + 19 and txt not in ("BASS", " 2", ">2", "")][-1]
    data = bytearray(Path(flash).read_bytes())
    b1 = blob1(flash)
    assert data[b1] == 1, "the effects were saved in blob 1"
    data[b1] = 0                                   # as v4 left it: no blob 1
    Path(flash).write_bytes(data)
    s = [f"100 btn {BTN['PLAY']}", f"500 btn {BTN['FX']}"]
    log2, _ = run_host(script(tmp_path, "v4b", s, 900), env={"HOST_FLASH": flash})
    p = at(log2, 100, 499)
    assert (64 + 19, chosen) in p and (64 + 2 * 19, "M") in p, "v4's sound and mute are kept"
    head, _, v = fx_page(log2, 500)
    assert head == "FX 1 LEAD" and v["REVERB"] > 0 and v["FILTER"] == 127, "no saved effects: the role's"
    run_host(script(tmp_path, "v4c", [f"100 btn {BTN['PLAY']}", f"300 btn {BTN['PLAY']}", f"400 btn {BTN['SAVE']}"], 900),
             env={"HOST_FLASH": flash})
    data = bytearray(Path(flash).read_bytes())
    assert data[b1] == 1
    v0 = b1 + 4 + 8 + 6                            # (used, length; magic, stamp; the entry's id and stamp)
    data[v0:v0 + 48] = bytes(range(128, 176))      # every effect of every channel out of range
    Path(flash).write_bytes(data)
    log3, _ = run_host(script(tmp_path, "v4d", s, 900), env={"HOST_FLASH": flash})
    head, _, v = fx_page(log3, 500)
    assert head == "FX 1 LEAD" and v == {"REVERB": 127, "DELAY": 127, "CHORUS": 127, "FILTER": 127, "RESO": 10, "DRIVE": 127}
    lines, tt = push(100, piece("mash"))
    lines += [f"{tt + d} btn {BTN[b]}" for d, b in [(0, "PLAY"), (2, "FX"), (4, "SAVE"), (6, "FX"), (8, "SAVE"), (10, "FX"),
                                                     (12, "PLAY"), (14, "FX"), (16, "HOME"), (18, "FX"), (20, "HOME")]]
    lines[3:3] = [f"104 btn {BTN['FX']}", f"106 btn {BTN['SAVE']}"]       # (during the push, too)
    log4, _ = run_host(script(tmp_path, "mash", lines, tt + 600), env={"HOST_FLASH": str(tmp_path / "f2.bin")})
    last = [txt for ms, y, txt in screen(log4) if y == 222]
    assert "LIBRARY" in log4 or any(txt.startswith("FX ") for _, _, txt in screen(log4))
    assert not [x for x in last if "FAILED" in x], last
