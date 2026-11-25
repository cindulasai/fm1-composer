"""US-30: a channel plays POLY, MONO or LEGATO with glide (SLOOP's voice code, copied); each sound brings SLOOP's own
voice (AUTO), a channel's override is set on the FM-1 and saved in the piece; SLIDE on a step in STEPS. The voice pool
itself (retrigger, legato, glide blocks, overrides, hostile sequences) is tests/player_test.c's 30.1-30.6; here: the
copy, the piece bytes (both writers and the C reader), WAVs rendered by the firmware's own code, the FM-1's screens."""
import math, os, random, re, struct, subprocess, sys, zlib
import pytest
from conftest import APP, run_host, wav_frames
from test_piece import cdump, rand_piece  # noqa: F401  (the C reader fixture)
from test_ui import push, script, at
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c
from test_engine import studio_cli, smf, fm1c_fit, engine_fit  # noqa: F401  (Studio's Rust fit)

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import (AUTO, BAD_EVENT, BAD_TABLE, LEGATO, MONO, POLY, Channel, Note, Piece, PieceError,  # noqa: E402
                        Section, decode, dump, encode)

BTN = {"ENV": 4, "LFO": 5, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB3": 4, "KNOB4": 5}
SLOOP_VOICE_BLOB = "530ce2f702e83ef7328b3989612e7baf60b35d12"     # firmware/src/voice.c @ bac9c05
COPIED = [(255, 281), (390, 447), (496, 509), (536, 553), (850, 858)]   # voice.c lines in synth.c, unchanged
ENV_LIN_86 = 23810                                                 # sloop_tables.h: GLIDE 86 (RATE: an octave in it)


def recrc(b):
    b = bytearray(b)
    b[-4:] = struct.pack("<I", zlib.crc32(bytes(b[:-4])) & 0xFFFFFFFF)
    return bytes(b)


@pytest.fixture(scope="module")
def render(tmp_path_factory):
    """render(piece bytes, seconds) -> WAV frames, through tests/render_piece.c; .ids: sound name -> its first id"""
    d = tmp_path_factory.mktemp("render")
    exe = d / "render_piece"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                    str(APP / "tests/render_piece.c"), "-lm"], check=True)
    names = subprocess.run([str(exe), "--sounds"], capture_output=True, text=True, check=True).stdout.splitlines()

    def run(data, secs=3, name="p"):
        f, w = d / f"{name}.f1p", d / f"{name}.wav"
        f.write_bytes(data)
        subprocess.run([str(exe), str(f), str(w), str(secs)], capture_output=True, text=True, check=True)
        return wav_frames(w)
    run.ids = {}
    for i, n in enumerate(names):
        run.ids.setdefault(n, i)
    return run


def pitches(frames, s0, s1, floor=300):
    """(seconds, MIDI pitch) a period of the left channel (rising zero crossings, interpolated), a median of 5"""
    x = frames[int(s0 * 44100) * 2:int(s1 * 44100) * 2:2]
    raw, last, peak = [], None, 0
    for i in range(1, len(x)):
        peak = max(peak * 0.999, abs(x[i]))
        if x[i - 1] < 0 <= x[i] and peak > floor:
            t = i - 1 + -x[i - 1] / (x[i] - x[i - 1])
            if last is not None:
                raw.append((s0 + t / 44100, 69 + 12 * math.log2(44100 / (t - last) / 440)))
            last = t
    return [(raw[k][0], sorted(p for _, p in raw[k - 2:k + 3])[2]) for k in range(2, len(raw) - 2)]


def glide_time(track, a, b):
    """seconds from where the pitch leaves a (a + 0.5 toward b) to where it nears b (b - 0.5)"""
    up = b > a
    t0 = next(t for t, p in track if (p > a + 0.5 if up else p < a - 0.5))
    t1 = next(t for t, p in track if t > t0 and (p > b - 0.5 if up else p < b + 0.5))
    return t1 - t0


