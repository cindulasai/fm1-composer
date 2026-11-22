"""US-8: COMPOSER's screens through host mode: pieces pushed over SysEx (the real transfer path), buttons, encoders
and keys from a script; asserted on the screen text the firmware draws and the sound it makes."""
import struct, sys
from conftest import APP, run_host, screen, wav_frames

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, Channel, Note, encode  # noqa: E402

ROLES = ["lead", "bass", "drums", "pad", "chords", "counter", "arp", "fx"]
BTN = {"SEL": 3, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"SELECT": 0, "KNOB1": 2, "KNOB2": 3, "PRESETS": 6}


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
    assert (64 + 19, new[-1]) in at(log2, 100), "the saved sound comes back after a power cycle"


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
