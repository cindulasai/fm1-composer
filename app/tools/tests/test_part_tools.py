"""US-31: EDIT's part tools through host mode: hold EDIT, turn KNOB1 SHIFT / KNOB2 X2 1/2 / KNOB3 TRANSPOSE; the roll
and the footer show it, FX takes the whole hold back, SAVE keeps it (the piece read back with the real fm1c)."""
import re, shutil, sys
from conftest import APP, run_host
from test_ui import script, at, shot
from test_edit_ui import BTN, ENC, run, song, cell, amber, key
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Piece, Channel, Note, encode, decode  # noqa: E402

E = BTN["EDIT"]
FOOT = 222
FOUR = [Note(0, 60, 24, 100), Note(96, 64, 24, 90), Note(192, 67, 24, 80), Note(400, 62, 24, 70)]   # bars 1-2


def foot(log, a, b=10 ** 9):
    return [t for y, t in at(log, a, b) if y == FOOT]


def test_31_2_hold_edit_turn_knobs_roll_footer_one_undo(tmp_path):
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB1']} 1"), (300, "shot"),
             (400, f"enc {ENC['KNOB3']} -3"), (500, "shot"), (600, f"enc {ENC['KNOB2']} 1"), (700, "shot"),
             (750, f"enc {ENC['KNOB2']} -1"), (800, f"btnup {E}"), (900, f"enc {ENC['KNOB1']} 1"),
             (1000, f"btn {BTN['FX']}"), (1100, "shot"), (1200, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [song(FOUR, bars=4)], steps, 1300)
    assert "SHIFT  X2 1/2  TRANSPOSE" in foot(log, t0 + 100, t0 + 199)          # the press says what the knobs do
    assert "SHIFT +1" in foot(log, t0 + 200, t0 + 299)
    a = shot(tmp_path.name, t0 + 300)
    assert amber(cell(a, 1, 60)) and not amber(cell(a, 0, 60)) and amber(cell(a, 5, 64))   # every note a step later
    assert "TRANSPOSE -3" in foot(log, t0 + 400, t0 + 499)
    b = shot(tmp_path.name, t0 + 500)
    assert amber(cell(b, 1, 57)) and not amber(cell(b, 1, 60)) and amber(cell(b, 5, 61))   # 3 semitones down
    assert "X2" in foot(log, t0 + 600, t0 + 699) and "1/2" in foot(log, t0 + 750, t0 + 799)
    assert (4, "EDIT 1 LEAD *") in at(log, t0 + 200, t0 + 299)
    assert (4, "BAR 1  1/8") in at(log, t0 + 900, t0 + 999), "let go: KNOB1 is the grid again"
    assert "UNDONE" in foot(log, t0 + 1000, t0 + 1099)
    c = shot(tmp_path.name, t0 + 1100)
    assert amber(cell(c, 0, 60)) and amber(cell(c, 2, 64))                      # (grid 1/8 now: C4 step 0, E4 step 2)
    assert not amber(cell(c, 0, 57)) and not amber(cell(c, 1, 57))
    assert "NOTHING TO UNDO" in foot(log, t0 + 1200), "the whole hold was one undo"


def test_31_2_save_keeps_the_tools_reread_from_flash(tmp_path, host_bin):
    flash = str(tmp_path / "flash.bin")
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB3']} 2"), (300, f"enc {ENC['KNOB1']} -1"),
             (400, f"enc {ENC['KNOB2']} 1"), (500, f"btnup {E}"), (600, f"btn {BTN['SAVE']}")]
    (log, _), t0 = run(tmp_path, [song(FOUR, bars=4)], steps, 900, name="tsave", env={"HOST_FLASH": flash})
    assert "SAVED" in foot(log, t0 + 600)
    fl = tmp_path / "xflash.bin"
    shutil.copy(flash, fl)
    port = f"exec:{host_bin} {fl}"
    lst = fm1c("list", "--port", port).stdout
    pid = lst.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "back.f1p", "--port", port).returncode == 0
    got = decode((tmp_path / "back.f1p").read_bytes()).channels[0].notes
    # +2 semitones, a 1/16 earlier wrapping in bars 1-2 (768 ticks), then x2 up to the 4 bars
    part = sorted(Note((n.tick - 24) % 768, n.pitch + 2, n.dur, n.vel) for n in FOUR)
    assert sorted(got) == sorted(part + [Note(n.tick + 768, n.pitch, n.dur, n.vel) for n in part])


