"""US-11: the EDIT screen through host mode: pieces pushed over SysEx, then buttons, encoders and note keys from a
script; asserted on the screen text, the piano roll's pixels (screenshots), the sound, and the saved piece read back
with the real fm1c."""
import shutil, struct, sys
from conftest import APP, run_host, screen, screen_full, wav_frames
from test_ui import push, script, at, rms, shot, catalog
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Piece, Channel, Note, encode, decode  # noqa: E402

BTN = {"OCT-": 0, "OCT+": 1, "FX": 2, "EDIT": 6, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB2": 3, "KNOB3": 4, "PRESETS": 6}
TWO = [Note(0, 60, 24, 100), Note(96, 64, 48, 90)]


def song(notes=TWO, name="song", bars=4, extra=()):
    return encode(Piece(name, 1200, bars * 384, [Channel("lead", list(notes)), *extra]))


def cell(img, col, pitch, low=53):
    """the colour in the middle of the roll's cell (step col, pitch)"""
    return img.getpixel((24 + col * 13 + 6, 26 + (low + 26 - pitch) * 6 + 3))


def amber(px):
    return px[0] > 200 and 130 < px[1] < 200 and px[2] < 60


def run(tmp_path, pieces, steps, end, name=None, env=None):
    """push pieces, open the first listed (newest), stop it, EDIT; then steps [(ms after, what)]: (log, wav), t0"""
    lines, t = [], 100
    for p in pieces:
        l, t = push(t, p)
        lines += l
    lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 150} btn {BTN['PLAY']}", f"{t + 250} btn {BTN['EDIT']}"]
    t0 = t + 300
    lines += [f"{t0 + ms} {what}" for ms, what in steps]
    return run_host(script(tmp_path, name or tmp_path.name, lines, t0 + end), env=env), t0


def key(ms, k, hold=60):
    return [(ms, f"keys {1 << k}"), (ms + hold, "keys 0")]


def test_11_1_edit_shows_the_channel_as_a_piano_roll(tmp_path):
    edge = Note(336, 67, 49, 90)                                        # ends on the page's last tick + 1
    (log, _), t0 = run(tmp_path, [song(TWO + [edge])], [(50, "shot")], 100)
    assert (4, "EDIT 1 LEAD") in at(log, t0 - 50) and (4, "BAR 1  1/16") in at(log, t0 - 50)
    img = shot(tmp_path.name, t0 + 50)
    assert amber(cell(img, 14, 67)) and amber(cell(img, 15, 67))
    assert img.getpixel((24 + 16 * 13 + 2, 26 + (79 - 67) * 6 + 3)) == (0, 0, 0)      # nothing past step 16
    assert img.getpixel((24 + 3 * 13 + 6, 26 + (79 - 61) * 6 + 3)) == (24, 24, 24)    # C#4: a black-key row
    assert img.getpixel((24 + 3 * 13 + 6, 26 + (79 - 62) * 6 + 3)) == (0, 0, 0)       # D4: white
    assert img.getpixel((10, 26 + (79 - 60) * 6 + 3)) == (123, 121, 123) and img.getpixel((10, 26 + (79 - 62) * 6 + 3)) == (0, 0, 0)
    assert amber(cell(img, 0, 60)) and amber(cell(img, 4, 64)) and amber(cell(img, 5, 64))   # 1/8 = 2 steps
    assert not amber(cell(img, 1, 60)) and not amber(cell(img, 6, 64)) and not amber(cell(img, 0, 61))
    assert (196, "C4  LEN 1/16  VEL 100") in at(log, t0 - 50)


def test_11_2_keys_add_and_remove_notes_cursor_grid_octave(tmp_path):
    steps = [*key(100, 9), (300, "shot"), *key(400, 9), (600, "shot"),
             (700, f"enc {ENC['SELECT']} 2"), *key(800, 9), (1000, "shot"),
             (1100, f"enc {ENC['KNOB1']} 1"), (1200, f"btn {BTN['OCT+']}"), (1300, f"enc {ENC['SELECT']} -1"),
             *key(1400, 0), (1600, "shot")]
    (log, wav), t0 = run(tmp_path, [song()], steps, 1700)
    a, b, c, d = (shot(tmp_path.name, t0 + ms) for ms in (300, 600, 1000, 1600))
    assert amber(cell(a, 0, 62)) and not amber(cell(b, 0, 62))         # the D4 key: added, then removed
    assert amber(cell(b, 0, 60))                                        # the note already there stays
    assert amber(cell(c, 2, 62)) and not amber(cell(c, 0, 62))          # two steps on
    f = wav_frames(wav)
    s = (t0 + 100) / 1000
    assert rms(f, s, s + 0.05) > 300 and rms(f, s - 0.1, s - 0.02) < 50  # the new note is heard as it goes in
    assert (4, "BAR 1  1/8") in at(log, t0 + 1100, t0 + 1199)
    assert amber(cell(d, 0, 65, low=65))                                # an octave up: F3 key = F4, at step 0 of 1/8
    assert (196, "C4  LEN 1/16  VEL 100") in at(log, t0 + 1400)          # (the step's first note, C4 before F4)


