"""US-23: sections (piece format v2): the table in both writers and the C reader, text2midi's markers fitted the same
by fm1c and Studio's engine, and the PLAYER's sections strip, launch and loop through host mode."""
import os, random, struct, sys
import pytest
from conftest import APP
from test_piece import cdump, rand_piece  # noqa: F401  (the C reader fixture)
from test_engine import BRIEFS, gen_track, engine_fit, fm1c_fit, meta, run, studio_cli, t2m_or_skip, vlq  # noqa: F401
from test_ui import BTN, at, push, rms, script
from conftest import run_host, screen_full, wav_frames

sys.path.insert(0, str(APP / "tools"))
from fm1c import fit as fitmod  # noqa: E402
from fm1c.piece import (BAD_SECT, BAD_VERSION, Channel, Note, Piece, PieceError, Section, decode, dump, encode,  # noqa: E402
                        section_name)

SNAMES = ["intro", "verse1", "chorus", "Verse 2", "bridge-long-name", "drop", "", "Café ☕", "outro", "BREAK", "a"]


def with_sections(p, rng):
    n = rng.choice([1, 1, 2, 5, 12, 24])
    starts = sorted(rng.sample(range(p.len_ticks), min(n, p.len_ticks)))
    p.sections = [Section(t, section_name(rng.choice(SNAMES))) for t in starts]
    return p


# ---- 23.1: the format ----
def test_23_1_c_reader_matches_python_on_1000_v2_pieces(tmp_path, cdump):
    rng = random.Random(231)
    paths, pieces = [], []
    for i in range(1000):
        p = with_sections(rand_piece(rng), rng)
        b = encode(p)
        assert b[4] == 2 and dump(decode(b)) == dump(p) and decode(b).sections == p.sections and encode(decode(b)) == b
        f = tmp_path / f"{i}.f1p"
        f.write_bytes(b)
        paths.append(f); pieces.append(p)
    outs = []
    for k in range(0, 1000, 250):
        outs += cdump(*paths[k:k + 250])
    for p, o in zip(pieces, outs):
        assert o == dump(p)
        assert ("\ns " in o) == bool(p.sections)


def test_23_1_no_sections_is_v1_byte_for_byte():
    rng = random.Random(2311)
    for _ in range(200):
        p = rand_piece(rng)
        b = encode(p)
        assert b[4] == 1 and decode(b).sections == []
        q = with_sections(decode(b), rng)
        b2 = encode(q)
        assert b2[4] == 2 and len(b2) == len(b) + 4 + 12 * len(q.sections) and b2[5:40] == b[5:40]
        q.sections = []
        assert encode(q) == b


def test_23_1_a_v2_piece_layout():
    p = Piece("sect", 1200, 8 * 384, [Channel("lead", [Note(0, 60, 96, 100)])], [Section(0, "INTRO"), Section(768, "VERSE1")])
    b = encode(p)
    assert b[4] == 2 and b[48 + 16] == 2 and b[48 + 16 + 1:48 + 16 + 4] == b"\0\0\0"
    assert b[68:80] == struct.pack("<I", 0) + b"INTRO\0\0\0" and b[80:92] == struct.pack("<I", 768) + b"VERSE1\0\0"
    assert struct.unpack_from("<I", b, 48 + 12)[0] == 92          # channel 1's events follow the table


# ---- 23.2: text2midi's markers ----
def marker_midi(path, markers, notes_end=8 * 1920, tpq=480, marker_tracks=1):
    """a conductor (tempo + markers [(tick, text bytes)]) split over marker_tracks tracks, and one lead track"""
    ms = sorted(markers, key=lambda m: m[0])
    tracks = []
    for k in range(marker_tracks):
        body, t = bytearray(meta(0x51, (500000).to_bytes(3, "big")) if k == 0 else b""), 0
        for tick, text in ms[k::marker_tracks]:
            body += vlq(tick - t) + b"\xff\x06" + vlq(len(text)) + text
            t = tick
        body += b"\x00\xff\x2f\x00"
        tracks.append(b"MTrk" + struct.pack(">I", len(body)) + bytes(body))
    lead = bytearray(meta(3, b"lead"))
    for i in range(0, notes_end, tpq):
        lead += vlq(0 if i == 0 else 10) + bytes([0x90, 60, 100]) + vlq(tpq - 10) + bytes([0x80, 60, 0])
    lead += b"\x00\xff\x2f\x00"
    tracks.append(b"MTrk" + struct.pack(">I", len(lead)) + bytes(lead))
    path.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 1, len(tracks), tpq) + b"".join(tracks))
    return path


