"""US-38: NEW SONG on the FM-1 (host mode). LIBRARY: SAVE opens the set-up screen (KNOB1 channels, SELECT one,
PRESETS its role, KNOB2 tempo, KNOB3 bars); PLAY makes the empty piece, saved at once as NEW n, and opens it in PLAYER.
The piece re-read from flash (fm1c pull) is byte for byte fm1c's encode() of the same empty piece."""
import sys
from conftest import APP, run_host, screen
from test_ui import push, script, at
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c
from test_edit_ui import song

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Channel, Note, Piece, decode, encode  # noqa: E402

BTN = {"HOME": 8, "SAVE": 9, "PLAY": 12, "LFO": 5}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB2": 3, "KNOB3": 4, "PRESETS": 6}
AUTO = 1                                                         # a channel's voice: the sound's own (fm1c POLY, AUTO, …)
FOOT = 222


def run(tmp_path, name, steps, end, pieces=(), flash=None):
    """push pieces, then steps [(ms after, what)] from LIBRARY: (log, t0)"""
    lines, t = [], 100
    for p in pieces:
        l, t = push(t, p)
        lines += l
    t0 = t + 50
    lines += [f"{t0 + ms} {what}" for ms, what in steps]
    env = {"HOST_FLASH": str(flash)} if flash else None
    log, _ = run_host(script(tmp_path, name, lines, t0 + end), env=env)
    return log, t0


def pulled(tmp_path, host_bin, flash):
    """every library piece as fm1c pulls it: {name: bytes}"""
    port = f"exec:{host_bin} {flash}"
    out = {}
    for line in fm1c("list", "--port", port).stdout.splitlines():
        if not line.startswith("#"):
            continue
        pid, name = line.split()[0].lstrip("#"), line.split(None, 1)[1].rsplit(None, 1)[0].strip()
        dst = tmp_path / f"p{pid}.f1p"
        assert fm1c("pull", pid, "-o", dst, "--port", port).returncode == 0
        out[decode(dst.read_bytes()).name] = dst.read_bytes()
    return out


def empty(name, tempo_x10, bars, roles):
    return encode(Piece(name, tempo_x10, bars * 384, [Channel(r, mode=AUTO) for r in roles]))


def test_38_1_set_up_screen_then_an_empty_piece_of_those_channels(tmp_path, host_bin):
    flash = tmp_path / "f.bin"
    steps = [(0, f"btn {BTN['SAVE']}"), (100, f"enc {ENC['KNOB1']} 2"), (200, f"enc {ENC['SELECT']} 3"),
             (300, f"enc {ENC['PRESETS']} -1"), (400, f"enc {ENC['SELECT']} -1"), (500, f"enc {ENC['PRESETS']} 1"),
             (600, f"enc {ENC['KNOB2']} -2"), (700, f"enc {ENC['KNOB3']} -4"), (800, f"btn {BTN['PLAY']}")]
    log, t0 = run(tmp_path, "ns1", steps, 1000, flash=flash)
    first = at(log, t0, t0 + 99)
    for want in ((4, "NEW SONG"), (4, "120 BPM 8 BARS"), (28, "4 CHANNELS"), (50, "> 1 DRUMS"), (70, "  2 BASS"),
                 (90, "  3 CHORDS"), (110, "  4 LEAD"), (FOOT, "PLAY MAKES IT  HOME CANCELS")):
        assert want in first, (want, first)                                   # the defaults
    assert (150, "  6 ARP") in at(log, t0 + 100, t0 + 199)                    # 6 channels: PAD, ARP added
    assert (110, "> 4 OTHER") in at(log, t0 + 300, t0 + 399)                  # LEAD, one back: wraps to OTHER
    assert (90, "> 3 PAD") in at(log, t0 + 500, t0 + 599)                     # CHORDS + 1: PAD
    assert (4, "118 BPM 4 BARS") in at(log, t0 + 700, t0 + 799)
    player = at(log, t0 + 800, t0 + 999)
    assert (4, "NEW 1") in player and (4, "118 BPM") in player and (26, "1:1") in player
    assert (FOOT, "NEW SONG: EMPTY, FILL IT WITH STEPS OR REC") in player
    roles = ("drums", "bass", "pad", "other", "pad", "arp")
    assert pulled(tmp_path, host_bin, flash) == {"NEW 1": empty("NEW 1", 1180, 4, roles)}   # saved at once