def test_11_3_length_velocity_and_undo(tmp_path):
    steps = [(100, f"enc {ENC['SELECT']} 4"), (200, f"enc {ENC['KNOB2']} 1"), (300, f"enc {ENC['KNOB3']} -2"),
             (400, f"btn {BTN['FX']}"), (500, f"btn {BTN['FX']}"), (600, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [song()], steps, 700)
    assert (196, "E4  LEN 1/8  VEL 90") in at(log, t0 + 100, t0 + 199)
    assert (196, "E4  LEN 72T  VEL 90") in at(log, t0 + 200, t0 + 299)
    assert (196, "E4  LEN 72T  VEL 82") in at(log, t0 + 300, t0 + 399)
    assert (196, "E4  LEN 72T  VEL 90") in at(log, t0 + 400, t0 + 499) and (222, "UNDONE") in at(log, t0 + 400, t0 + 499)
    assert (196, "E4  LEN 1/8  VEL 90") in at(log, t0 + 500, t0 + 599)
    assert (222, "NOTHING TO UNDO") in at(log, t0 + 600)
    assert (4, "EDIT 1 LEAD *") in at(log, t0 + 200)                    # unsaved edits are marked


def test_11_4_save_replaces_the_piece_and_keeps_the_choices(tmp_path, host_bin):
    names = catalog(tmp_path)
    flash = str(tmp_path / "flash.bin")
    steps = [(50, f"btn {BTN['HOME']}"), (100, f"enc {ENC['PRESETS']} 1"), *key(130, 0), (200, f"btn {BTN['EDIT']}"),
             *key(300, 9), (500, f"btn {BTN['SAVE']}")]
    (log, _), t0 = run(tmp_path, [song()], steps, 900, name="esave1", env={"HOST_FLASH": flash})
    assert (222, "SAVED") in at(log, t0 + 500)
    turned = [txt for ms, y, x, c, txt in screen_full(log) if t0 + 100 <= ms < t0 + 200 and y == 64 and x == 98][-1]
    first = [txt for ms, y, x, c, txt in screen_full(log) if ms < t0 + 100 and y == 64 and x == 98][-1]
    assert turned == names[(names.index(first) + 1) % len(names)]
    (log2, _) = run_host(script(tmp_path, "esave2", ["100 btn 12"], 600), env={"HOST_FLASH": flash})
    assert (4, "1 PIECE") in at(log2, 0, 99) and (28, "> song") in at(log2, 0, 99)
    assert [txt for ms, y, x, c, txt in screen_full(log2) if ms >= 100 and y == 64 and x == 98][-1] == turned
    assert [txt for ms, y, x, c, txt in screen_full(log2) if ms >= 100 and y == 64 and x == 206][-1] == "M"   # mute kept
    fl = tmp_path / "xflash.bin"
    shutil.copy(flash, fl)
    port = f"exec:{host_bin} {fl}"
    lst = fm1c("list", "--port", port).stdout
    assert "song" in lst and "1 piece" in lst, lst
    pid = lst.split()[0].lstrip("#")
    r = fm1c("pull", pid, "-o", tmp_path / "back.f1p", "--port", port)
    assert r.returncode == 0, r.stderr
    assert decode((tmp_path / "back.f1p").read_bytes()).channels[0].notes == sorted(TWO + [Note(0, 62, 24, 100)])


def test_11_6_edits_while_playing_unsaved_and_full(tmp_path):
    steps = [(50, f"btn {BTN['PLAY']}"), *key(150, 9), (230, f"enc {ENC['KNOB2']} 2"), (260, f"enc {ENC['KNOB3']} -3"),
             (300, f"btn {BTN['PLAY']}"), (400, "shot"),
             *key(500, 9), (700, f"btn {BTN['HOME']}"), (800, f"btn {BTN['HOME']}"), (900, f"btn {BTN['HOME']}"),
             (1000, f"btn {BTN['PLAY']}"), (1100, f"btn {BTN['PLAY']}"), (1200, f"btn {BTN['EDIT']}"), (1300, "shot")]
    (log, _), t0 = run(tmp_path, [song()], steps, 1400)
    assert (222, "PLAYING FROM THIS BAR") in at(log, t0 + 50, t0 + 149)
    assert (222, "STOP TO EDIT") in at(log, t0 + 150, t0 + 299)
    assert not amber(cell(shot(tmp_path.name, t0 + 400), 0, 62))       # nothing changed while playing
    assert (222, "STOP TO EDIT") in at(log, t0 + 230, t0 + 299) and (196, "C4  LEN 1/16  VEL 100") in at(log, t0 + 300, t0 + 499)
    assert (222, "UNSAVED EDITS: HOME AGAIN TO DISCARD") in at(log, t0 + 800, t0 + 899)
    assert (4, "COMPOSER") in at(log, t0 + 900, t0 + 999)               # the second HOME leaves
    assert not amber(cell(shot(tmp_path.name, t0 + 1300), 0, 62))      # and the edit is gone


def test_11_6_piece_full_and_library_full(tmp_path, host_bin):
    full = song([Note(i, 60 + i % 12, 10, 90) for i in range((32768 - 68) // 6)], "full", bars=999)
    assert len(full) == 32768
    (log, _), t0 = run(tmp_path, [full], key(100, 0), 300, name="efull")      # (F3: not in that step yet)
    assert (222, "PIECE FULL") in at(log, t0 + 100)
    big = [song([Note(i * 24, 60 + i % 12, 10, 90) for i in range(5322)], f"big{k}", bars=999) for k in range(9)]
    assert (len(big[0]) + 64 + 4095) // 4096 == 8 and len(big[0]) + 6 + 64 <= 8 * 4096   # 8 sectors each, edited too: 9 fill 72 of 73
    steps = [*key(100, 9), (300, f"btn {BTN['SAVE']}"), (400, f"btn {BTN['HOME']}"), (500, f"btn {BTN['HOME']}"),
             (600, f"btn {BTN['HOME']}")]
    flash = tmp_path / "full.bin"
    (log, _), t0 = run(tmp_path, big, steps, 700, name="elibfull", env={"HOST_FLASH": str(flash)})
    assert (222, "LIBRARY FULL") in at(log, t0 + 300, t0 + 399)
    assert (4, "9 PIECES") in at(log, t0 + 500) and (28, "> big8") in at(log, t0 + 500)
    port = f"exec:{host_bin} {flash}"
    pid = [ln.split()[0].lstrip("#") for ln in fm1c("list", "--port", port).stdout.splitlines() if "big8" in ln][0]
    assert fm1c("pull", pid, "-o", tmp_path / "b8.f1p", "--port", port).returncode == 0
    assert (tmp_path / "b8.f1p").read_bytes() == big[8]                 # the old piece, byte for byte


def test_11_3_one_knob_turn_on_a_chord_is_one_undo(tmp_path):
    chord = [Note(0, 60, 24, 100), Note(0, 64, 24, 100), Note(0, 67, 24, 100)]
    steps = [(100, f"enc {ENC['KNOB2']} 1"), (200, f"btn {BTN['FX']}"), (300, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [song(chord)], steps, 400)
    assert (196, "C4  LEN 1/8  VEL 100") in at(log, t0 + 100, t0 + 199)
    assert (196, "C4  LEN 1/16  VEL 100") in at(log, t0 + 200, t0 + 299)       # all three back with one FX
    assert (222, "NOTHING TO UNDO") in at(log, t0 + 300)


def test_11_1_edit_while_playing_opens_at_the_bar_playing(tmp_path):
    lines, t = push(100, song(bars=4))
    lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 50 + 5500} btn {BTN['EDIT']}"]   # 5.5 s at 120 BPM: tick ~1056, bar 3
    log, _ = run_host(script(tmp_path, "eplay", lines, t + 5800))
    assert (4, "BAR 3  1/16") in at(log, t + 5550) and (222, "STOP TO EDIT") in at(log, t + 5550)
