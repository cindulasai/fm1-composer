"""US-22: text2midi's instruments, roles and mix on the FM-1 (fm1c/sounds.py, fit.py)."""
import os, re, struct, subprocess, sys
import mido
import pytest
from conftest import APP, run_host
from midifix import make_midi

sys.path.insert(0, str(APP / "tools"))
from fm1c import sounds  # noqa: E402
from fm1c.fit import fit, role_of  # noqa: E402
from fm1c.piece import decode, encode  # noqa: E402

KB = APP / "studio" / "engine" / "crates" / "t2m-core" / "data" / "instruments"
BRIEFS = APP / "studio" / "engine" / "suite" / "briefs"
N = sounds.NAMES
DRUM_KITS = {0, 1, 2, *range(40, 72)}


def kb_instruments():
    """every instrument of text2midi's pinned knowledge base: (id, name, aliases, gm_program, roles, is a kit)"""
    out = []
    for f in sorted(KB.glob("*.ron")):
        for block in re.split(r"\n    \(\n", f.read_text())[1:]:
            get = lambda k: re.search(rf"^        {k}: (.*),$", block, re.M).group(1)    # noqa: E731
            out.append((get("id").strip('"'), get("name").strip('"'), re.findall(r'"([^"]*)"', get("aliases")),
                        int(get("gm_program")), re.findall(r"\w+", get("roles")), "kit: [" in block))
    return out


# ---- 22.1 ----
# (the mapping, by sound name: what each text2midi instrument plays)
WANT = {"keys.piano": "SINE KEY", "keys.rhodes": "TINE EP", "bass.electric": "FAT BASS", "bass.synth": "SUB BASS",
        "bass.upright": "ROUND BASS", "kit.standard": "STD KIT", "pad.warm": "SOFT PAD", "synth.lead": "PULSE LD",
        "guitar.clean": "PLUCK", "guitar.overdrive": "ROCK DRIVE", "guitar.nylon": "NYLON PICK",
        "strings.ensemble": "STRINGS", "harp.orchestral": "PLUCK", "vibes.vibraphone": "WOOD BARS", "fx.riser": "WIND"}


def test_22_1_every_text2midi_instrument_maps_to_a_catalog_sound(tmp_path):
    from test_ui import catalog
    names = catalog(tmp_path)
    assert list(N) == names[:80]                          # ids 0-79 as the firmware has them
    kb = kb_instruments()
    assert len(kb) == 15 and {i[0] for i in kb} == set(sounds.INSTRUMENTS) == set(WANT)
    aliases = {}
    for iid, name, als, prog, roles, kit in kb:
        assert sounds.INSTRUMENTS[iid] == (prog, sounds.INSTRUMENTS[iid][1]) and sounds.INSTRUMENTS[iid][1] in roles
        role = sounds.INSTRUMENTS[iid][1]
        s = sounds.sound(f"{role} – {iid}", None, role)
        assert 0 <= s < 80 and N[s] == WANT[iid], iid
        assert (s in DRUM_KITS) == kit
        assert sounds.sound(iid, None, role) == s                      # the id alone names it too
        for a in [name] + als:
            aliases[a.lower()] = iid
            assert sounds.sound(f"x – {a.upper()}", None, role) == s, a      # and its name and aliases
    assert sounds.ALIASES == aliases


GM_WANT = ["SINE KEY"] * 4 + ["TINE EP"] * 2 + ["PLUCK"] * 2 + \
    ["GLASS BELL", "GLASS BELL", "RING BELL", "WOOD BARS", "WOOD BARS", "WOOD BARS", "BELL", "PLUCK"] + \
    ["DRAWBARS", "DRAWBARS", "FULL ORGAN", "ORGAN", "GOSPEL", "GOSPEL", "WIND", "GOSPEL"] + \
    ["NYLON PICK", "NYLON PICK", "PLUCK", "PLUCK", "PLUCK", "ROCK DRIVE", "ROCK DRIVE", "RING BELL"] + \
    ["ROUND BASS", "FAT BASS", "FAT BASS", "ROUND BASS", "SQR BASS", "SQR BASS", "SUB BASS", "WAVE BASS"] + \
    ["STRING"] * 4 + ["PWM STR", "PLUCK", "PLUCK", "ROUND BASS"] + \
    ["STRINGS", "STRINGS", "PWM STR", "PWM STR", "CHIP CHOIR", "CHIP CHOIR", "CHIP CHOIR", 15] + \
    ["BRASS SECT"] * 3 + [12, "BRASS SECT", "BRASS SECT", 12, 15] + \
    ["WIND"] * 7 + ["SOFT FLUTE"] + ["SOFT FLUTE"] * 8 + \
    ["PULSE LD", "SAW LEAD", "SOFT FLUTE", "WAVE LEAD", "ROCK DRIVE", "CHIP CHOIR", "SYNC LEAD", "ACID"] + \
    [76, "SOFT PAD", "PWM STR", "CHIP CHOIR", "STRING", "WIRE", 76, "RESO"] + \
    ["RAIN", 4, "GLASS BELL", 4, "BELL", 38, "RING BELL", 38] + \
    ["WIRE", "PLUCK", "PLUCK", "NYLON PICK", "WOOD BARS", "WIND", "STRING", "WIND"] + \
    ["RING BELL", "JAZZ PERC", "WOOD BARS", "JAZZ PERC", "ROUND BASS", "ROUND BASS", "METAL", 36] + \
    ["METAL", 36, "RAIN", 38, 38, 36, "RAIN", "METAL"]