def test_23_2_markers_become_the_table_in_both_fitters(studio_cli, tmp_path):
    f = marker_midi(tmp_path / "t2m.mid", [(0, b"intro"), (2 * 1920, b"verse1"), (4 * 1920, b"chorus"), (6 * 1920, b"outro")])
    piece, rep = fitmod.fit(str(f), "song")
    assert [(s.tick, s.name) for s in piece.sections] == [(0, "INTRO"), (768, "VERSE1"), (1536, "CHORUS"), (2304, "OUTRO")]
    assert not any("section" in ln for ln in rep.lines)
    want, got = fm1c_fit(f, "song", 6), engine_fit(studio_cli, f, "song", 6, tmp_path)
    assert want[0][4] == 2 and got == want


def test_23_2_text2midi_compose_carries_its_form(studio_cli, tmp_path):
    """fm1c compose (text2midi's t2m-cli) and Studio's make: the brief's form as the table, the same bytes"""
    import json
    t2m_or_skip()
    brief = BRIEFS / "funk-classic-1.json"
    a, b = tmp_path / "a", tmp_path / "b"
    a.mkdir(); b.mkdir()
    assert run(APP / "fm1c", "compose", brief, "--seed", 1, "-o", "x.f1p", cwd=a).returncode == 0
    assert run(studio_cli, "make", brief, "--seed", 1, "--name", "funk-classic-1", "-o", "x.f1p", cwd=b).returncode == 0
    data = (a / "x.f1p").read_bytes()
    assert data == (b / "x.f1p").read_bytes() and data[4] == 2
    form = json.loads(brief.read_text())["form"]
    p = decode(data)
    assert [s.name for s in p.sections] == [section_name(f["id"]) for f in form]
    bars = [0]
    for f in form[:-1]:
        bars.append(bars[-1] + f["bars"])
    assert [s.tick for s in p.sections] == [384 * x for x in bars]


def test_23_2_no_markers_no_table_same_bytes_as_before(studio_cli, tmp_path):
    f = marker_midi(tmp_path / "plain.mid", [])
    piece, _ = fitmod.fit(str(f), "song")
    want, got = fm1c_fit(f, "song", 6), engine_fit(studio_cli, f, "song", 6, tmp_path)
    assert piece.sections == [] and want[0][4] == 1 and got == want
    piece.sections = []
    assert want[0] == encode(piece)


@pytest.mark.parametrize("case", ["unsorted over tracks", "same tick", "past the end", "30 markers", "odd names"])
def test_23_2_hostile_markers_fit_the_same(studio_cli, tmp_path, case):
    ms = {"unsorted over tracks": [(t * 1920, b"s%d" % t) for t in (5, 1, 3, 0, 2)],
          "same tick": [(0, b"a"), (0, b"b"), (1920, b"c"), (2400, b"d")],
          "past the end": [(0, b"intro"), (8 * 1920, b"end"), (50 * 1920, b"far")],
          "30 markers": [(t * 480, b"m%d" % t) for t in range(30)],
          "odd names": [(0, b""), (480, "Café ☕".encode()), (960, b"\xe9t\xe9"), (1440, b"   "), (1920, b"a very long name")]}[case]
    f = marker_midi(tmp_path / "m.mid", ms, marker_tracks=2 if case == "unsorted over tracks" else 1)
    want, got = fm1c_fit(f, "song", 6), engine_fit(studio_cli, f, "song", 6, tmp_path)
    assert got == want
    p = decode(want[0])
    ticks = [s.tick for s in p.sections]
    assert ticks == sorted(set(ticks)) and len(ticks) <= 24 and all(t < p.len_ticks for t in ticks)
    assert all(1 <= len(s.name) <= 8 and s.name == s.name.upper() for s in p.sections)
    if case == "30 markers":
        assert len(ticks) == 24 and "6 sections after the 24th dropped" in want[1]
    if case == "past the end":
        assert ticks == [0] and "2 section markers past the end dropped" in want[1]
    if case == "same tick":
        assert [s.name for s in p.sections] == ["A", "C", "D"]


