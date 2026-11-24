"""US-24: the live keyboard in the piece's key, auto-chords, MIDI IN. The key in the piece (both writers, the C reader,
text2midi's brief through fm1c compose and Studio's make), SLOOP's code as copied, and the PLAYER's KEYS mode and MIDI
IN through host mode (screen and WAV pitch). The C side (24.1-24.3, 24.5) is tests/live_test.c."""
import cmath, json, math, os, random, re, struct, subprocess, sys
import pytest
from conftest import APP, run_host, screen, wav_frames
from test_piece import cdump, rand_piece  # noqa: F401  (the C reader fixture)
from test_engine import BRIEFS, run, studio_cli, t2m_or_skip  # noqa: F401
from test_ui import push, rms, script

sys.path.insert(0, str(APP / "tools"))
from fm1c import fit as fitmod  # noqa: E402
from fm1c.piece import BAD_SECT, Channel, Note, Piece, PieceError, Section, decode, dump, encode  # noqa: E402

BTN = {"OCT-": 0, "OCT+": 1, "SEL": 3, "ENV": 4, "HOME": 8, "ARP": 10, "PLAY": 12}
ENC = {"KNOB3": 4, "KNOB4": 5}
SLOOP_SEQ_BLOB = "379606c94bf2cbe2fa8ed2bcc909b22891f32782"      # firmware/src/seq.c @ bac9c05


# ---- 24.1: the key in the piece ----
def test_24_1_key_in_the_sections_table_c_reader_matches_python(tmp_path, cdump):
    rng = random.Random(241)
    paths, pieces = [], []
    for i in range(300):
        p = rand_piece(rng)
        starts = sorted(rng.sample(range(p.len_ticks), min(rng.choice([1, 3, 24]), p.len_ticks)))
        p.sections = [Section(t, "S%d" % k) for k, t in enumerate(starts)]
        p.key = None if i % 5 == 0 else (rng.randrange(12), rng.randrange(16))
        b = encode(p)
        end = 48 + 16 * len(p.channels)
        assert b[end + 1:end + 4] == (bytes([p.key[0] + 1, p.key[1], 0]) if p.key else b"\0\0\0")
        assert decode(b).key == p.key and encode(decode(b)) == b and dump(decode(b)) == dump(p)
        f = tmp_path / f"{i}.f1p"
        f.write_bytes(b)
        paths.append(f); pieces.append(p)
    outs = cdump(*paths[:150]) + cdump(*paths[150:])
    for p, o in zip(pieces, outs):
        assert o == dump(p)
        assert ("\nk " in o) == (p.key is not None)


def test_24_1_v1_and_keyless_v2_pieces_unchanged():
    rng = random.Random(2412)
    for _ in range(100):
        p = rand_piece(rng)
        b = encode(p)
        p.key = (rng.randrange(12), rng.randrange(16))
        assert encode(p) == b and decode(b).key is None, "a v1 piece carries no key: the same bytes"
        assert "\nk " not in dump(p)
        p.sections, p.key = [Section(0, "A")], None
        v2 = encode(p)
        assert v2[4] == 2 and v2[48 + 16 * len(p.channels) + 1:48 + 16 * len(p.channels) + 4] == b"\0\0\0"
    for bad in [(12, 0), (-1, 1), (0, 16), (3,), "D"]:
        with pytest.raises(PieceError) as e:
            encode(Piece("k", 1200, 384, [Channel("lead", [Note(0, 60, 9, 99)])], [Section(0, "A")], bad))
        assert e.value.code == BAD_SECT