def test_22_1_every_gm_program_has_a_sound():
    want = [w if isinstance(w, int) else N.index(w) for w in GM_WANT]      # (names that repeat are given as ids)
    assert len(want) == 128 and list(sounds.GM) == want
    assert [sounds.sound("Track", p, "lead") for p in range(128)] == want
    assert not set(want) & DRUM_KITS
    assert [N[a] for a in sounds.FM6_ALT] == ["SINE KEY", "BELL", "FAT BASS", "BRASS", "SOFT PAD", "RING BELL", "ORGAN",
                                              "PLUCK"]
    assert all(a < sounds.FM6_FIRST for a in sounds.FM6_ALT) and N[sounds.FM6_FIRST] == "TINE EP"


@pytest.mark.parametrize("program,kit", [(0, "STD KIT"), (7, "STD KIT"), (8, "VINTAGE"), (16, "BOOM KIT"), (24, "909"),
                                         (25, "808"), (26, "909"), (32, "JAZZ"), (40, "JAZZ"), (48, "STD KIT"),
                                         (56, "GLITCH"), (63, "GLITCH"), (64, "STD KIT"), (127, "STD KIT")])
def test_22_1_gm_drum_kits(program, kit):
    assert N[sounds.sound("Track", program, "drums")] == kit
    assert N[sounds.sound("Loop", program, "percussion")] == kit


# ---- 22.2 ----
@pytest.mark.parametrize("name,src,program,role", [
    ("keys – keys.rhodes", 0, None, "chords"), ("keys", 0, 4, "chords"), ("keys", 0, None, "other"),
    ("pad2", 1, None, "pad"), ("texture – fx.riser", 2, None, "fx"), ("hook – harp.orchestral", 3, None, "arp"),
    ("topline – synth.lead", 4, 0, "lead"), ("Piano", 5, None, "chords"), ("Track 1", 3, 33, "bass"),
    ("Track 1", 3, 56, "lead"), ("Track 1", 3, 127, "fx"), ("Track 1", 3, 128, "other"), ("Track 1", 3, -1, "other"),
    ("Track", 9, 33, "drums"), ("rhodes – keys.rhodes", 9, None, "drums"), ("bass – synth.lead", 0, 80, "bass"),
    ("lead – bass.synth", 0, 38, "lead"), ("vibes", 6, None, "chords"), ("Sweep", 7, None, "fx"),
])
def test_22_2_roles_from_instrument_or_program(name, src, program, role):
    assert role_of(name, src, program) == role


def test_22_2_no_program_with_a_role_becomes_other():
    want = ["chords"] * 32 + ["bass"] * 8 + ["pad"] * 16 + ["lead"] * 32 + ["pad"] * 8 + ["fx"] * 8 + ["lead"] * 8 + \
        ["fx"] * 16
    assert [role_of("Track 7", 0, p) for p in range(128)] == want
    assert all(role_of("Track 7", 9, p) == "drums" for p in (None, 0, 33, 127))


def test_22_2_a_file_keeps_its_roles_and_sounds(tmp_path):
    pc = lambda ch, p: [mido.Message("program_change", channel=ch, program=p)]     # noqa: E731
    notes = [(480 * k, 60, 240, 90) for k in range(4)]
    f = make_midi(tmp_path / "r.mid", [("keys", 0, notes, pc(0, 4)), ("Bass", 9, notes, pc(9, 25)),
                                       ("Strings 2", 1, notes, pc(1, 40)), ("Track 4", 2, notes, pc(2, 33)),
                                       ("keys – keys.piano", 3, notes)])
    piece, rep = fit(str(f))
    got = [(c.role, N[c.sound], c.src) for c in piece.channels]
    assert got == [("drums", "808", 9), ("bass", "FAT BASS", 2), ("chords", "TINE EP", 0), ("chords", "SINE KEY", 3),
                   ("pad", "STRING", 1)]
    assert rep.reductions == 0


