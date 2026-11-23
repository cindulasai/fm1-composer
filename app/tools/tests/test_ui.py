"""US-8: COMPOSER's screens through host mode: pieces pushed over SysEx (the real transfer path), buttons, encoders
and keys from a script; asserted on the screen text the firmware draws and the sound it makes."""
import struct, sys
from conftest import APP, run_host, screen, screen_full, wav_frames

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, Channel, Note, encode  # noqa: E402

ROLES = ["lead", "bass", "drums", "pad", "chords", "counter", "arp", "fx"]
BTN = {"SEL": 3, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB2": 3, "PRESETS": 6}


AMBER = (255 >> 3) << 11 | (165 >> 2) << 5


def piece(name, nch=8, bars=8):
    chans = []
    for c in range(nch):
        notes = [Note(t * 96, (36 if ROLES[c] == "drums" else 48 + c * 3) + t % 3, 80, 110) for t in range(bars * 4)]
        chans.append(Channel(ROLES[c], notes))
    return encode(Piece(name, 1200, bars * 384, chans))


def push(t, data):
    """script lines pushing one piece over SysEx from t ms (a frame every 2 ms); (lines, t after)"""
    out = [f"{t} sysex {link.frame(3, struct.pack('<I', len(data))).hex()}"]
    for off in range(0, len(data), 256):
        t += 2
        out.append(f"{t} sysex {link.frame(4, struct.pack('<I', off) + data[off:off + 256]).hex()}")
    t += 2
    out.append(f"{t} sysex {link.frame(5).hex()}")
    return out, t + 10


def script(tmp_path, name, lines, end):
    p = tmp_path / f"{name}.txt"
    p.write_text("\n".join(lines + [f"{end} end"]) + "\n")
    return p


def at(log, ms0, ms1=10 ** 9):
    return [(y, txt) for ms, y, txt in screen(log) if ms0 <= ms <= ms1]


def rms(frames, s0, s1):
    seg = frames[int(s0 * 44100) * 2:int(s1 * 44100) * 2]
    return (sum(v * v for v in seg) / max(1, len(seg))) ** 0.5


def library_with(tmp_path, names, extra, end, name=None, env=None):
    lines, t = [], 100
    for n in names:
        l, t = push(t, piece(n))
        lines += l
    lines += [f"{t + ms} {what}" for ms, what in extra]
    return run_host(script(tmp_path, name or tmp_path.name, lines, end), env=env), t


def test_8_1_library_lists_newest_first_and_play_opens(tmp_path):
    (log, wav), t = library_with(tmp_path, ["alpha", "beta", "gamma"],
                                 [(50, f"enc {ENC['SELECT']} 1"), (100, f"btn {BTN['PLAY']}")], 4000)
    s = at(log, 0, t + 40)
    assert (4, "3 PIECES") in s and (28, "> gamma") in s and (48, "  beta") in s and (68, "  alpha") in s
    assert any(txt.endswith("KB FREE") for _, txt in s)
    assert (48, "> beta") in at(log, t + 50, t + 90)
    after = at(log, t + 100)
    assert (4, "beta") in after and (222, "PLAYING") in after
    f = wav_frames(wav)
    assert rms(f, 0, t / 1000) == 0 and rms(f, t / 1000 + 0.3, 3.9) > 300


def test_8_2_player_strips_and_bar_beat(tmp_path):
    (log, _), t = library_with(tmp_path, ["eight"], [(50, f"btn {BTN['PLAY']}")], 5000)
    s = at(log, t + 50)
    for c, role in enumerate(ROLES):
        assert (64 + c * 19, role.upper() if role != "counter" else "COUNTER") in s, role
    assert (64 + 2 * 19, "STD KIT") in s and (64, "SAW LEAD") in s and (64 + 19, "SQR BASS") in s
    beats = [txt for y, txt in s if y == 26]
    assert beats[:6] == ["1:1", "1:2", "1:3", "1:4", "2:1", "2:2"], beats[:8]


def test_8_3_sound_change_heard_and_saved_across_reboot(tmp_path):
    flash = str(tmp_path / "flash.bin")
    keys = [(50, f"btn {BTN['PLAY']}"), (600, f"enc {ENC['KNOB1']} 1"), (650, f"enc {ENC['PRESETS']} 5"),
            (800, "keys 0x10"), (850, "keys 0x0"),
            (900, f"btn {BTN['SAVE']}"), (1500, f"btn {BTN['PLAY']}")]
    (log, wav), t = library_with(tmp_path, ["keep"], keys, 2500, name="save1", env={"HOST_FLASH": flash})
    s = at(log, t + 600)
    new = [txt for y, txt in s if y == 64 + 19 and txt not in ("BASS", " 2", ">2", "")]
    assert new and new[-1] != "SQR BASS"
    assert (222, "SAVES WHEN STOPPED") in s and (222, "SAVED") in at(log, t + 1500) and "blob 0 saved" in log
    (log0, wav0), _ = library_with(tmp_path, ["keep"], keys[:2], 2500, name="save0")
    a, b = wav_frames(wav0), wav_frames(wav)
    turn = int((t + 650) / 1000 * 44100) * 2
    assert a[:turn - 2000] == b[:turn - 2000] and a[turn + 2000:turn + 40000] != b[turn + 2000:turn + 40000]   # heard
    (log2, _) = run_host(script(tmp_path, "save2", [f"100 btn {BTN['PLAY']}"], 1000), env={"HOST_FLASH": flash})
    back = at(log2, 100)
    assert (64 + 19, new[-1]) in back, "the saved sound comes back after a power cycle"
    assert (64 + 2 * 19, "M") in back and (64 + 3 * 19, "M") not in back, "so does the mute"


def test_8_4_mute_solo_and_home(tmp_path):
    keys = [(50, f"btn {BTN['PLAY']}"), (300, "keys 0x1"), (350, "keys 0x0"),
            (500, f"btndown {BTN['SEL']}"), (550, "keys 0x4"), (600, "keys 0x0"), (650, f"btnup {BTN['SEL']}"),
            (1000, f"btn {BTN['HOME']}")]
    (log, wav), t = library_with(tmp_path, ["mix"], keys, 2500)
    s = at(log, t + 300, t + 450)
    assert (64, "M") in s
    s = at(log, t + 550, t + 900)
    assert (64 + 19, "S") in s and (64, "M") in s
    s = at(log, t + 1000)
    assert (4, "COMPOSER") in s and (28, "> mix") in s
    f = wav_frames(wav)
    assert rms(f, (t + 1100) / 1000, 2.4) < 20


def test_8_5_mashing_and_deleting_the_playing_piece(tmp_path):
    mash = [(i * 3, f"btn {BTN[b]}") for i, b in enumerate(["PLAY", "HOME", "SAVE", "PLAY"] * 10)]
    log, _ = run_host(script(tmp_path, "empty", [f"{100 + ms} {w}" for ms, w in mash], 800))
    assert all(txt != "PLAYING" for _, txt in at(log, 0)) and (112, "LIBRARY EMPTY") in at(log, 0)
    (log, wav), t = library_with(tmp_path, ["doomed", "other"],
                                 [(10, f"enc {ENC['SELECT']} 1"), (20, f"btn {BTN['PLAY']}")]
                                 + [(30 + i, f"btn {BTN['PLAY']}") for i in range(0, 20, 2)]
                                 + [(500, f"sysex {link.frame(6, struct.pack('<I', 0)).hex()}"),
                                    (1500, f"btn {BTN['HOME']}")], 2500)   # (1 + 10 presses: playing)
    assert "SYSEX out F07D463146" in log                      # deleted while playing
    f = wav_frames(wav)
    assert rms(f, (t + 600) / 1000, (t + 1400) / 1000) > 300  # still playing from RAM
    s = at(log, t + 1500)
    assert (4, "1 PIECE") in s and (28, "> other") in s and all("doomed" not in txt for _, txt in s)


def test_8_1_long_library_scrolls_with_the_cursor(tmp_path):
    names = [f"p{k:02}" for k in range(11)]               # newest first on screen: p10 .. p00
    (log, _), t = library_with(tmp_path, names, [(50, f"enc {ENC['SELECT']} 9")], 600)
    s = screen_full(log)
    rows = [(y, txt) for ms, y, x, c, txt in s if ms >= t + 50 and x == 4]
    assert (28 + 8 * 20, "> p01") in rows and (28, "  p09") in rows, rows   # scrolled one row
    amber = [txt for ms, y, x, c, txt in s if ms >= t + 50 and x == 4 and c == AMBER]
    assert amber == ["> p01"], amber                                    # only the cursor row is lit
    assert all(y < 28 + 9 * 20 for ms, y, x, c, txt in s if x == 4 and y >= 28), "nine rows, clear of the footer"


def test_8_2_fewer_channels_draw_fewer_strips(tmp_path):
    lines, t = push(100, piece("trio", nch=3))
    log, _ = run_host(script(tmp_path, "trio", lines + [f"{t + 50} btn {BTN['PLAY']}"], t + 400))
    ys = {y for y, txt in at(log, t + 50) if txt}
    assert 64 + 2 * 19 in ys and not ys & {64 + c * 19 for c in range(3, 8)}


def test_8_1_audio_has_no_gaps(tmp_path):
    from fm1c.piece import Piece, Channel, Note, encode
    pad = encode(Piece("pad", 1200, 384 * 4, [Channel("pad", [Note(0, 60, 384 * 4 - 1, 100)])]))
    lines, t = push(100, pad)
    _, wav = run_host(script(tmp_path, "gaps", lines + [f"{t + 20} btn {BTN['PLAY']}"], t + 1500))
    f = wav_frames(wav)
    left = f[int((t + 300) / 1000 * 44100) * 2:int((t + 1400) / 1000 * 44100) * 2:2]
    run = best = 0
    for v in left:
        run = run + 1 if v == 0 else 0
        best = max(best, run)
    assert best < 8, best                                   # a held pad: no silent blocks


def catalog(tmp_path):
    """the sound names in PRESETS order (the firmware's own catalog, via render_piece --sounds)"""
    import subprocess
    exe = tmp_path / "render_piece"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                    str(APP / "tests/render_piece.c"), "-lm"], check=True)
    return subprocess.run([str(exe), "--sounds"], capture_output=True, text=True, check=True).stdout.splitlines()


