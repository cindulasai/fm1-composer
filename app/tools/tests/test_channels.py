"""US-39: add / remove channels on the FM-1 (host mode). PLAYER, holding SEL: OCT+ adds a channel after the last
(selected, its role the first of NEW SONG's the piece lacks), OCT- asks to remove the selected one (PLAY = yes, any
other button keeps it). The per-channel choices move with their channels; the piece is edited (SAVE as any edit).
The piece's bytes against a model: tests/edit_test.c (39.1)."""
import sys
from conftest import APP, run_host, screen, wav_frames
from test_ui import push, script, at
from test_link import host_bin  # noqa: F401  (the fixture)
from test_new_song import pulled
from test_live import goertzel, hz
from test_live_send import piece as eight

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Channel, Note, Piece, Section, decode, encode  # noqa: E402

BTN = {"OCT_DOWN": 0, "OCT_UP": 1, "SEL": 3, "LFO": 5, "HOME": 8, "SAVE": 9, "PLAY": 12}
ENC = {"PRESETS": 6}
FOOT = 222
AUTO = 1


def add_keys(ms):
    return [(ms, f"btndown {BTN['SEL']}"), (ms + 20, f"btn {BTN['OCT_UP']}"), (ms + 40, f"btnup {BTN['SEL']}")]


def remove_keys(ms):
    return [(ms, f"btndown {BTN['SEL']}"), (ms + 20, f"btn {BTN['OCT_DOWN']}"), (ms + 40, f"btnup {BTN['SEL']}")]


def run(tmp_path, name, p, steps, end, flash=None, play=False):
    """p pushed and opened (stopped, unless play); then steps [(ms after t0, what)]: (log, wav), t0"""
    lines, t = push(100, p)
    lines += [f"{t + 50} btn {BTN['PLAY']}"] + ([] if play else [f"{t + 100} btn {BTN['PLAY']}"])
    t0 = t + 150
    for ms, what in steps:
        if isinstance(what, bytes):
            l, _ = push(t0 + ms, what)
            lines += l
        else:
            lines.append(f"{t0 + ms} {what}")
    return run_host(script(tmp_path, name, lines, t0 + end), env={"HOST_FLASH": str(flash)} if flash else None), t0


def three(name="three"):
    """LEAD (A4), BASS (A2), PAD (E5), a note each beat; 2 sections, 120 BPM, 8 bars"""
    pitch = {"lead": 69, "bass": 45, "pad": 76}
    chans = [Channel(r, [Note(b * 96, pitch[r], 80, 100) for b in range(32)], vol=110 - 10 * k, pan=30 + 20 * k)
             for k, r in enumerate(pitch)]
    return encode(Piece(name, 1200, 8 * 384, chans, [Section(0, "A"), Section(4 * 384, "B")], (0, 1)))