def test_38_2_filled_with_steps_saved_newest_first_one_format(tmp_path, host_bin):
    flash = tmp_path / "f.bin"
    steps = [(0, f"btn {BTN['SAVE']}"), (100, f"btn {BTN['PLAY']}"), (300, f"btn {BTN['LFO']}"),
             (400, "keys 128"), (460, "keys 0"), (600, f"btn {BTN['LFO']}"), (700, f"btn {BTN['SAVE']}"),
             (800, f"btn {BTN['HOME']}")]
    log, t0 = run(tmp_path, "ns2", steps, 1000, pieces=[song(name="older")], flash=flash)
    assert (FOOT, "SAVED") in at(log, t0 + 700, t0 + 799)
    lib = at(log, t0 + 800)
    assert (28, "> NEW 1") in lib and (4, "2 PIECES") in lib                  # newest first
    raw = pulled(tmp_path, host_bin, flash)["NEW 1"]
    back = decode(raw)
    assert encode(back) == raw and len(back.channels) == 4
    assert [(n.tick, n.pitch) for n in back.channels[0].notes] == [(0, 60)] and all(not c.notes for c in back.channels[1:])


def test_38_3_one_and_eight_channels_clashing_names_and_cancel(tmp_path, host_bin):
    flash = tmp_path / "f.bin"
    steps = [(0, f"btn {BTN['SAVE']}"), (100, f"btn {BTN['HOME']}"),                          # cancel: nothing made
             (200, f"btn {BTN['SAVE']}"), (300, f"enc {ENC['KNOB1']} -9"), (350, f"enc {ENC['SELECT']} 5"),
             (400, f"btn {BTN['PLAY']}"), (600, f"btn {BTN['HOME']}"),
             (700, f"btn {BTN['SAVE']}"), (800, f"enc {ENC['KNOB1']} 9"), (850, f"enc {ENC['SELECT']} 9"),
             (900, f"btn {BTN['PLAY']}")]
    log, t0 = run(tmp_path, "ns3", steps, 1100, pieces=[song(name="NEW 1")], flash=flash)
    assert (4, "1 PIECE") in at(log, t0 + 100, t0 + 199)                     # HOME: back, the library as it was
    assert (28, "1 CHANNEL") in at(log, t0 + 300, t0 + 399) and (50, "> 1 DRUMS") in at(log, t0 + 350, t0 + 399)
    assert (4, "NEW 2") in at(log, t0 + 400, t0 + 599)                        # NEW 1 taken: the next free
    assert (28, "8 CHANNELS") in at(log, t0 + 800, t0 + 899) and (190, "> 8 PERC") in at(log, t0 + 850, t0 + 899)
    got = pulled(tmp_path, host_bin, flash)
    assert sorted(got) == ["NEW 1", "NEW 2", "NEW 3"]
    assert got["NEW 2"] == empty("NEW 2", 1200, 8, ["drums"])
    assert got["NEW 3"] == empty("NEW 3", 1200, 8, ["drums", "bass", "chords", "lead", "pad", "arp", "counter", "percussion"])


def test_38_3_library_full_makes_nothing(tmp_path, host_bin):
    big = [song([Note(i * 24, 60 + i % 12, 10, 90) for i in range(5322)], f"big{k}", bars=999) for k in range(9)]
    small = song(name="small")
    flash = tmp_path / "f.bin"
    steps = [(0, f"btn {BTN['SAVE']}"), (100, f"btn {BTN['PLAY']}"), (200, f"btn {BTN['HOME']}")]
    log, t0 = run(tmp_path, "nsfull", steps, 300, pieces=[*big, small], flash=flash)
    assert (FOOT, "LIBRARY FULL") in at(log, t0 + 100, t0 + 199)
    assert not any(txt.startswith("NEW ") for _, txt in at(log, t0 + 100, t0 + 199)) and (4, "10 PIECES") in at(log, t0 + 200)
    assert sorted(pulled(tmp_path, host_bin, flash)) == sorted([f"big{k}" for k in range(9)] + ["small"])


def test_38_3_an_empty_piece_plays_silent_and_loops_its_length(tmp_path):
    steps = [(0, f"btn {BTN['SAVE']}"), (100, f"enc {ENC['KNOB2']} 127"), (200, f"enc {ENC['KNOB3']} -9"),
             (300, f"btn {BTN['PLAY']}"), (400, f"btn {BTN['PLAY']}")]
    log, t0 = run(tmp_path, "nsplay", steps, 3000)
    assert (4, "240 BPM 1 BARS") in at(log, t0 + 200, t0 + 299)               # clamped: 240, 1 bar
    pos = [txt for ms, y, txt in screen(log) if y == 26 and ms >= t0 + 400]
    assert "1:4" in pos and pos.index("1:4") < len(pos) - 1 - pos[::-1].index("1:1"), pos   # back to 1:1: it loops
    assert (FOOT, "PLAYING") in at(log, t0 + 400, t0 + 499) and not any("1:5" in p or "2:" in p for p in pos)