def shot(name, ms):
    from PIL import Image
    return Image.open(APP / f"build/host/{name}-{ms}.ppm").convert("RGB")


def test_8_1_cursor_stops_at_the_last_piece_and_sizes_show(tmp_path):
    (log, _), t = library_with(tmp_path, ["alpha", "beta", "gamma"], [(50, f"enc {ENC['SELECT']} 3")], 400)   # (to one past the end)
    assert (68, "> alpha") in at(log, t + 50)
    size = f"{(len(piece('alpha')) + 1023) // 1024}K"
    assert [txt for ms, y, x, c, txt in screen_full(log) if ms >= t + 50 and y == 68 and x == 194] == [size]


def test_8_2_select_tempo_and_sound_turns_exactly(tmp_path):
    names = catalog(tmp_path)
    k = names.index("STD KIT")
    keys = [(50, f"btn {BTN['PLAY']}"), (200, f"enc {ENC['SELECT']} 2"), (300, f"enc {ENC['KNOB2']} 3"),
            (400, f"enc {ENC['PRESETS']} 1"), (500, f"enc {ENC['PRESETS']} -2")]
    (log, _), t = library_with(tmp_path, ["turns"], keys, 800)
    y3 = 64 + 2 * 19
    s = screen_full(log)
    assert [c for ms, y, x, c, txt in s if t + 200 <= ms < t + 300 and y == y3 and txt == ">3"] == [AMBER]
    assert (4, "123 BPM") in at(log, t + 300, t + 399)               # 1 BPM a click
    kits = names[:3] + names[40:72]                                   # (v5: a drum channel turns through the kits)
    assert (y3, kits[(kits.index(names[k]) + 1) % len(kits)]) in at(log, t + 400, t + 499)
    assert (y3, kits[(kits.index(names[k]) - 1) % len(kits)]) in at(log, t + 500)


