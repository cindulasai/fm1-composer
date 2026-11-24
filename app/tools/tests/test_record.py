"""US-26: REC, live play recorded into the selected channel. SLOOP's latency compensation as copied (26.1), and the
PLAYER through host mode (26.3): play, REC (a bar of clicks), a phrase on the keys, stop, SAVE, reboot, and the
phrase plays back from the piece at the ticks it was played at. The recorder's timing, grid, overdub / replace,
undo and hostile passes are tests/live_test.c (26.1, 26.2, 26.4)."""
import re, shutil, sys
import pytest
from conftest import APP, run_host, screen, screen_full, wav_frames
from test_ui import push, script, at
from test_link import host_bin  # noqa: F401  (the fixture: xfer.c + lib.c on this computer)
from test_cli import fm1c
from test_live import goertzel, hz

sys.path.insert(0, str(APP / "tools"))
from fm1c.piece import Channel, Piece, Section, decode, encode  # noqa: E402

BTN = {"GLO": 7, "SAVE": 9, "ARP": 10, "PLAY": 12, "REC": 13}
ENC = {"ALGORITHM": 1}
SLOOP_SEQ_BLOB = "379606c94bf2cbe2fa8ed2bcc909b22891f32782"      # firmware/src/seq.c @ bac9c05
RED = (255 >> 3) << 11 | (48 >> 2) << 5 | 48 >> 3
TICK = 60 / 120 / 96                                              # seconds a tick at 120 BPM


def test_26_1_sloops_latency_compensation_copied_unchanged():
    """live.c's recording block is seq.c @ bac9c05 lines 473-495 (REC_LAT 512, rec_target) line for line"""
    from test_fm6 import blob, sloop_src
    live = (APP / "firmware/src/live.c").read_text()
    head = "/* ---- SLOOP firmware/src/seq.c @ bac9c05, unchanged (recording) ---- */"
    body = live[live.index(head):live.index("/* ---- end of SLOOP's code (recording) ---- */")]
    assert "#define REC_LAT 512u" in body and "static uint32_t rec_target(const track_t *t, uint32_t *later)" in body
    prov = (APP / "firmware/SLOOP-PROVENANCE.md").read_text()
    row = re.search(r"^\| app/firmware/src/live\.c \| firmware/src/seq\.c \((.*)\) \| " + SLOOP_SEQ_BLOB, prov, re.M)
    assert row and "REC_LAT" in row[1] and "rec_target" in row[1] and "473-495" in prov
    src = sloop_src()
    if src is None:
        pytest.skip("SLOOP's source is not here (research/sloop-src or $SLOOP_SRC)")
    assert blob(src / "firmware/src/seq.c") == SLOOP_SEQ_BLOB
    seq = (src / "firmware/src/seq.c").read_text().splitlines()
    assert body.splitlines()[1:] == seq[472:495]


# ---- 26.3: host mode ----
def rec_piece():
    """C major (carried): channel 1 LEAD, channel 2 BASS, both empty: 4 bars at 120"""
    return encode(Piece("rec", 1200, 4 * 384, [Channel("lead"), Channel("bass")], [Section(0, "A")], (0, 1)))


PHRASE = [(4250, 7, 200, 60, 816), (4500, 9, 200, 62, 864), (4750, 11, 200, 64, 912), (5000, 12, 600, 65, 960)]
#          ms after PLAY, key (C4 D4 E4 F4), held ms, the note (C major, C4 the root), its tick (1/16 grid)