def test_31_3_refusals_empty_drums_playing(tmp_path):
    drums = Channel("drums", [Note(0, 36, 24, 100), Note(96, 38, 24, 100)])
    empty = encode(Piece("empty", 1200, 4 * 384, [Channel("lead", []), drums]))
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB1']} 1"), (300, f"enc {ENC['KNOB2']} 1"), (400, f"btnup {E}")]
    (log, _), t0 = run(tmp_path, [empty], steps, 500, name="tempty")
    assert "EMPTY CHANNEL" in foot(log, t0 + 200, t0 + 299) and "EMPTY CHANNEL" in foot(log, t0 + 300, t0 + 399)
    assert (4, "EDIT 1 LEAD *") not in at(log, t0)                                  # (never marked: nothing changed)
    dr = encode(Piece("dr", 1200, 4 * 384, [drums]))
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB3']} 1"), (300, "shot"), (400, f"enc {ENC['KNOB1']} 1"),
             (500, f"btnup {E}"), (600, f"btn {BTN['PLAY']}"), (700, f"btndown {E}"), (800, f"enc {ENC['KNOB3']} 1"),
             (900, f"btnup {E}"), (1000, f"btn {BTN['PLAY']}"), (1100, f"btn {BTN['FX']}"), (1200, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [dr], steps, 1300, name="tdrums")
    assert "DRUMS DO NOT TRANSPOSE" in foot(log, t0 + 200, t0 + 299)
    assert "SHIFT +1" in foot(log, t0 + 400, t0 + 499)                              # drums shift
    assert "STOP TO EDIT" in foot(log, t0 + 800, t0 + 899)                          # tools while playing: refused
    assert "UNDONE" in foot(log, t0 + 1100, t0 + 1199) and "NOTHING TO UNDO" in foot(log, t0 + 1200)


def test_31_3_fifty_turns_one_hold_one_undo_and_full_piece(tmp_path):
    steps = [(100, f"btndown {E}")]
    for i in range(50):
        steps.append((200 + i * 20, f"enc {ENC['KNOB' + str(1 + i % 3)]} {1 if i % 2 else -1}"))
    steps += [(1300, f"btnup {E}"), (1400, f"btn {BTN['FX']}"), (1500, "shot"), (1600, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [song(FOUR, bars=4)], steps, 1700, name="t50")
    assert "UNDONE" in foot(log, t0 + 1400, t0 + 1499) and "NOTHING TO UNDO" in foot(log, t0 + 1600)
    img = shot("t50", t0 + 1500)
    assert amber(cell(img, 0, 60)) and amber(cell(img, 4, 64)) and amber(cell(img, 8, 67))   # all back, one FX
    assert "Sanitizer" not in log
    full = song([Note(i, 60 + i % 12, 10, 90) for i in range((32768 - 68) // 6)], "full", bars=999)
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB2']} 1"), (300, f"btnup {E}"), (400, f"btn {BTN['FX']}")]
    (log, _), t0 = run(tmp_path, [full], steps, 500, name="tfull")
    assert "PART TOO BIG TO UNDO" in foot(log, t0 + 200, t0 + 299) or "PIECE FULL" in foot(log, t0 + 200, t0 + 299)
    assert "NOTHING TO UNDO" in foot(log, t0 + 400), "nothing half-done, nothing logged"
    small = Channel("bass", [Note(i * 24, 40, 12, 90) for i in range(64)])
    big = [Note(i, 60 + i % 12, 10, 90) for i in range((32768 - 84 - 64 * 6) // 6)]
    full2 = encode(Piece("full2", 1200, 999 * 384, [small, Channel("lead", big)]))
    assert len(full2) > 32768 - 12
    (log, _), t0 = run(tmp_path, [full2], steps, 500, name="tfull2")
    assert "PIECE FULL" in foot(log, t0 + 200, t0 + 299) and "NOTHING TO UNDO" in foot(log, t0 + 400)


def test_31_4_readme_documents_the_tools_as_the_screen_says(tmp_path):
    readme = (APP / "README.md").read_text()
    row = [ln for ln in readme.splitlines() if ln.startswith("| | EDIT held")]
    assert len(row) == 1, "README's EDIT table: one row for the tools"
    for knob in ("KNOB1", "KNOB2", "KNOB3", "FX"):
        assert knob in row[0]
    shown = re.findall(r"`([^`]+)`", row[0])
    src = (APP / "firmware/src/composer.c").read_text()
    for s in shown:                              # every footer string the README quotes is one the firmware writes
        base = re.sub(r"[+-]\d+$", "", s).strip()
        assert base and base in src, s
    for need in ("SHIFT +1", "X2", "1/2", "TRANSPOSE -3", "DRUMS DO NOT TRANSPOSE", "PIECE FULL", "EMPTY CHANNEL"):
        assert need in shown, need
    # and on screen: the strings of a hold
    steps = [(100, f"btndown {E}"), (200, f"enc {ENC['KNOB1']} 1"), (300, f"enc {ENC['KNOB2']} 1"),
             (400, f"enc {ENC['KNOB2']} -1"), (500, f"enc {ENC['KNOB3']} -3"), (600, f"btnup {E}")]
    (log, _), t0 = run(tmp_path, [song(FOUR, bars=4)], steps, 700, name="treadme")
    feet = foot(log, t0 + 100)
    for s in ("SHIFT +1", "X2", "1/2", "TRANSPOSE -3"):
        assert s in feet, s
