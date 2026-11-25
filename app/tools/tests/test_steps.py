"""US-25: the step sequencer. A channel's loop length and swing in the piece (both writers' model, the C reader),
SLOOP's swing formula copied unchanged, and the STEPS screen through host mode: keys, knobs, the moving step, SAVE,
reboot, the same pattern. The grid, polymeter, swing timing and edits while playing are tests/steps_test.c."""
import os, random, re, shutil, struct, sys, zlib
import pytest
from conftest import APP, run_host, screen, screen_full, wav_frames
from test_piece import cdump, rand_piece  # noqa: F401  (the C reader fixture)
from test_ui import push, script, at, rms, shot
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c
from test_live import goertzel, hz

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import BAD_TABLE, Channel, Note, Piece, PieceError, Section, decode, dump, encode  # noqa: E402

BTN = {"FX": 2, "LFO": 5, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "ALGORITHM": 1, "KNOB1": 2, "KNOB2": 3, "KNOB3": 4, "KNOB4": 5}
SLOOP_SEQ_BLOB = "379606c94bf2cbe2fa8ed2bcc909b22891f32782"      # firmware/src/seq.c @ bac9c05
AMBER = (255 >> 3) << 11 | (165 >> 2) << 5


def with_steps(p, rng):
    for c in p.channels:
        c.steps, c.swing = rng.choice([0, 0, 1, 12, 16, 64, rng.randint(0, 64)]), rng.choice([0, 0, 60, 100, rng.randint(0, 100)])
    return p


# ---- 25.1 / 25.2 / 25.3: loop length and swing in the piece ----
def test_25_1_c_reader_matches_python_with_loop_lengths_and_swing(tmp_path, cdump):
    rng = random.Random(251)
    paths, pieces = [], []
    for i in range(400):
        p = with_steps(rand_piece(rng), rng)
        if i % 3 == 0:
            p.sections = [Section(0, "A")]
        b = encode(p)
        assert dump(decode(b)) == dump(p) and encode(decode(b)) == b
        for k, c in enumerate(p.channels):
            assert b[48 + 16 * k + 5:48 + 16 * k + 8] == bytes([c.steps, c.swing, 0])
        f = tmp_path / f"{i}.f1p"
        f.write_bytes(b)
        paths.append(f); pieces.append(p)
    outs = []
    for k in range(0, 400, 200):
        outs += cdump(*paths[k:k + 200])
    for p, o in zip(pieces, outs):
        assert o == dump(p)
        assert ("\nt " in o) == any(c.steps or c.swing for c in p.channels)


def test_25_1_a_piece_without_loops_or_swing_keeps_its_bytes():
    rng = random.Random(2512)
    for _ in range(100):
        p = rand_piece(rng)
        b = encode(p)
        q = with_steps(decode(b), rng)
        q.channels[0].steps = 5
        b2 = encode(q)
        assert len(b2) == len(b) and b2 != b
        for c in q.channels:
            c.steps = c.swing = 0
        assert encode(q) == b and "\nt " not in dump(q)
    tiny = encode(Piece("x", 1200, 384, [Channel("lead")]))
    assert len(tiny) == 68 and decode(tiny).channels[0].notes == []        # the smallest piece: one empty channel


def test_25_2_loop_length_saved_with_the_piece():
    p = Piece("poly", 1200, 4 * 384, [Channel("bass", [Note(0, 40, 24, 100)], steps=12), Channel("drums", [], steps=16)])
    b = encode(p)
    assert b[48 + 5] == 12 and b[64 + 5] == 16
    q = decode(b)
    assert [c.steps for c in q.channels] == [12, 16] and "t 12 0\n" in dump(q)
    for bad in (65, -1):
        p.channels[0].steps = bad
        with pytest.raises(PieceError) as e:
            encode(p)
        assert e.value.code == BAD_TABLE


def test_25_3_swing_saved_with_the_piece():
    p = Piece("swing", 1200, 384, [Channel("lead", [Note(24, 60, 24, 100)], swing=60)], [Section(0, "A")], (2, 3))
    b = encode(p)
    assert b[48 + 6] == 60 and b[4] == 2
    q = decode(b)
    assert q.channels[0].swing == 60 and q.key == (2, 3) and "t 0 60\n" in dump(q)
    p.channels[0].swing = 101
    with pytest.raises(PieceError) as e:
        encode(p)
    assert e.value.code == BAD_TABLE