# ---- 30.1: SLOOP's code, copied ----
def test_30_1_sloops_voice_code_copied_line_for_line_with_provenance():
    """synth.c holds voice.c @ bac9c05 lines 255-281 (glide), 390-447 (mono_play, mono_pick, mono_remove), 496-509 and
    536-553 (trk_note_on / _off's MONO / LEGATO parts), 850-858 (the glide a block), each unchanged; the provenance row
    names them (the v8 render CRC itself: tests/player_test.c 30.1)"""
    from test_fm6 import blob, sloop_src
    syn = (APP / "firmware/src/synth.c").read_text()
    prov = (APP / "firmware/SLOOP-PROVENANCE.md").read_text()
    row = re.search(r"^\| app/firmware/src/synth\.c \| firmware/src/voice\.c \((.*?)\) .*?\| " + SLOOP_VOICE_BLOB + r" .*\|$", prov, re.M)
    assert row, "a provenance row for the copied voice code"
    for fn in ("glide_set", "glide_ticks", "mono_play", "mono_pick", "mono_remove"):
        assert fn in row[1] and re.search(rf"\b{fn}\(", syn)
    assert "lines 255-281, 390-447, 496-509, 536-553 and 850-858" in row[0]
    src = sloop_src()
    if src is None:
        pytest.skip("SLOOP's source is not here (research/sloop-src or $SLOOP_SRC); the row is checked above")
    assert blob(src / "firmware/src/voice.c") == SLOOP_VOICE_BLOB
    voice = (src / "firmware/src/voice.c").read_text().splitlines()
    for a, b in COPIED:
        assert "\n".join(voice[a - 1:b]) + "\n" in syn, f"voice.c lines {a}-{b}, unchanged"


# ---- 30.2: MONO / LEGATO and glide, measured in a WAV ----
def test_30_2_glide_slides_the_pitch_over_the_glide_time(render):
    """SINE KEY, glide 86 (RATE: an octave in 704.6 control ticks): LEGATO, A4 then E5 over it: the pitch leaves A4 as E5
    starts and arrives over 7/12 of that (+-10%); MONO glides too, also without overlap; glide 0 jumps"""
    sk = render.ids["SINE KEY"]
    step = ENV_LIN_86 * 3 >> 10                                    # voice.c line 852: 1/4096 semitone a block
    want = 7 * 4096 / step * 32 / 44100 * 6 / 7                    # (A4 + 0.5 .. E5 - 0.5: 6 of the 7 semitones)
    for mode, notes in ((LEGATO, [Note(0, 69, 192, 100), Note(96, 76, 288, 100)]),
                        (MONO, [Note(0, 69, 90, 100), Note(96, 76, 288, 100)])):
        f = render(encode(Piece("glide", 1200, 768, [Channel("bass", notes, sound=sk, mode=mode, glide=86)])), 3)
        tr = pitches(f, 0.05, 1.5)
        assert all(abs(p - 69) < 0.3 for t, p in tr if 0.1 < t < 0.49), "A4 first"
        g = glide_time([x for x in tr if x[0] > 0.45], 69, 76)
        assert abs(g - want) <= want * 0.10, f"mode {mode}: the glide took {g * 1000:.0f} ms, want {want * 1000:.0f} +-10%"
        assert all(abs(p - 76) < 0.3 for t, p in tr if 0.9 < t < 1.4), "then E5"
    f = render(encode(Piece("jump", 1200, 768, [Channel("bass", [Note(0, 69, 192, 100), Note(96, 76, 288, 100)],
                                                        sound=sk, mode=LEGATO)])), 3)
    tr = pitches(f, 0.45, 0.7)
    assert sum(1 for t, p in tr if 70 < p < 75) <= 2, "glide 0: E5 at once (legato, no glide)"


# ---- 30.3: each sound's own voice; the override wins ----
def test_30_3_a_sloop_preset_brings_mono_and_its_glide_an_override_wins(render):
    """TALKBOX (SLOOP's VOICE engine: MONO, GLIDE 60 in its extras): AUTO glides from A3 to E4 (an octave in 77 ms: 45
    ms); POLY and MONO glide 0 (overrides) put E4 there at once, AUTO only after the glide"""
    tb = render.ids["TALKBOX"]

    def e4(mode, glide=0):
        notes = [Note(0, 57, 192, 100), Note(96, 64, 288, 100)]
        f = render(encode(Piece("tb", 1200, 768, [Channel("bass", notes, sound=tb, mode=mode, glide=glide)])), 2, f"tb{mode}")
        from test_live import goertzel, hz
        b = 0.5 + 32 / 44100                                       # (E4 is due at tick 96: 0.5 s, its block)
        return goertzel(f, b + 0.004, b + 0.022, hz(64)), goertzel(f, b + 0.15, b + 0.3, hz(64))
    auto, poly, mono = e4(AUTO), e4(POLY), e4(MONO)
    assert auto[1] > 0 and poly[1] > 0 and mono[1] > 0, "E4 sounds in the end"
    assert poly[0] > 0.3 * poly[1] and mono[0] > 0.3 * mono[1], "the overrides: E4 at once"
    assert auto[0] < 0.25 * auto[1] and auto[0] < 0.25 * poly[0], f"AUTO: still gliding toward E4 ({auto}, {poly})"