def test_24_1_hostile_key_bytes_same_verdict_in_c_and_python(tmp_path, cdump):
    import zlib
    good = bytearray(encode(Piece("x", 1200, 768, [Channel("lead", [Note(0, 60, 9, 99)])], [Section(0, "A")], (11, 15))))

    def put(off, data):
        b = bytearray(good); b[off:off + len(data)] = data
        b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF)
        return bytes(b)
    for name, data in {"root 13": put(65, b"\x0d"), "scale 16": put(66, b"\x10"), "scale, no key": put(65, b"\0"),
                       "reserved": put(67, b"\1")}.items():
        f = tmp_path / "h.f1p"
        f.write_bytes(data)
        with pytest.raises(PieceError) as e:
            decode(data)
        assert e.value.code == BAD_SECT and cdump(f)[0] == f"ERR {BAD_SECT}\n", name
    f = tmp_path / "g.f1p"
    f.write_bytes(bytes(good))
    assert "k 11 15\n" in cdump(f)[0]


def test_24_1_brief_key():
    k = fitmod.brief_key
    t = lambda tonic, system, tuning="12tet": {"tonality": {"tonic": tonic, "system": system, "tuning": tuning}}   # noqa: E731
    assert k(t("D", "mode.dorian")) == (2, 3) and k(t("Eb", "mode.minor")) == (3, 2) and k(t("F#", "mode.major")) == (6, 1)
    assert k(t("bb", "thaat.kalyan")) == (10, 9) and k(t(" C## ", "thaat.khamaj")) == (2, 4) and k(t("Cb", "thaat.bhairavi")) == (11, 8)
    assert k(t({"hz": 440}, "mode.major")) == (9, 1) and k(t({"hz": 277.0}, "thaat.kafi")) == (1, 3)
    assert k(t({"hz": 0}, "mode.minor")) == (0, 2) and k(t({"hz": -3}, "mode.minor")) == (0, 2) and k(t({"hz": float("inf")}, "mode.minor")) == (0, 2)
    assert k(t("D", "thaat.todi")) is None, "not one of SLOOP's 16: the FM-1 guesses it"
    assert k(t("D", "mode.dorian", "just")) is None and k(t("H", "mode.major")) is None and k(t("C#b", "mode.major")) is None
    assert k(t("C###", "mode.major")) is None and k(t("D", "mode.nope")) is None and k(t({"hz": True}, "mode.major")) is None
    assert k({}) is None and k([]) is None and k({"tonality": {"tonic": "D", "system": ["x"], "tuning": "12tet"}}) is None
    assert k(t(5, "mode.major")) is None and k(t("", "mode.major")) is None


@pytest.mark.parametrize("tonality,want", [(("D", "mode.minor"), (2, 2)), (({"hz": 370.0}, "thaat.kalyan"), (6, 9)),
                                           (("Bb", "mode.major"), (10, 1)), (("E", "thaat.todi"), None),
                                           (("B", "thaat.bhairavi"), (11, 8))],
                         ids=["D minor", "F# lydian (Hz)", "Bb major", "E todi (none)", "B phrygian"])
def test_24_1_text2midi_compose_carries_the_brief_key_same_bytes(studio_cli, tmp_path, tonality, want):
    """fm1c compose (text2midi's t2m-cli) and Studio's make: the brief's key in the table, the same bytes"""
    t2m_or_skip()
    brief = json.loads((BRIEFS / "funk-classic-1.json").read_text())
    brief["tonality"]["tonic"], brief["tonality"]["system"] = tonality
    f = tmp_path / "k.json"
    f.write_text(json.dumps(brief))
    a, b = tmp_path / "a", tmp_path / "b"
    a.mkdir(); b.mkdir()
    r = run(APP / "fm1c", "compose", f, "--seed", 3, "-o", "x.f1p", cwd=a)
    assert r.returncode == 0, r.stderr
    assert run(studio_cli, "make", f, "--seed", 3, "--name", "k", "-o", "x.f1p", cwd=b).returncode == 0
    data = (a / "x.f1p").read_bytes()
    assert data == (b / "x.f1p").read_bytes() and data[4] == 2
    assert decode(data).key == fitmod.brief_key(brief) == want