def test_25_3_sloops_swing_formula_copied_unchanged():
    """player.c's SLOOP block is seq.c @ bac9c05 lines 396-400 (swing_units), line for line, with its provenance row"""
    from test_fm6 import blob, sloop_src
    pl = (APP / "firmware/src/player.c").read_text()
    body = pl[pl.index("/* ---- SLOOP firmware/src/seq.c @ bac9c05, unchanged ---- */"):pl.index("/* ---- end of SLOOP's code")]
    prov = (APP / "firmware/SLOOP-PROVENANCE.md").read_text()
    assert re.search(r"^\| app/firmware/src/player\.c \| firmware/src/seq\.c \(swing_units\) \| " + SLOOP_SEQ_BLOB + r" \|", prov, re.M)
    assert "* u / 200u" in body
    src = sloop_src()
    if src is None:
        pytest.skip("SLOOP's source is not here (research/sloop-src or $SLOOP_SRC)")
    assert blob(src / "firmware/src/seq.c") == SLOOP_SEQ_BLOB
    seq = (src / "firmware/src/seq.c").read_text().splitlines()
    assert body.splitlines()[1:] == seq[395:400]


# ---- 25.4: the STEPS screen, host mode ----
ROLES8 = ["lead", "bass", "drums", "pad", "chords", "counter", "arp", "fx"]


def blank(name="pat"):
    return encode(Piece(name, 1200, 4 * 384, [Channel(r) for r in ROLES8]))


def steps_run(tmp_path, data, steps, end, name, env=None):
    """push, open (it plays), stop, LFO: STEPS; then steps [(ms after, what)]: ((log, wav), t0)"""
    lines, t = push(100, data)
    lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 150} btn {BTN['PLAY']}", f"{t + 250} btn {BTN['LFO']}"]
    t0 = t + 300
    lines += [f"{t0 + ms} {what}" for ms, what in steps]
    return run_host(script(tmp_path, name, lines, t0 + end), env=env), t0


def keys(ms, *k):
    return [(ms, f"keys {sum(1 << x for x in k)}"), (ms + 60, "keys 0")]