# ---- 30.4: the mode in the piece ----
POLY_KEPT = [POLY, AUTO, MONO, LEGATO]


def with_voices(p, rng):
    for c in p.channels:
        c.mode, c.glide = rng.choice(POLY_KEPT), rng.choice([0, 0, 127, rng.randint(0, 127)])
        c.notes = [Note(n.tick, n.pitch, n.dur, n.vel, rng.random() < 0.2) for n in c.notes]
    return p


def test_30_4_c_reader_matches_python_on_1000_pieces_with_voices_and_slides(tmp_path, cdump):
    rng = random.Random(304)
    paths, pieces = [], []
    for i in range(1000):
        p = with_voices(rand_piece(rng), rng)
        if i % 4 == 0:
            p.sections = [Section(0, "A")]
        b = encode(p)
        assert dump(decode(b)) == dump(p) and encode(decode(b)) == b
        assert struct.unpack_from("<I", b, 44)[0] == sum(c.mode << 4 * k for k, c in enumerate(p.channels))
        assert [b[48 + 16 * k + 7] for k in range(len(p.channels))] == [c.glide for c in p.channels]
        f = tmp_path / f"{i}.f1p"
        f.write_bytes(b)
        paths.append(f); pieces.append(p)
    outs = []
    for k in range(0, 1000, 250):
        outs += cdump(*paths[k:k + 250])
    assert len(outs) == 1000
    for p, o in zip(pieces, outs):
        assert o == dump(p)
    assert any("\nv " in o for o in outs) and any(" s\n" in o for o in outs)


def test_30_4_a_v8_piece_reads_every_channel_poly_and_keeps_its_bytes(tmp_path, cdump):
    rng = random.Random(3041)
    for _ in range(100):
        b = encode(rand_piece(rng))                                # (no mode bytes: what v8 wrote)
        q = decode(b)
        assert all(c.mode == POLY and c.glide == 0 and not any(n.slide for n in c.notes) for c in q.channels)
        assert encode(q) == b and "\nv " not in dump(q)
    f = tmp_path / "v8.f1p"
    f.write_bytes(b)
    assert "\nv " not in cdump(f)[0]


def test_30_4_new_pieces_from_fm1c_and_studio_are_every_channel_auto(studio_cli, tmp_path):
    """fm1c's fit and Studio's (Rust) write the same bytes: every channel AUTO, glide 0"""
    mid = tmp_path / "m.mid"
    mid.write_bytes(smf([("lead", 0, 81), ("bass", 1, 33), ("drums", 9, None), ("pad", 2, 89)]))
    want, got = fm1c_fit(mid, "auto", 6), engine_fit(studio_cli, mid, "auto", 6, tmp_path)
    assert want and got and got[0] == want[0]
    p = decode(want[0])
    assert len(p.channels) == 4 and all(c.mode == AUTO and c.glide == 0 for c in p.channels)
    assert struct.unpack_from("<I", want[0], 44)[0] == 0x1111