def test_8_2_a_sound_outside_the_catalog_falls_back_to_the_roles(tmp_path):
    p = encode(Piece("odd", 1200, 384, [Channel("lead", [Note(0, 60, 80, 100)], sound=200),
                                         Channel("bass", [Note(0, 40, 80, 100)], sound=254)]))
    lines, t = push(100, p)
    log, _ = run_host(script(tmp_path, "odd", lines + [f"{t + 50} btn {BTN['PLAY']}"], t + 300))
    s = at(log, t + 50)
    assert (64, "SAW LEAD") in s and (83, "SQR BASS") in s


def test_8_5_library_emptied_while_open_is_cleared(tmp_path):
    lines, t = push(100, piece("only"))
    lines += [f"{t + 50} sysex {link.frame(6, struct.pack('<I', 0)).hex()}", f"{t + 150} shot"]
    log, _ = run_host(script(tmp_path, "emptied", lines, t + 250))
    assert (112, "LIBRARY EMPTY") in at(log, t + 50)
    im = shot("emptied", t + 150)
    assert all(im.getpixel((x, y)) == (0, 0, 0) for y in range(28, 108) for x in range(240)), "the old row is gone"


def test_8_4_footer_survives_a_strip_redraw(tmp_path):
    keys = [(50, f"btn {BTN['PLAY']}"), (300, "keys 0x1"), (350, "keys 0x0"), (450, "shot")]
    (log, _), t = library_with(tmp_path, ["foot"], keys, 600, name="foot")
    im = shot("foot", t + 450)
    assert any(im.getpixel((x, y)) != (0, 0, 0) for y in range(222, 238) for x in range(240)), "PLAYING still shown"