def test_25_4_steps_screen_keys_knobs_moving_step_save_reboot_same_pattern(tmp_path, host_bin):
    flash = str(tmp_path / "flash.bin")
    steps = [*keys(50, 7),                                   # C4 at step 1 (length 1, velocity 100)
             (150, f"enc {ENC['KNOB2']} 1"), (200, f"enc {ENC['KNOB3']} -5"),   # its length 2 steps, velocity 80
             (250, f"enc {ENC['SELECT']} 4"),                # step 5
             *keys(300, 11, 14),                             # E4 G4
             *keys(400, 4, 9),                               # + A3 D4: 4 notes
             *keys(500, 18),                                 # B4: a 5th, refused
             *keys(600, 4, 9),                               # A3 D4 away again
             (700, f"enc {ENC['KNOB1']} 1"),                 # channel 2 (BASS)
             *keys(750, 0),                                  # F3 at step 5
             (850, f"enc {ENC['KNOB4']} 12"),                # loop 12 steps
             (900, f"enc {ENC['ALGORITHM']} 60"),            # swing 60
             (950, "shot"),
             (1000, f"btn {BTN['FX']}"),                     # undo: the swing
             (1050, f"enc {ENC['ALGORITHM']} 60"),
             (1100, f"btn {BTN['PLAY']}"), (3600, f"btn {BTN['PLAY']}"),
             (3700, f"btn {BTN['SAVE']}")]
    (log, wav), t0 = steps_run(tmp_path, blank(), steps, 3900, "steps1", env={"HOST_FLASH": flash})
    s0 = at(log, t0 - 50, t0 + 40)
    assert (4, "STEPS 1 LEAD") in s0 and (4, "1-16") in s0 and (166, "STEP 1  -") in s0
    assert (184, "LEN 1  VEL 100") in s0 and (202, "LOOP OFF  SWING 0") in s0
    assert all((24 + 17 * k, str(k + 1)) in s0 for k in range(8)), "8 tracks"
    assert (166, "STEP 1  C4") in at(log, t0 + 50, t0 + 140) and (4, "STEPS 1 LEAD *") in at(log, t0 + 50, t0 + 140)
    assert (184, "LEN 2  VEL 80") in at(log, t0 + 200, t0 + 240)
    assert (166, "STEP 5  E4 G4") in at(log, t0 + 300, t0 + 390)
    assert (166, "STEP 5  A3 D4 E4 G4") in at(log, t0 + 400, t0 + 490)
    assert (222, "4 NOTES A STEP") in at(log, t0 + 500, t0 + 590)
    assert (166, "STEP 5  E4 G4") in at(log, t0 + 600, t0 + 690)
    assert (4, "STEPS 2 BASS *") in at(log, t0 + 700, t0 + 740) and (166, "STEP 5  F3") in at(log, t0 + 750, t0 + 840)
    assert (202, "LOOP 12  SWING 0") in at(log, t0 + 850, t0 + 890) and (202, "LOOP 12  SWING 60") in at(log, t0 + 900, t0 + 990)
    assert (222, "UNDONE") in at(log, t0 + 1000, t0 + 1040) and (202, "LOOP 12  SWING 0") in at(log, t0 + 1000, t0 + 1040)
    img = shot("steps1", t0 + 950)
    cell = lambda row, col: img.getpixel((24 + col * 13 + 6, 24 + row * 17 + 8))   # noqa: E731
    amber = lambda px: px[0] > 200 and 130 < px[1] < 200 and px[2] < 60          # noqa: E731
    assert amber(cell(0, 0)) and cell(0, 1) == (123, 121, 123) and not amber(cell(0, 2)), "C4: a start, then its tie"
    assert amber(cell(0, 4)) and amber(cell(1, 4)) and cell(1, 6) != cell(1, 4) and cell(2, 4)[0] < 80
    plays = [int(txt.split()[1]) for ms, y, x, c, txt in screen_full(log)
             if t0 + 1100 <= ms <= t0 + 3600 and y == 202 and x == 160 and txt.startswith("PLAY ")]
    assert plays[:13] == list(range(1, 13)) + [1], f"the BASS's 12-step loop moving: {plays[:16]}"
    assert max(plays) == 12 and len(plays) >= 18
    assert (222, "SAVED") in at(log, t0 + 3700)
    first = wav_frames(wav)

    log2, wav2 = run_host(script(tmp_path, "steps2", ["100 btn 12"], 3000), env={"HOST_FLASH": flash})
    assert (28, "> pat") in at(log2, 0, 99) and (222, "PLAYING") in at(log2, 100)
    fl = tmp_path / "xflash.bin"
    shutil.copy(flash, fl)
    port = f"exec:{host_bin} {fl}"
    pid = fm1c("list", "--port", port).stdout.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "back.f1p", "--port", port).returncode == 0
    chans = [Channel(r) for r in ROLES8]
    chans[0].notes = [Note(0, 60, 48, 80), Note(96, 64, 48, 80), Note(96, 67, 48, 80)]
    chans[1].notes, chans[1].steps, chans[1].swing = [Note(96, 53, 48, 80)], 12, 60
    assert (tmp_path / "back.f1p").read_bytes() == encode(Piece("pat", 1200, 4 * 384, chans)), "the saved piece: encode()'s bytes"
    second = wav_frames(wav2)
    for f, p0 in ((first, (t0 + 1100) / 1000), (second, 0.1)):   # the same pattern, before and after the reboot
        g4 = goertzel(f, p0 + 0.52, p0 + 0.72, hz(67))            # (the LEAD is monophonic: G4, the top note)
        assert g4 > 100 * goertzel(f, p0 + 0.03, p0 + 0.22, hz(67)), "G4 at step 5"
        assert goertzel(f, p0 + 0.03, p0 + 0.22, hz(60)) > 8 * goertzel(f, p0 + 0.52, p0 + 0.72, hz(60)), "C4 at step 1"
        f3 = goertzel(f, p0 + 2.02, p0 + 2.2, hz(53))
        assert f3 > 100 * goertzel(f, p0 + 1.4, p0 + 1.9, hz(53)) and f3 > 100 * goertzel(f, p0 + 0.03, p0 + 0.22, hz(53)), \
            "F3 at step 5, again at step 17: the 12-step loop"
        assert goertzel(f, p0 + 2.02, p0 + 2.2, hz(67)) * 100 < g4, "the lead does not loop"


def test_25_4_steps_start_at_the_playing_section_pages_of_16(tmp_path):
    lead = [Note(0, 60, 24, 100), Note(768, 72, 24, 100), Note(768 + 16 * 24, 76, 24, 100)]
    p = encode(Piece("secs", 1200, 8 * 384, [Channel("lead", lead), Channel("bass")], [Section(0, "A"), Section(768, "B")]))
    lines, t = push(100, p)
    steps = [(50, f"btn {BTN['PLAY']}"), *keys(100, 14),          # opens, plays; F#4 + 1: section B at the next bar
             (2200, f"btn {BTN['LFO']}"), (2300, f"btn {BTN['PLAY']}"), (2400, f"enc {ENC['SELECT']} 16"), (2500, "shot")]
    lines += [f"{t + ms} {what}" for ms, what in steps]
    log, _ = run_host(script(tmp_path, "stsec", lines, t + 2600))
    assert (166, "STEP 1  C5") in at(log, t + 2200, t + 2390), "from B's start"
    assert (4, "17-32") in at(log, t + 2400) and (166, "STEP 17  E5") in at(log, t + 2400)
    img = shot("stsec", t + 2500)
    px = img.getpixel((24 + 6, 24 + 8))
    assert px[0] > 200 and 130 < px[1] < 200 and px[2] < 60, "page 2's first step: E5, 16 steps after B's start"