def test_24_1_sloops_code_copied_unchanged():
    """live.c's SLOOP blocks are seq.c @ bac9c05 line for line; the scale masks are the same in fit.py and lib.rs"""
    from test_fm6 import blob, sloop_src
    live = (APP / "firmware/src/live.c").read_text()
    masks = []
    for line in live[live.index("SCALE_MASK[] = {"):live.index("#define NSCALES")].splitlines()[1:-1]:
        bits = [int(x) for x in re.findall(r"1 << (\d+)", line)]
        masks.append(sum(1 << b for b in bits) if bits else int(re.search(r"0x([0-9A-F]+)", line)[1], 16))
    assert tuple(masks) == fitmod.SCALE_MASKS
    rs = (APP / "studio/engine/studio/src/lib.rs").read_text()
    m = re.search(r"SCALE_MASKS: \[u16; 16\] = \[([^\]]*)\]", rs)
    assert tuple(int(x, 16) for x in re.findall(r"0x([0-9A-F]+)", m[1])) == fitmod.SCALE_MASKS
    prov = (APP / "firmware/SLOOP-PROVENANCE.md").read_text()
    assert re.search(r"^\| app/firmware/src/live\.c \| firmware/src/seq\.c \(.*kb_pitch.*chord_play_notes.*midi_sel_set.*\) \| "
                     + SLOOP_SEQ_BLOB + r"( \+ [0-9a-f]{40})? \|", prov, re.M)
    assert "app/firmware/src/midi_uart.c | firmware/src/midi_uart.c | changed: 15 lines (US-12 SysEx) and SLOOP 2.8.3's midi_in_lost" in prov
    src = sloop_src()
    if src is None:
        pytest.skip("SLOOP's source is not here (research/sloop-src or $SLOOP_SRC); the masks are checked above")
    seq = (src / "firmware/src/seq.c").read_text().splitlines()
    assert blob(src / "firmware/src/seq.c") == SLOOP_SEQ_BLOB
    body = live[live.index("/* ---- SLOOP firmware/src/seq.c @ bac9c05, unchanged ---- */"):live.index("/* ---- end of SLOOP's code")]
    ours = [ln for ln in body.splitlines()[1:] if ln and not ln.startswith("/* COMPOSER:")]
    theirs = [ln for a, b in ((22, 40), (43, 43), (141, 144), (196, 236), (252, 393), (2922, 2935)) for ln in seq[a - 1:b] if ln]
    assert ours == theirs
    uart = (APP / "firmware/src/midi_uart.c").read_text()
    sl = (src / "firmware/src/midi_uart.c").read_text()
    assert "if ((um.st & 0xE0u) == 0x80u)\n                midi_in_lost = 1;" in uart and \
        "if ((um.st & 0xE0u) == 0x80u)\n                midi_in_lost = 1;" in sl


# ---- 24.4: the PLAYER, host mode ----
def goertzel(frames, s0, s1, f):
    """the power at f Hz in the left channel between s0 and s1 seconds"""
    seg = frames[int(s0 * 44100) * 2:int(s1 * 44100) * 2:2]
    w = 2 * math.pi * f / 44100
    c, q1, q2 = 2 * math.cos(w), 0.0, 0.0
    for x in seg:
        q1, q2 = x + c * q1 - q2, q1
    return (q1 * q1 + q2 * q2 - c * q1 * q2) / max(1, len(seg)) ** 2


def hz(n):
    return 440 * 2 ** ((n - 69) / 12)


def peak_at(frames, s0, s1, note):
    """note's fundamental: 10 times the power a semitone either side"""
    p, lo, hi = (goertzel(frames, s0, s1, hz(note + d)) for d in (0, -1, 1))
    return p > 10 * max(lo, hi), (p, lo, hi)


def live_piece():
    """D dorian (carried): channel 1 CHORDS (SINE KEY) silent, channel 2 BASS silent, channel 3 the drums every 8th"""
    drums = [Note(t * 48, 36 if t % 4 == 0 else 42, 30, 90) for t in range(8 * 8)]
    return encode(Piece("live", 1200, 8 * 384, [Channel("chords", []), Channel("bass", []), Channel("drums", drums)],
                        [Section(0, "A")], (2, 3)))