def test_30_4_the_channel_screen_sets_the_voice_saved_in_the_piece_reboot_same(tmp_path, host_bin):
    """PLAYER: KNOB3 the selected channel's voice (AUTO POLY MONO LEGATO), KNOB4 its glide; SAVE; after a reboot the
    same, and the piece pulled back says so"""
    flash = str(tmp_path / "flash.bin")
    d = encode(Piece("voices", 1200, 384, [Channel("lead", [Note(0, 60, 48, 100)], mode=AUTO),
                                            Channel("bass", [Note(0, 36, 48, 100)], mode=AUTO)]))
    lines, t = push(100, d)
    lines += [f"{t + 50} btn {BTN['PLAY']}", f"{t + 150} btn {BTN['PLAY']}",
              f"{t + 250} enc {ENC['KNOB3']} 2", f"{t + 350} enc {ENC['KNOB4']} 40",
              f"{t + 450} enc {ENC['KNOB1']} 1", f"{t + 500} enc {ENC['KNOB3']} -1", f"{t + 600} enc {ENC['KNOB4']} 5",
              f"{t + 700} enc {ENC['KNOB3']} 9", f"{t + 800} enc {ENC['KNOB3']} -1", f"{t + 900} btn {BTN['SAVE']}"]
    log, _ = run_host(script(tmp_path, "vox1", lines, t + 1000), env={"HOST_FLASH": flash})
    assert (222, "VOICE 1 MONO GLIDE 0") in at(log, t + 250, t + 340), "AUTO -> POLY -> MONO"
    assert (222, "VOICE 1 MONO GLIDE 40") in at(log, t + 350, t + 440)
    assert (222, "VOICE 2 AUTO LEGATO GLIDE 0") in at(log, t + 500, t + 590), "AUTO (the first): SQR BASS's own, LEGATO"
    assert (222, "VOICE 2 LEGATO GLIDE 5") in at(log, t + 600, t + 690), "a glide turned in AUTO: the mode the sound plays"
    assert (222, "VOICE 2 LEGATO GLIDE 5") in at(log, t + 700, t + 790), "KNOB3 stops at LEGATO"
    assert (222, "VOICE 2 MONO GLIDE 5") in at(log, t + 800, t + 890)
    assert (222, "SAVED") in at(log, t + 900)
    log2, _ = run_host(script(tmp_path, "vox2", ["100 btn 12", "200 btn 12", f"300 enc {ENC['KNOB3']} -1",
                                                 f"400 enc {ENC['KNOB3']} 1", f"500 enc {ENC['KNOB1']} 1",
                                                 f"550 enc {ENC['KNOB3']} 1"], 700), env={"HOST_FLASH": flash})
    assert (222, "VOICE 1 POLY GLIDE 40") in at(log2, 300, 390) and (222, "VOICE 1 MONO GLIDE 40") in at(log2, 400, 490)
    assert (222, "VOICE 2 LEGATO GLIDE 5") in at(log2, 550), "after the reboot: MONO 5 as saved (+1: LEGATO)"
    port = f"exec:{host_bin} {flash}"
    pid = fm1c("list", "--port", port).stdout.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "b.f1p", "--port", port).returncode == 0
    back = decode((tmp_path / "b.f1p").read_bytes())
    assert [(c.mode, c.glide) for c in back.channels] == [(MONO, 40), (MONO, 5)]
    assert [c.notes for c in back.channels] == [c.notes for c in decode(d).channels]


# ---- 30.5: SLIDE in STEPS ----
def test_30_5_slide_on_a_step_glides_into_the_next_steps_note(tmp_path, host_bin, render):
    """STEPS: C4 at step 1, ENV: SLIDE (shown on the step), E4 at step 2; C4 at 5 and E4 at 6 without; SAVE, PLAY: the
    WAV glides from step 1 into 2 (SLOOP's SLIDE_T, ~46 ms, glide 0) and jumps from 5 to 6"""
    from test_steps import steps_run, keys
    flash = str(tmp_path / "flash.bin")
    d = encode(Piece("slide", 1200, 384, [Channel("bass", [], sound=render.ids["SINE KEY"], mode=LEGATO)]))
    steps = [*keys(50, 7), (150, f"btn {BTN['ENV']}"), (250, f"enc {ENC['SELECT']} 1"), *keys(300, 11),
             (400, f"enc {ENC['SELECT']} 3"), *keys(450, 7), (550, f"enc {ENC['SELECT']} 1"), *keys(600, 11),
             (700, f"enc {ENC['SELECT']} -5"), (800, f"btn {BTN['SAVE']}"), (900, f"btn {BTN['PLAY']}")]
    (log, wav), t0 = steps_run(tmp_path, d, steps, 3400, "slide", env={"HOST_FLASH": flash})
    assert (184, "LEN 1  VEL 100  SLIDE") in at(log, t0 + 150, t0 + 240), "SLIDE shown on the step"
    assert (184, "LEN 1  VEL 100") in at(log, t0 + 450, t0 + 540), "step 5: no SLIDE"
    assert (184, "LEN 1  VEL 100  SLIDE") in at(log, t0 + 700, t0 + 790), "back on step 1: SLIDE"
    assert (222, "SAVED") in at(log, t0 + 800)
    f = wav_frames(wav)
    p0 = (t0 + 900) / 1000                                         # PLAY: step 1 at once, a step = 125 ms
    for bar in (0, 2):
        b = p0 + bar
        tr = pitches(f, b + 0.06, b + 0.8)
        slid = [t for t, p in tr if b + 0.1 < t < b + 0.25 and 60.6 < p < 63.4]
        jump = [t for t, p in tr if b + 0.6 < t < b + 0.75 and 60.6 < p < 63.4]
        assert len(slid) >= 4 and 0.02 <= slid[-1] - slid[0] <= 0.08, f"bar {bar + 1}: steps 1 -> 2 glide ({len(slid)} periods)"
        assert len(jump) <= 2, f"bar {bar + 1}: steps 5 -> 6 jump ({len(jump)} periods between)"
    port = f"exec:{host_bin} {flash}"
    pid = fm1c("list", "--port", port).stdout.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "s.f1p", "--port", port).returncode == 0
    back = decode((tmp_path / "s.f1p").read_bytes())
    assert [(n.tick, n.pitch, n.slide) for n in back.channels[0].notes] == [(0, 60, True), (24, 64, False), (96, 60, False),
                                                                            (120, 64, False)]