# ---- 22.3 ----
def cc(ch, *pairs):
    return [mido.Message("control_change", channel=ch, control=c, value=v) for c, v in pairs]


def test_22_3_volume_and_pan_bytes_from_cc7_and_cc10(tmp_path):
    notes = [(0, 60, 240, 90)]
    f = make_midi(tmp_path / "m.mid", [("lead – synth.lead", 0, notes, cc(0, (7, 0), (10, 127))),
                                       ("bass – bass.synth", 1, notes, cc(1, (7, 127), (10, 0))),
                                       ("pad – pad.warm", 2, notes, cc(2, (7, 30), (10, 1), (7, 90), (10, 100))),
                                       ("chords – keys.piano", 3, notes, cc(4, (7, 5), (10, 6), (11, 7))),
                                       ("arp – harp.orchestral", 5, notes, cc(5, (10, 64), (7, 126), (8, 1)))])
    piece, _ = fit(str(f))
    data = encode(piece)
    table = [data[48 + 16 * i:48 + 16 * i + 5] for i in range(len(piece.channels))]
    # role, sound, vol, pan, source: bass first, then lead, chords, pad, arp (fit.py's order)
    assert table == [bytes([4, 10, 127, 0, 1]), bytes([0, 31, 0, 127, 0]), bytes([2, 8, 100, 64, 3]),
                     bytes([3, 4, 30, 1, 2]), bytes([7, 11, 126, 64, 5])]
    assert [(c.vol, c.pan) for c in decode(data).channels] == [(127, 0), (0, 127), (100, 64), (30, 1), (126, 64)]


def test_22_3_text2midi_mix_is_its_neutral_mix(tmp_path):
    """text2midi writes no CC7/CC10 (its parts play at full level, centred): every channel 100 / 64"""
    t2m = t2m_or_skip()
    piece = decode(compose(t2m, tmp_path, "cinematic-orchestral-1").read_bytes())
    assert len(piece.channels) == 7 and {(c.vol, c.pan) for c in piece.channels} == {(100, 64)}


# ---- 22.5 ----
def t2m_or_skip():
    from fm1c import cli
    t2m = os.environ.get("FM1C_T2M") or cli.DEFAULT_T2M
    if not os.access(t2m, os.X_OK):
        pytest.skip(f"text2midi's t2m-cli is not built at {t2m}")
    return t2m