def test_24_4_player_keys_in_the_key_octaves_chromatic_midi_in(tmp_path):
    lines, t = push(100, live_piece())
    k = lambda ms, key, hold=500: [(ms, f"keys {1 << key}"), (ms + hold, "keys 0")]   # noqa: E731
    steps = [(50, f"btn {BTN['PLAY']}"),                 # LIBRARY: opens and plays
             (300, f"btn {BTN['ARP']}"),                 # KEYS on
             *k(500, 7),                                 # C4 -> D4, the root
             *k(1200, 11),                               # E4 -> F4, D dorian's third
             (1900, f"btn {BTN['OCT+']}"), *k(2000, 7),  # D5
             (2700, f"enc {ENC['KNOB3']} 13"),           # DOR (3) + 13 = CHROMATIC
             (2800, f"btn {BTN['OCT-']}"), *k(2900, 7),  # C4 as it is
             (3600, "trs 0x90 69 100"), (4100, "trs 0x80 69 0"),      # MIDI ch 1: channel 1 (sine), A4 as sent
             (4300, "trs 0x91 57 100"), (4800, "trs 0x81 57 0"),      # MIDI ch 2: channel 2 (SQR BASS), A3
             (5000, "trs 0x90 57 100"), (5500, "trs 0x90 57 0"),      # A3 on channel 1 (note-on velocity 0 = off)
             (5700, "trs 0x9A 76 100"), (6200, "trs 0x8A 76 0"),      # MIDI ch 11: the selected channel (1)
             (6400, f"btn {BTN['ARP']}"), *k(6500, 7),   # KEYS off: C4 mutes channel 5 (none): silent
             (7000, f"btn {BTN['ARP']}"), (7050, f"enc {ENC['KNOB3']} -13"),   # KEYS on, D DOR again
             (7100, f"enc {ENC['KNOB4']} 2"), *k(7200, 7),                   # E DOR: C4 plays E4
             *[(7900 + 20 * i, f"btn {BTN['OCT+']}") for i in range(4)],      # OCT+ four times: +3 at most
             *[(8100 + 20 * i, f"btn {BTN['OCT-']}") for i in range(8)]]     # OCT- eight times: -3 at most
    lines += [f"{t + ms} {what}" for ms, what in steps]
    log, wav = run_host(script(tmp_path, "l24four", lines, t + 8500))
    foot = [txt for ms, y, txt in screen(log) if y == 222 and ms >= t]
    assert "KEYS D DOR" in foot and "KEYS D DOR OCT+1" in foot and "KEYS CHROMATIC OCT+1" in foot and "KEYS CHROMATIC" in foot
    assert "KEYS OFF" in foot and "KEYS E DOR" in foot and "KEYS E DOR OCT+3" in foot and "KEYS E DOR OCT-3" in foot
    assert foot[-1] == "KEYS E DOR OCT-3" and not any("OCT+4" in x or "OCT-4" in x for x in foot)
    f = wav_frames(wav)
    b = t / 1000
    assert rms(f, b + 0.06, b + 0.36) > 100, "the piece plays (the drums)"
    for ms, note in ((500, 62), (1200, 65), (2000, 74), (2900, 60), (3600, 69), (4300, 57), (5000, 57), (5700, 76)):
        ok, p = peak_at(f, b + (ms + 80) / 1000, b + (ms + 450) / 1000, note)
        assert ok, (ms, note, p)
    ok, p = peak_at(f, b + 7.2, b + 7.3, 64)      # (E DOR: a short window, past the drums' next hit)
    assert ok, (7200, 64, p)
    sq = goertzel(f, b + 4.38, b + 4.75, hz(57) * 3) / goertzel(f, b + 4.38, b + 4.75, hz(57))
    sine = goertzel(f, b + 5.08, b + 5.45, hz(57) * 3) / goertzel(f, b + 5.08, b + 5.45, hz(57))
    assert sq > 20 * sine, ("MIDI ch 2 in channel 2's sound, ch 1 in channel 1's", sq, sine)
    assert not peak_at(f, b + 6.58, b + 6.95, 62)[0] and not peak_at(f, b + 6.58, b + 6.95, 60)[0], "KEYS off: no note"