# ---- 30.6: hostile ----
def test_30_6_corrupt_mode_reads_poly_bad_glide_and_slide_bytes_refused(tmp_path, cdump, render):
    """a mode nibble past LEGATO reads POLY in both readers and renders as POLY; a glide of 128 or a SLIDE velocity of 0
    is refused with the same code by both"""
    sk = render.ids["SINE KEY"]
    notes = [Note(0, 69, 192, 100), Note(96, 76, 288, 100)]
    poly = encode(Piece("x", 1200, 768, [Channel("bass", notes, sound=sk), Channel("lead", [], mode=MONO, glide=9)]))
    bad = bytearray(poly)
    bad[44] = (bad[44] & 0xF0) | 0xF                                 # channel 1: 15
    bad = recrc(bad)
    q = decode(bad)
    assert q.channels[0].mode == POLY and q.channels[1].mode == MONO
    f = tmp_path / "c.f1p"
    f.write_bytes(bad)
    assert cdump(f)[0] == dump(q) and dump(q) == dump(decode(poly))
    assert render(bad, 2, "corrupt") == render(poly, 2, "poly"), "a corrupt mode renders as POLY"
    for off, v, code in ((48 + 7, 128, BAD_TABLE), (48 + 16 + 7, 255, BAD_TABLE), (48 + 32 + 5, 128, BAD_EVENT)):
        b = bytearray(poly); b[off] = v; b = recrc(b)
        g = tmp_path / "h.f1p"; g.write_bytes(b)
        with pytest.raises(PieceError) as e:
            decode(b)
        assert e.value.code == code and cdump(g)[0] == f"ERR {code}\n", (off, v)
    for c in (Channel("lead", mode=4), Channel("lead", glide=128), Channel("lead", mode=-1)):
        with pytest.raises(PieceError) as e:
            encode(Piece("x", 1200, 384, [c]))
        assert e.value.code == BAD_TABLE


def test_30_6_every_mode_and_glide_extreme_renders_clean(render):
    """glide 0 and 127 in MONO and LEGATO, 4-note chords on MONO, notes without overlap: no stuck note (silent after
    the end), no clipping, the renderer runs clean"""
    sk, rb = render.ids["SINE KEY"], render.ids["ROBOT"]
    chans = []
    for k, (mode, glide) in enumerate([(MONO, 0), (MONO, 127), (LEGATO, 0), (LEGATO, 127), (AUTO, 0), (MONO, 64)]):
        notes = [Note(s * 24, 48 + k * 4 + (s * 5) % 12 + j * 3, 30 if s % 3 else 12, 100) for s in range(32) for j in range(4)]
        chans.append(Channel("bass", notes, sound=rb if k == 4 else sk, mode=mode, glide=glide))
    f = render(encode(Piece("hostile", 1500, 768, chans)), 5)
    assert max(abs(v) for v in f[:int(2.0 * 44100) * 2]) > 1000, "heard"
    assert max(abs(v) for v in f[int(4.5 * 44100) * 2:]) < 40, "silent after the end: no note held"


# ---- 30.7: the real firmware in fm1-emu, on-chip timing ----
@pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_30_7_strict_timing_densest_piece_4_mono_glide_channels(tmp_path):
    """the densest piece with its first 4 channels MONO / LEGATO with glide (every note gliding): 10 s under on-chip
    timing, no empty audio half"""
    from test_emu import dense, emu, replies, rms as erms, usb_script
    p = decode(dense())
    for k, c in enumerate(p.channels[:4]):
        c.mode, c.glide = (MONO, LEGATO)[k % 2], 60 + 10 * k
    d = encode(p)
    assert len(d) > 30000
    usb, t = usb_script([d], gap=60)
    keys_ = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^"]
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=",".join(keys_), strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert erms(s, len(s) // 2 - 5 * 44100) > 300, "the densest piece, 4 channels gliding, heard"