def compose(t2m, tmp_path, brief):
    out = tmp_path / f"{brief}.f1p"
    r = subprocess.run([str(APP / "fm1c"), "compose", str(BRIEFS / f"{brief}.json"), "--seed", "1", "--t2m", t2m,
                        "-o", str(out)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    assert "role default sound" not in r.stdout and "FM6 sounds at once" not in r.stdout, r.stdout
    return out


def test_22_5_composed_suite_brief_shows_the_mapped_sounds_on_the_player(tmp_path):
    from test_ui import BTN, at, push, script
    t2m = t2m_or_skip()
    data = compose(t2m, tmp_path, "latin-bossa-1").read_bytes()
    piece = decode(data)
    want = [("drums", "STD KIT"), ("bass", "ROUND BASS"), ("lead", "WOOD BARS"), ("chords", "NYLON PICK")]
    assert [(c.role, N[c.sound]) for c in piece.channels] == want
    lines, t = push(100, data)
    (log, _) = run_host(script(tmp_path, "us22_bossa", lines + [f"{t + 50} btn {BTN['PLAY']}"], t + 1500))
    s = at(log, t + 50)
    pitch = 17 if piece.sections else 19                     # (US-23: text2midi's form makes room for the sections strip)
    assert piece.sections
    for c, (role, sound) in enumerate(want):
        assert (64 + c * pitch, role.upper()) in s and (64 + c * pitch, sound) in s, (role, sound)
    assert not {txt for y, txt in s} & {"SQR BASS", "SAW LEAD", "SINE KEY"}     # (no role defaults)


# ---- 22.6 ----
@pytest.mark.parametrize("name,program", [("lead – no.such.horn", None), ("Track", 128), ("Track", -1),
                                          ("Track", 300), ("Track", None), ("", None), ("x – ", None)])
def test_22_6_unknown_instrument_or_program_is_the_role_default(name, program):
    for role in ("lead", "drums", "percussion"):
        assert sounds.sound(name, program, role) is None
    assert sounds.role_hint(name, program) is None


def test_22_6_kit_and_melodic_instruments_stay_on_their_side():
    assert sounds.sound("lead – kit.standard", None, "lead") is None         # a kit never plays a melody
    assert N[sounds.sound("lead – kit.standard", 4, "lead")] == "TINE EP"    # (its program does)
    assert sounds.sound("drums – keys.piano", None, "drums") is None          # nor a piano the drums
    assert N[sounds.sound("drums – keys.piano", 25, "drums")] == "808"
    assert N[sounds.sound("drums – kit.standard", 25, "drums")] == "STD KIT"  # (the instrument first)
    assert N[sounds.sound("lead – keys.piano", 25, "lead")] == "SINE KEY"


def test_22_6_role_defaults_listed_once_in_the_report(tmp_path):
    notes = [(480 * k, 60, 240, 90) for k in range(4)]
    f = make_midi(tmp_path / "u.mid", [("lead – mystery.horn", 0, notes), ("chords", 1, notes),
                                       ("Track 3", 2, notes), ("bass – mystery.bass", 3, notes,
                                                               [mido.Message("program_change", channel=3, program=38)]),
                                       ("", 4, notes)])
    piece, rep = fit(str(f))
    assert [(c.role, c.sound) for c in piece.channels] == [("bass", 10), ("lead", 255), ("chords", 255),
                                                           ("other", 255), ("other", 255)]
    assert rep.lines == ["role default sound for 'lead – mystery.horn', 'chords', 'Track 3', 'other' "
                         "(no known instrument or GM program)"]


def test_22_6_more_fm6_sounds_than_the_pool_take_their_nearest_sound(tmp_path):
    notes = [(480 * k, 60, 240, 90) for k in range(4)]
    parts = [("bass – bass.upright", 0, notes), ("lead – vibes.vibraphone", 1, notes),
             ("chords – guitar.nylon", 2, notes), ("pad 3", 3, notes, [mido.Message("program_change", channel=3,
                                                                                         program=8)]),
             ("counter – keys.rhodes", 4, notes)]
    piece, rep = fit(str(make_midi(tmp_path / "f.mid", parts)))
    assert [N[c.sound] for c in piece.channels] == ["ROUND BASS", "WOOD BARS", "NYLON PICK", "BELL", "SINE KEY"]
    assert [c.sound for c in piece.channels] == [74, 77, 79, 19, 8]
    assert rep.lines == ["pad: GLASS BELL plays as BELL (the FM-1 plays at most 3 FM6 sounds at once)",
                         "counter: TINE EP plays as SINE KEY (the FM-1 plays at most 3 FM6 sounds at once)"]
    piece, rep = fit(str(make_midi(tmp_path / "g.mid", parts[:3] + parts[4:])))
    assert [c.sound for c in piece.channels] == [74, 77, 79, 8] and len(rep.lines) == 1


def test_22_6_a_program_byte_past_127_is_clipped_like_every_data_byte(tmp_path):
    """the reader clips data bytes (mido's clip=True, Studio's smf.rs the same): 0xC0 0x85 is program 127"""
    trk = b"\x00\xff\x03\x05Track\x00\xc0\x85\x00\x90\x3c\x40\x60\x80\x3c\x00\x00\xff\x2f\x00"
    p = tmp_path / "c.mid"
    p.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 1, 1, 96) + b"MTrk" + struct.pack(">I", len(trk)) + trk)
    piece, rep = fit(str(p))
    assert [(c.role, N[c.sound]) for c in piece.channels] == [("fx", "METAL")] and rep.reductions == 0


def test_34_a_fourth_fm6_channel_plays_its_roles_default(tmp_path):
    """only 3 FM6 engine blocks: a piece asking a 4th (channel 4, bass) gets the bass default, as if it asked for it
    (COMPOSER tells the synth each channel's role with its sound); heard alone, the two runs are the same samples"""
    from test_ui import at, push, script
    from fm1c.piece import Channel, Note, Piece, encode
    from conftest import wav_frames

    def run(sound4, name):
        roles = ["lead", "pad", "chords", "bass"]
        chans = [Channel(r, [Note(t * 96, 40 + 7 * c + t % 3, 80, 110) for t in range(32)], sound=s)
                 for c, (r, s) in enumerate(zip(roles, [72, 73, 74, sound4]))]
        lines, t = push(100, encode(Piece("fm6x4", 1200, 8 * 384, chans)))
        lines += [f"{t + 50} btn 12", f"{t + 80} keys {1 | 4 | 16}", f"{t + 140} keys 0"]   # play; mute 1-3
        log, wav = run_host(script(tmp_path, name, lines, t + 4000))
        assert (64 + 3 * 19, "BASS") in at(log, t + 50)
        f = wav_frames(wav)
        return f[int((t + 1500) / 1000 * 44100) * 2:int((t + 3900) / 1000 * 44100) * 2]
    a, b = run(75, "fm6four"), run(255, "fm6dflt")
    assert max(abs(x) for x in b) > 1000 and a == b