# ---- 25.5: hostile ----
def test_25_5_hostile_bytes_same_verdict_in_c_and_python(tmp_path, cdump):
    good = bytearray(encode(Piece("x", 1200, 384, [Channel("lead", [Note(0, 60, 9, 99)]), Channel("bass", [])])))

    def put(off, v):
        b = bytearray(good); b[off] = v; b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF); return bytes(b)
    for off, v, ok in ((64 + 5, 65, False), (64 + 6, 101, False), (64 + 7, 128, False),   # (US-30: byte 7 = glide 0..127)
                       (64 + 5, 64, True), (64 + 6, 100, True),
                       (48 + 5, 255, False)):
        f = tmp_path / "h.f1p"
        f.write_bytes(put(off, v))
        if ok:
            assert cdump(f)[0] == dump(decode(f.read_bytes()))
            continue
        with pytest.raises(PieceError) as e:
            decode(f.read_bytes())
        assert e.value.code == BAD_TABLE and cdump(f)[0] == f"ERR {BAD_TABLE}\n", (off, v)


def test_25_5_steps_full_piece_refused_with_a_message_nothing_lost(tmp_path, host_bin):
    notes = [Note(2000 + i, 60 + i % 12, 10, 90) for i in range((32768 - 68) // 6)]
    full = encode(Piece("full", 1200, 999 * 384, [Channel("lead", notes)]))
    assert len(full) == 32768
    flash = tmp_path / "full.bin"
    steps = [*keys(50, 7), (150, f"enc {ENC['KNOB4']} 70"), (200, f"enc {ENC['ALGORITHM']} 120"),
             (250, f"enc {ENC['KNOB2']} 100"), (300, f"btn {BTN['SAVE']}")]
    (log, _), t0 = steps_run(tmp_path, full, steps, 400, "stfull", env={"HOST_FLASH": str(flash)})
    assert (222, "PIECE FULL") in at(log, t0 + 50, t0 + 140) and (166, "STEP 1  -") in at(log, t0 + 50, t0 + 140)
    assert (202, "LOOP 64  SWING 0") in at(log, t0 + 150, t0 + 190), "loop 70 -> 64 at most (a byte: no room needed)"
    assert (202, "LOOP 64  SWING 100") in at(log, t0 + 200, t0 + 240), "swing 120 -> 100"
    assert (184, "LEN 64  VEL 100") in at(log, t0 + 250, t0 + 290), "length 100 -> 64 steps"
    port = f"exec:{host_bin} {flash}"
    pid = fm1c("list", "--port", port).stdout.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "b.f1p", "--port", port).returncode == 0
    back = decode((tmp_path / "b.f1p").read_bytes())
    assert back.channels[0].notes == notes and (back.channels[0].steps, back.channels[0].swing) == (64, 100)


# ---- 25.6: the real firmware in fm1-emu, on-chip timing ----
@pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_25_6_strict_timing_8_tracks_64_steps_chords_swing_60(tmp_path):
    """all 8 tracks at 64 steps (a loop of 64 in an 8-bar piece) with a 4-note chord on every step, swing 60, plus
    the notes after them: 10 s under on-chip timing, no empty audio half"""
    from test_emu import emu, replies, rms as erms, usb_script
    chans = []
    for c, role in enumerate(ROLES8):
        notes = [Note(s * 24, 36 + c * 6 + k * 4 + s % 5, 20, 100) for s in range(64) for k in range(4)]
        notes += [Note(64 * 24 + i * 12, 40 + i % 30, 10, 90) for i in range(100)]
        chans.append(Channel(role, notes, steps=64, swing=60))
    d = encode(Piece("steps64", 1500, 8 * 384, chans))
    assert len(d) > 16000
    usb, t = usb_script([d], gap=60)
    keys_ = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^"]
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=",".join(keys_), strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert erms(s, len(s) // 2 - 5 * 44100) > 300, "8 tracks of 4-note chords, swung, heard"


# ---- the modules STEPS shares, where its own tests do not reach ----
def test_edit_plays_from_the_cursors_bar(tmp_path):
    """EDIT's PLAY starts at the bar the cursor is in (STEPS' PLAY starts at the piece's or the section's start)"""
    from test_edit_ui import run as edit_run
    p = encode(Piece("bar2", 1200, 4 * 384, [Channel("lead", [Note(384, 72, 48, 100)])]))
    (log, wav), t0 = edit_run(tmp_path, [p], [(50, f"enc {ENC['SELECT']} 17"), (100, f"btn {BTN['PLAY']}")], 600, name="ebar")
    assert (222, "PLAYING FROM THIS BAR") in at(log, t0 + 100)
    f = wav_frames(wav)
    b = (t0 + 100) / 1000
    assert goertzel(f, b + 0.02, b + 0.2, hz(72)) > 100 * goertzel(f, b - 0.3, b - 0.05, hz(72)), "bar 2's note at once"