def test_23_2_random_midi_with_markers_fit_as_fm1c_does(studio_cli, tmp_path):
    rng = random.Random(232)
    same = 0
    for i in range(80):
        tpq = rng.choice([96, 120, 480, 960])
        ms = sorted((rng.randint(0, tpq * 4 * rng.choice([4, 40, 400])), rng.choice(SNAMES).encode(rng.choice(["utf-8", "latin-1"]), "replace"))
                    for _ in range(rng.choice([0, 1, 3, 10, 30])))
        body, t = bytearray(), 0
        for tick, text in ms:
            body += vlq(tick - t) + b"\xff\x06" + vlq(len(text)) + text
            t = tick
        body += b"\x00\xff\x2f\x00"
        cond = b"MTrk" + struct.pack(">I", len(body)) + bytes(body)
        trs = [cond] + [gen_track(rng, tpq) for _ in range(rng.choice([1, 2, 4]))]
        p = tmp_path / f"r{i}.mid"
        p.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 1, len(trs), tpq) + b"".join(trs))
        want, got = fm1c_fit(p, "song", 6), engine_fit(studio_cli, p, "song", 6, tmp_path)
        assert got == want, f"file {i}"
        same += want is not None and want[0][4] == 2
    assert same > 20, same


# ---- 23.4: the PLAYER ----
def sect_song():
    """240 BPM (a bar a second): INTRO bars 1-2 and CHORUS1 bars 7-8 play every beat; VERSE1 (bars 3-6) is silent"""
    notes = [Note(t * 96, 60, 80, 110) for t in range(32) if t < 8 or t >= 24]
    return encode(Piece("sections", 2400, 8 * 384, [Channel("lead", notes)],
                        [Section(0, "INTRO"), Section(768, "VERSE1"), Section(2304, "CHORUS1")]))


def keys(ms, k):
    return [(ms, f"keys {1 << k}"), (ms + 40, "keys 0")]


def test_23_4_player_sections_strip_launch_loop_screen_and_wav(tmp_path):
    lines, t = push(100, sect_song())
    steps = [(50, f"btn {BTN['PLAY']}"),               # LIBRARY: opens and plays from INTRO
             *keys(550, 14),                            # F#4 + 1: VERSE1, at the next bar line
             (900, f"btn {11}"),                        # SEQ: loop the playing section
             (8600, f"btn {11}")]                       # SEQ: loop off: CHORUS1 follows the verse
    lines += [f"{t + ms} {what}" for ms, what in steps]
    log, wav = run_host(script(tmp_path, "s23four", lines, t + 11500))
    s = at(log, t + 50)
    full = [(ms - t, y, x, c, txt) for ms, y, x, c, txt in screen_full(log) if ms >= t + 50]
    strip = {txt for _, y, _, _, txt in full if y == 202}
    assert {"1 INTRO", "2 VERSE1", "3 CHORUS1", "LOOP"} <= strip, strip
    assert (26, "INTRO 1:1") in s and (26, "VERSE1 3:2") in s, [x for x in s if x[0] == 26][:20]
    assert (222, "NEXT VERSE1") in s and (222, "SECTION LOOP ON") in s and (222, "SECTION LOOP OFF") in s
    assert any(y == 202 and txt == "2 VERSE1" and c == 0xFD20 for _, y, _, c, txt in full), "the playing one in amber"
    assert (64, "LEAD") in s
    f = wav_frames(wav)
    base = (t + 50) / 1000
    assert rms(f, base + 0.1, base + 0.9) > 300                         # INTRO
    assert rms(f, base + 2.8, base + 8.5) < 5                           # the silent verse (after the reverb), looped past bar 7
    assert rms(f, base + 9.2, base + 10.8) > 300                        # loop off: CHORUS1 after the verse


def test_23_4_a_v1_piece_keeps_its_player_screen(tmp_path):
    from test_ui import piece
    lines, t = push(100, piece("plain", nch=2))
    lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 300} btn 11", *[f"{t + ms} {w}" for ms, w in keys(400, 14)]]
    log, _ = run_host(script(tmp_path, "s23v1", lines, t + 1500))
    s = at(log, t + 50)
    assert (64 + 19, "BASS") in s and not any(y == 202 for y, _ in s) and (222, "NO SECTIONS") in s