def test_26_3_play_rec_a_phrase_stop_save_reboot_it_plays_back(tmp_path, host_bin):
    flash = str(tmp_path / "flash.bin")
    lines, t = push(100, rec_piece())
    p0 = t + 50                                                   # LIBRARY: PLAY opens the piece and plays it
    steps = [(0, f"btn {BTN['PLAY']}"), (100, f"btn {BTN['ARP']}"),        # KEYS on
             (150, f"enc {ENC['ALGORITHM']} 1"), (200, f"enc {ENC['ALGORITHM']} -1"),   # the grid: 1/8, back to 1/16
             (250, f"btn {BTN['GLO']}"), (300, f"btn {BTN['GLO']}"),       # REPLACE, back to OVERDUB
             (1000, f"btn {BTN['REC']}")]                          # at tick ~192: bar 2 counts in, bar 3 records
    for ms, key, hold, _, _ in PHRASE:
        steps += [(ms, f"keys {1 << key}"), (ms + hold, "keys 0")]
    steps += [(6500, f"btn {BTN['PLAY']}"), (6700, f"btn {BTN['SAVE']}")]
    lines += [f"{p0 + ms} {what}" for ms, what in steps]
    log, wav = run_host(script(tmp_path, "rec1", lines, p0 + 7000), env={"HOST_FLASH": flash})
    foot = [txt for ms, y, txt in screen(log) if y == 222 and ms >= p0]
    for want in ("KEYS C MAJ", "REC 1/8 OVERDUB", "REC 1/16 OVERDUB", "REC 1/16 REPLACE", "REC IN 4", "REC IN 3", "REC IN 2",
                 "REC IN 1", "RECORDING 1/16 OVERDUB", "REC OFF: 4 NOTES", "SAVED"):
        assert want in foot, (want, foot)
    assert foot.index("REC IN 1") < foot.index("RECORDING 1/16 OVERDUB") < foot.index("REC OFF: 4 NOTES") < foot.index("SAVED")
    marks = [(ms, txt) for ms, y, x, c, txt in screen_full(log) if y == 30 and x == 4 and c == RED]
    assert any(txt == "IN" and p0 + 1000 <= ms < p0 + 4000 for ms, txt in marks), "the count-in shows"
    assert any(txt == "REC" and p0 + 3900 <= ms <= p0 + 4200 for ms, txt in marks), "REC shows while recording"
    f = wav_frames(wav)
    b = p0 / 1000
    quiet = goertzel(f, b + 1.7, b + 1.95, 2000) + goertzel(f, b + 1.7, b + 1.95, 1000)
    assert goertzel(f, b + 1.995, b + 2.03, 2000) > 100 * quiet, "the count-in's first click (high) at bar 2"
    for beat in (1, 2, 3):
        s = b + 2 + beat * 0.5
        assert goertzel(f, s - 0.005, s + 0.03, 1000) > 100 * quiet, f"click {beat + 1} on beat {beat + 1}"
    assert goertzel(f, b + 4.03, b + 4.2, 2000) + goertzel(f, b + 4.03, b + 4.2, 1000) < 2 * quiet + 1, "no click once recording"

    fl = tmp_path / "xflash.bin"                                  # the saved piece, as fm1c pulls it
    shutil.copy(flash, fl)
    port = f"exec:{host_bin} {fl}"
    pid = fm1c("list", "--port", port).stdout.split()[0].lstrip("#")
    assert fm1c("pull", pid, "-o", tmp_path / "back.f1p", "--port", port).returncode == 0
    raw = (tmp_path / "back.f1p").read_bytes()
    back = decode(raw)
    assert encode(back) == raw and back.channels[1].notes == [] and back.len_ticks == 4 * 384 and back.key == (0, 1)
    got = [(n.tick, n.pitch, n.vel) for n in back.channels[0].notes]
    assert got == [(tick, note, 100) for _, _, _, note, tick in PHRASE], got
    for n, (_, _, hold, _, _) in zip(back.channels[0].notes, PHRASE):
        assert abs(n.dur - hold / 1000 / TICK) <= 5, (n, hold)      # the length it was held, in ticks

    log2, wav2 = run_host(script(tmp_path, "rec2", ["100 btn 12"], 6000), env={"HOST_FLASH": flash})   # reboot, PLAY
    assert (222, "PLAYING") in at(log2, 100)
    f2 = wav_frames(wav2)
    for _, _, hold, note, tick in PHRASE:                         # each note at its tick, from the piece
        s = 0.1 + tick * TICK
        on = goertzel(f2, s + 0.02, s + 0.15, hz(note))
        assert on > 20 * goertzel(f2, 0.2, 0.6, hz(note)), ("silent before the phrase", note)
        assert on > 5 * goertzel(f2, s - 0.12, s - 0.01, hz(note)), ("it starts at its tick", note, tick)