def test_24_4_oct_pages_sections_only_past_14_with_keys_off(tmp_path):
    """OCT+/- page the section keys (US-23) only when KEYS is off and there are more than 14 sections"""
    for n, paged in ((14, False), (15, True)):
        b = encode(Piece("pages", 1200, n * 384, [Channel("lead", [Note(0, 60, 80, 100)])],
                         [Section(k * 384, "S%d" % k) for k in range(n)]))
        lines, t = push(100, b)
        lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 400} btn {BTN['OCT+']}"]
        log, _ = run_host(script(tmp_path, "l24page%d" % n, lines, t + 900))
        foot = [txt for ms, y, txt in screen(log) if y == 222 and ms > t + 380]
        assert ("KEYS: SECTIONS 15-24" in foot) == paged, (n, foot)


# ---- 24.5: hostile, host mode ----
def test_24_5_every_key_midi_edges_lost_note_offs_and_a_push_no_stuck_voice(tmp_path):
    from test_ui import piece
    lines, t = push(100, live_piece())
    steps = [(50, f"btn {BTN['PLAY']}"), (150, f"btn {BTN['PLAY']}"),   # opened, stopped: what sounds is live
             (400, f"btn {BTN['ARP']}"), (450, f"btn {BTN['ENV']}"),     # KEYS, TRIAD
             (600, f"keys {(1 << 27) - 1}"), (1100, "keys 0"),         # all 27 keys at once
             (1200, "trs 0x90 0 100"), (1210, "trs 0x90 127 100"), (1500, "trs 0x90 0 0"), (1510, "trs 0x80 127 0"),
             (1600, f"keys {1 << 7}"), (1700, "btn 2"), (1950, "btn 8"), (1960, "keys 0")]   # held into FX and back
    steps += [(2000, f"trs 0x91 {30 + i} 90") for i in range(40)]        # 40 notes, then 30 CCs and their 40 note-offs
    steps += [(2300, "trs 0xB1 1 1")] * 30 + [(2300, f"trs 0x81 {30 + i} 0") for i in range(40)]   # (6 do not fit)
    lines += [f"{t + ms} {what}" for ms, what in steps]
    second, t2 = push(t + 2600, piece("second", nch=2))                 # a push, MIDI notes between its frames
    lines += second
    lines += [f"{t + 2600 + 10 * i} trs {0x90 if i % 2 == 0 else 0x80} {60 + i // 2 % 12} {100 if i % 2 == 0 else 0}"
              for i in range((t2 - t - 2600) // 10 - 2)]
    lines += [f"{t2 + 2500} trs 0x91 50 100",                           # a note-off never sent (channel 2, BASS)
              f"{t2 + 3500} btn {BTN['HOME']}"]                         # leaving the piece ends it
    log, wav = run_host(script(tmp_path, "l24five", lines, t2 + 6000))
    assert "KEYS D DOR TRIAD" in [txt for ms, y, txt in screen(log) if y == 222]
    assert "2 PIECES" in [txt for ms, y, txt in screen(log) if ms >= t2 + 3500], "the push saved, MIDI meanwhile"
    f = wav_frames(wav)
    b = t / 1000
    assert rms(f, b + 0.7, b + 1.0) > 100, "every key at once plays"
    assert rms(f, b + 1.25, b + 1.45) > 100, "notes 0 / 127"
    assert rms(f, b + 2.05, b + 2.25) > 100, "40 MIDI notes"
    quiet = rms(f, t2 / 1000 + 1.4, t2 / 1000 + 2.4)
    assert quiet < 5, ("after the lost note-offs, the push: no voice stuck", quiet)
    assert rms(f, t2 / 1000 + 2.6, t2 / 1000 + 3.4) > 100, "the note held without a note-off"
    assert rms(f, t2 / 1000 + 5.0, t2 / 1000 + 6.0) < 5, "HOME ended it"


# ---- 24.6: the real firmware in fm1-emu, on-chip timing ----
@pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_24_6_strict_timing_densest_piece_and_4_live_notes(tmp_path):
    """the densest piece playing, KEYS on, a 4-note chord held on the keys and a MIDI note on the jack every half
    second: 10 s under on-chip timing, no empty audio half"""
    import re as _re
    from test_emu import dense, emu, replies, rms as erms, usb_script
    d = dense()
    assert len(d) > 30000
    usb, t = usb_script([d], gap=60)
    keys = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^", f"{t + 500}:ARP", f"{t + 550}:ARP^"]
    keys += [f"{t + 800}:n{k}" for k in (7, 9, 11, 12)]                    # C4 D4 E4 F4 held: 4 live notes
    for i, at in enumerate(range(t + 1000, t + 10000, 500)):
        keys += [f"{at}:m{0x90 + i % 8:02X}{40 + i % 30:02X}64", f"{at + 250}:m{0x80 + i % 8:02X}{40 + i % 30:02X}00"]
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=",".join(keys), strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = _re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert erms(s, len(s) // 2 - 5 * 44100) > 300, "the densest piece and the live notes, heard"


# ---- 24.4 through the jack's parser: raw bytes into fm1-emu's DIN jack (midi_uart.c um_byte, the ISR poll) ----
@pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_24_4_midi_in_raw_bytes_through_the_jack_parser_in_the_emulator(tmp_path):
    """the real firmware in fm1-emu: a piece open and stopped, then raw MIDI bytes into the DIN jack: a note-on, a
    second note in running status (data bytes only: only the parser makes it a note), then both released as note-on
    velocity 0 (the second again in running status): silent before, heard while held, silent after (none stuck)"""
    from test_emu import emu, replies, rms as erms, usb_script
    quiet = encode(Piece("jack", 1200, 384, [Channel("lead", [Note(0, 60, 1, 1)])]))
    usb, t = usb_script([quiet], gap=60)
    keys = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^", f"{t + 600}:PLAY", f"{t + 650}:PLAY^"]    # open (plays), stop
    keys += [f"{t + 1500}:m903C64", f"{t + 1550}:m4364"]                                     # C4, then G4 in running status
    keys += [f"{t + 2500}:m903C00", f"{t + 2550}:m4300"]                                     # released: note-on velocity 0
    end = t + 4500
    report, _, s = emu(tmp_path, end, usb=usb, keys=",".join(keys), wav=True)
    assert replies(report)[-1] == 0x45, "the piece saved"
    fr = lambda ms: len(s) // 2 - int((end - ms) * 44.1)                                     # (the recording ends at `end`)
    before, held, after = erms(s, fr(t + 1000), fr(t + 1450)), erms(s, fr(t + 1800), fr(t + 2400)), erms(s, fr(t + 3800), fr(t + 4400))
    assert held > 300 and held > 20 * max(before, 1), (before, held, after)                  # the jack's notes, heard
    assert after < held / 20, (before, held, after)                                          # released: none stuck


def test_24_1_oct_keys_in_steps_move_the_entered_note_by_octaves(tmp_path):
    """STEPS (composer.c's ed_octs, shared with KEYS' octave): OCT- then C4's key enters C3 at step 1; OCT+ twice
    (net +1) then C4's key enters C5 at step 2: OCT- goes down, OCT+ up"""
    from test_steps import BTN as SB, ENC as SE, blank, keys as skeys, steps_run
    steps = [(50, "btn 0"), *skeys(100, 7),                                    # OCT-, then the C4 key: C3
             (200, "btn 1"), (250, "btn 1"), (300, f"enc {SE['SELECT']} 1"), *skeys(350, 7)]   # +2: C5 at step 2
    (log, _), t0 = steps_run(tmp_path, blank(), steps, 600, "stoct")
    from test_ui import at as uat
    assert (166, "STEP 1  C3") in uat(log, t0 + 100, t0 + 190), uat(log, t0 + 100, t0 + 190)
    assert (166, "STEP 2  C5") in uat(log, t0 + 350, t0 + 590), uat(log, t0 + 350, t0 + 590)
    assert SB and SE                                                           # (the same screen's maps)