# ---- 23.5: hostile tables ----
@pytest.mark.parametrize("secs", [
    [Section(0, "A")] * 2, [Section(384, "A"), Section(0, "B")], [Section(8 * 384, "END")], [Section(-1, "A")],
    [Section(i, "S") for i in range(25)], [Section(0, "")], [Section(0, "NINECHARS")], [Section(0, "\x07")]],
    ids=["0-length", "unsorted", "past the end", "negative", "25", "empty name", "9 chars", "control"])
def test_23_5_hostile_tables_refused_by_the_writer(secs):
    p = Piece("x", 1200, 8 * 384, [Channel("lead", [Note(0, 60, 9, 99)])], secs)
    with pytest.raises(PieceError) as e:
        encode(p)
    assert e.value.code == BAD_SECT


def test_23_5_hostile_v2_bytes_same_verdict_in_c_and_python(tmp_path, cdump):
    import zlib
    good = bytearray(encode(Piece("x", 1200, 8 * 384, [Channel("lead", [Note(0, 60, 9, 99)])],
                                  [Section(0, "A"), Section(384, "B")])))

    def fix(b):
        b = bytearray(b); b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF); return bytes(b)

    def put(off, data):
        b = bytearray(good); b[off:off + len(data)] = data; return fix(b)
    cases = {"count 0": put(64, b"\0"), "count 25": put(64, b"\x19"), "reserved": put(67, b"\1"),   # (US-24: 65, 66 the key)
             "same start": put(80, struct.pack("<I", 0)), "unsorted": put(68, struct.pack("<I", 500)),
             "past the end": put(80, struct.pack("<I", 8 * 384)), "empty name": put(72, b"\0"),
             "NUL then a letter": put(73, b"\0Z"), "control": put(72, b"\x1f"), "version 3": put(4, b"\3"),
             "v1 bytes marked v2": fix(bytes(good[:4]) + b"\2" + bytes(encode(Piece("x", 1200, 768, [Channel("lead", [])]))[5:]))}
    for name, data in cases.items():
        f = tmp_path / "h.f1p"
        f.write_bytes(data)
        with pytest.raises(PieceError) as e:
            decode(data)
        assert cdump(f)[0] == f"ERR {e.value.code}\n", name
        assert e.value.code == (BAD_VERSION if name == "version 3" else BAD_SECT), (name, e.value.code)


def test_23_5_decode_names_the_bad_channel_and_section():
    import zlib
    b = bytearray(encode(Piece("x", 1200, 768, [Channel("lead", [Note(0, 60, 9, 99)]), Channel("bass", [Note(0, 40, 9, 99)])],
                               [Section(0, "A"), Section(384, "B")])))
    b[48 + 32 + 28 + 6 + 5] = 0                                # channel 2's first note: velocity 0
    b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF)
    with pytest.raises(PieceError, match="^channel 2: bad note$"):
        decode(bytes(b))


# ---- 23.6: the real firmware in fm1-emu, on-chip timing ----

@pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_23_6_strict_timing_densest_piece_a_section_jump_every_bar(tmp_path):
    """the densest piece, one section a bar, SEQ looping the playing one (a jump back every bar) and a launch every
    ~bar, alternately the last section and the first: 10 s under on-chip timing, no empty audio half"""
    import re
    from test_emu import dense_sections, emu, replies, rms, usb_script
    d, ns = dense_sections()
    assert len(d) > 30000 and d[4] == 2 and ns >= 8
    usb, t = usb_script([d], gap=60)
    keys = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^", f"{t + 400}:SEQ", f"{t + 450}:SEQ^"]
    for i, at in enumerate(range(t + 900, t + 10500, 1700)):
        k = 13 + (min(ns, 14) - 1 if i % 2 == 0 else 0)      # n13 = F#4: section 1 (14 keys: sections 1..14)
        keys += [f"{at}:n{k}", f"{at + 60}:n{k}^"]
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=",".join(keys), strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert rms(s, len(s) // 2 - 5 * 44100) > 300, "the densest piece, jumping, heard"