def test_39_2_add_pick_its_sound_fill_it_remove_another_save(tmp_path, host_bin):
    flash = tmp_path / "f.bin"
    steps = [*add_keys(0), (100, f"enc {ENC['PRESETS']} 1"),
             (200, f"btn {BTN['LFO']}"), (300, "keys 128"), (360, "keys 0"), (500, f"btn {BTN['LFO']}"),
             (600, "enc 0 -3"), *remove_keys(700), (800, f"btn {BTN['HOME']}"),           # asked, then kept
             *remove_keys(900), (1000, f"btn {BTN['PLAY']}"), (1100, f"btn {BTN['SAVE']}")]
    (log, _), t0 = run(tmp_path, "ch2", three(), steps, 1300, flash=flash)
    assert (FOOT, "CH 4 ADDED: PRESETS PICKS ITS SOUND") in at(log, t0, t0 + 99)
    row = [txt for y, txt in at(log, t0, t0 + 99) if y == 115]                 # (rows 17 apart with sections)
    assert ">4" in row and "DRUMS" in row and "STD KIT" in row, row          # selected; the first role the piece lacks
    kit = [txt for y, txt in at(log, t0 + 100, t0 + 199) if y == 115 and txt not in ("", ">4", "DRUMS")]
    assert kit and kit[0] != "STD KIT", kit                                   # PRESETS: its sound
    assert (4, "STEPS 4 DRUMS *") in at(log, t0 + 200, t0 + 499)              # STEPS fills the new channel
    assert (FOOT, "REMOVE CH 1? PLAY = YES") in at(log, t0 + 700, t0 + 799)
    assert (FOOT, "KEPT CH 1") in at(log, t0 + 800, t0 + 899)
    assert (FOOT, "CH 1 REMOVED") in at(log, t0 + 1000, t0 + 1099) and (FOOT, "SAVED") in at(log, t0 + 1100)
    back = pulled(tmp_path, host_bin, flash)["three"]
    old = decode(three())
    want = Piece("three", 1200, 8 * 384, old.channels[1:] + [Channel("drums", [Note(0, 60, 24, 100)], mode=AUTO)],
                 old.sections, old.key)
    assert back == encode(want), (decode(back), want)                         # sections, key, volumes and pans kept


def test_39_3_eight_max_one_min_and_no_room(tmp_path):
    (log, _), t0 = run(tmp_path, "ch8", eight("alpha"), [*add_keys(0)], 200)
    assert (FOOT, "8 CHANNELS MAX") in at(log, t0)
    one = encode(Piece("one", 1200, 4 * 384, [Channel("lead", [Note(0, 60, 96, 100)])]))
    (log, _), t0 = run(tmp_path, "ch1", one, [*remove_keys(0), (100, f"btn {BTN['PLAY']}")], 300)
    assert (FOOT, "A SONG NEEDS 1 CHANNEL") in at(log, t0, t0 + 99) and (FOOT, "STOPPED") not in at(log, t0 + 100)
    full = encode(Piece("full", 1200, 999 * 384, [Channel("lead", [Note(i, 60 + i % 12, 10, 90) for i in range((32768 - 68 - 8) // 6)])]))
    assert 32768 - 16 < len(full) <= 32768
    (log, _), t0 = run(tmp_path, "chfull", full, [*add_keys(0)], 200)
    assert (FOOT, "PIECE FULL") in at(log, t0)


def test_39_3_removed_while_playing_lets_go_mutes_follow(tmp_path):
    steps = [(400, "keys 16"), (460, "keys 0"),                               # mute channel 3 (PAD; A3, the 3rd white key)
             *remove_keys(1000), (1100, f"btn {BTN['PLAY']}")]                 # remove channel 2 (BASS) while playing
    (log, wav), t0 = run(tmp_path, "chplay", three(), [(0, "enc 0 1"), *steps], 3000, play=True)
    assert (FOOT, "CH 2 REMOVED") in at(log, t0 + 1100)
    f = wav_frames(wav)
    s = (t0 + 1150) / 1000
    before, after = (s - 0.6, s - 0.1), (s + 0.3, s + 1.6)
    assert goertzel(f, *before, hz(45)) > 50 * (goertzel(f, *after, hz(45)) + 1), "the bass let go at once"
    assert goertzel(f, *after, hz(69)) > 0.3 * goertzel(f, *before, hz(69)), "the lead plays on"
    assert goertzel(f, *after, hz(76)) * 50 < goertzel(f, *after, hz(69)), "the pad, now channel 2, still muted"


def test_39_3_a_live_send_waiting_holds_for_the_stop(tmp_path):
    steps = [(300, three("gamma")), *add_keys(1000)]
    (log, _), t0 = run(tmp_path, "chlive", three(), steps, 1400, play=True)
    texts = [txt for y, txt in at(log, t0 + 300) if y == FOOT]
    assert "NEXT: gamma" in texts and "NEXT AT STOP: gamma" in texts and "LIVE: gamma" not in texts, texts
