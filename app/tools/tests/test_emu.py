"""US-9: the firmware CI builds, in fm1-emu: boot screen, pushes over emulated USB, playing heard on USB audio,
power cut at every flash write of a save, strict on-chip timing. Screens are compared, pixel for pixel (RGB565),
with host mode running the same steps. Run by tests/emu_e2e.sh (FM1_EMU_RUN=1); the SKIP and `fm1c emu` checks
run in the plain gate."""
import os, re, struct, subprocess, sys
from pathlib import Path
import pytest
from conftest import APP, run_host

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, PieceError, Channel, Note, encode  # noqa: E402
from test_cli import fm1c  # noqa: E402

LIVE, FWSC = os.environ.get("FM1_LIVE", ""), os.environ.get("FM1_FWSC", "")
emu_only = pytest.mark.skipif(os.environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh")
ROLES = ["lead", "bass", "drums", "pad", "chords", "counter", "arp", "fx"]


def piece(name, bars=4):
    chans = [Channel(r, [Note(t * 96, (36 if r == "drums" else 48 + c * 3) + t % 3, 80, 110) for t in range(bars * 4)])
             for c, r in enumerate(ROLES)]
    return encode(Piece(name, 1200, bars * 384, chans))


def dense(sounds=None):
    """as many notes as a piece may hold: 8 channels of 16th-note pairs, near the 32 KB limit (sounds: one id a
    channel, else each role's default)"""
    best, bars = None, 1
    while True:
        chans = [Channel(r, [Note(s * 24, (38 if r == "drums" else 40 + c * 5) + k * 7 + s % 5, 20, 100)
                             for s in range(bars * 16) for k in range(2)], **({"sound": sounds[c]} if sounds else {}))
                 for c, r in enumerate(ROLES)]
        try:
            best, bars = encode(Piece("dense", 1400, bars * 384, chans)), bars + 1
        except PieceError:                                   # (one bar more than fits)
            return best


def frames(data):
    return ([link.frame(3, struct.pack("<I", len(data)))]
            + [link.frame(4, struct.pack("<I", o) + data[o:o + 256]) for o in range(0, len(data), 256)] + [link.frame(5)])


def usb_script(pieces, t=1500, gap=100):
    """FM1_USB_SEND for the pushes (no answer is awaited, so a frame every `gap` ms); (script, ms after)"""
    out = []
    for d in pieces:
        for f in frames(d):
            out.append(f"{t}:{f.hex()}")
            t += gap
        t += 200
    return ";".join(out), t


def emu(tmp, ms, usb=None, keys=None, flash=None, cut=None, strict=False, wav=False):
    """one headless fm1-live run; (report text, LCD as RGB565 bytes, USB audio samples or None)"""
    work = tmp / "run"
    (work / "work").mkdir(parents=True, exist_ok=True)
    env = {**os.environ}
    for k in ("FM1_USB_SEND", "FM1_BENCH_KEYS", "FM1_FLASH_CUT", "FM1_STRICT", "FM1_USB_WAV"):
        env.pop(k, None)
    env["FM1_NO_PLAY"] = "1"                                 # (--shot presses PLAY by itself otherwise)
    args = [LIVE, "--fwsc", FWSC, "--shot", str(ms), "--lcd-only"]
    if usb:
        (work / "usb.txt").write_text(usb)
        env["FM1_USB_SEND"] = "@" + str(work / "usb.txt")
    if usb or wav:
        args.append("--usb")
    if keys:
        env["FM1_BENCH_KEYS"] = keys
    if flash:
        args += ["--flash", str(flash)]
    if cut:
        env["FM1_FLASH_CUT"] = str(cut)
    if strict:
        env["FM1_STRICT"] = "1"
    if wav:
        env["FM1_USB_WAV"] = str(work / "usb.raw")
    r = subprocess.run(args, cwd=work, env=env, capture_output=True, text=True, timeout=600)
    assert r.returncode == 0, r.stdout[-2000:] + r.stderr[-2000:]
    samples = None
    if wav:
        raw = (work / "usb.raw").read_bytes()
        samples = struct.unpack(f"<{len(raw) // 2}h", raw)
    return r.stdout + r.stderr, rgb565(work / "work/lcd.ppm"), samples


def rgb565(path):
    from PIL import Image
    im = Image.open(path).convert("RGB")
    rgb = im.tobytes()
    return bytes(v >> (2 if k % 3 == 1 else 3) for k, v in enumerate(rgb))


def host_ref(tmp, name, pieces):
    """the LIBRARY screen host mode draws after the same pushes"""
    lines, t = [], 100
    for d in pieces:
        for f in frames(d):
            lines.append(f"{t} sysex {f.hex()}")
            t += 2
        t += 20
    p = tmp / f"{name}.txt"
    p.write_text("\n".join(lines + [f"{t + 500} shot", f"{t + 600} end"]) + "\n")
    run_host(p)
    return rgb565(APP / f"build/host/{name}-{t + 500}.ppm")


def replies(report):
    """the reply commands the firmware sent, in order"""
    out = []
    for m in re.finditer(r"usb in:\s+([0-9a-f ]+)", report):
        p = link.parse(bytes.fromhex(m[1].replace(" ", "")))
        if p:
            out.append(p[0])
    return out


def rms(s, a=0, b=None):
    seg = s[a * 2:None if b is None else b * 2]
    return (sum(v * v for v in seg) / max(1, len(seg))) ** 0.5


@emu_only
def test_9_1_boot_screen_is_host_modes(tmp_path):
    _, lcd, _ = emu(tmp_path, 1000)
    run_host("boot")
    assert lcd == rgb565(APP / "build/host/boot-1000.ppm")


@emu_only
def test_9_2_push_two_pieces_over_usb_then_play(tmp_path):
    a, b = piece("alpha"), piece("beta")
    usb, t = usb_script([a, b])
    report, lcd, s = emu(tmp_path, t + 500, usb=usb, wav=True)
    n = len(frames(a)) - 2
    assert replies(report) == [0x43] + [0x44] * n + [0x45] + [0x43] + [0x44] * n + [0x45], replies(report)
    assert lcd == host_ref(tmp_path, "emu2", [a, b]), "LIBRARY lists both, as host mode does"
    assert s and rms(s) == 0, "silent until PLAY"
    report, lcd, s = emu(tmp_path, t + 2500, usb=usb, keys=f"{t + 300}:PLAY,{t + 350}:PLAY^", wav=True)
    assert rms(s, len(s) // 2 - 44100) > 300, "playing, heard on the USB audio"


@emu_only
def test_9_3_power_cut_at_every_flash_write_of_a_save(tmp_path):
    a, b = piece("alpha"), piece("beta")
    usb, t = usb_script([a, b])
    refs = [host_ref(tmp_path, "cut0", []), host_ref(tmp_path, "cut1", [a]), host_ref(tmp_path, "cut2", [a, b])]
    report, _, _ = emu(tmp_path, t, usb=usb, flash=tmp_path / "whole.bin")
    ops = int(re.search(r"flash: (\d+) erases / programs", report)[1])
    assert ops >= 4, report[-500:]
    states = []
    for n in range(1, ops + 1):
        f = tmp_path / f"cut{n}.bin"
        report, _, _ = emu(tmp_path, t, usb=usb, flash=f, cut=n)
        assert f"power cut at op {n}" in report
        _, lcd, _ = emu(tmp_path, 1000, flash=f)               # power back on
        assert lcd in refs, f"cut at op {n}: the library is not one of: empty, alpha, alpha + beta"
        states.append(refs.index(lcd))
    assert states == sorted(states) and 1 in states, states  # never a piece back from an unfinished save
    _, lcd, _ = emu(tmp_path, 1000, flash=tmp_path / "whole.bin")
    assert lcd == refs[2]
    f = tmp_path / f"cut{states.index(1) + 1}.bin"             # after a cut, the next push works
    report, _, _ = emu(tmp_path, 4000, usb=usb_script([piece("gamma")])[0], flash=f)
    assert replies(report)[-1] == 0x45, replies(report)


@emu_only
def test_9_4_strict_timing_densest_piece_never_starves(tmp_path):
    d = dense()
    assert len(d) > 30000
    usb, t = usb_script([d], gap=60)
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=f"{t + 200}:PLAY,{t + 250}:PLAY^", strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert rms(s, len(s) // 2 - 5 * 44100) > 300, "10 s of the densest piece, heard"


def strict_densest(tmp_path, d):
    """d pushed and played 10 s under on-chip timing: (empty halves, underruns, rms of the last 5 s)"""
    usb, t = usb_script([d], gap=60)
    report, _, s = emu(tmp_path, t + 10500, usb=usb, keys=f"{t + 200}:PLAY,{t + 250}:PLAY^", strict=True, wav=True)
    assert replies(report)[-1] == 0x45, "saved under on-chip timing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m, report[-800:]
    return m[1], m[2], rms(s, len(s) // 2 - 5 * 44100)


FM6_FIRST, KITS_FIRST = 72, 40                               # (v5 catalog: v4's 40, SLOOP's 32 kits, FM6's 8)


@emu_only
def fm6_presets():
    """FM6's presets as eng_fm6.c lists them: [(name, mono)]"""
    src = (APP / "engines/eng_fm6.c").read_text()
    return [(n, int(m)) for n, m in re.findall(r'^\s*\{"([^"]+)", \{[^}]*\}, \{[^}]*\}, -?\d+, (\d+), FX', src, re.M)]


def fm6_sixteen():
    """the most FM6 the firmware plays: its 3 FM6 channels (ENG_FM6_MAX) on polyphonic presets, each a 6-note chord a
    16th held a half note, so 18 FM6 notes always want the 16 voices (the rest of the channels silent)"""
    poly = [FM6_FIRST + k for k, (_, mono) in enumerate(fm6_presets()) if not mono][:3]
    best, bars = None, 1
    while True:
        chans = [Channel(r, [Note(s * 24, 48 + c * 4 + k * 3 + s % 3, 192, 100) for s in range(bars * 16) for k in range(6)],
                         sound=poly[c]) for c, r in enumerate(["lead", "pad", "chords"])]
        try:
            best, bars = (encode(Piece("fm6x16", 1400, bars * 384, chans)), chans), bars + 1
        except PieceError:
            return best


def host_render(tmp_path, d, name="p"):
    """the piece through the firmware's own player and synth on this computer (tests/render_piece.c): its report"""
    exe, f1p = tmp_path / "render_piece", tmp_path / f"{name}.f1p"
    if not exe.exists():
        subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                        str(APP / "tests/render_piece.c"), "-lm"], check=True)
    f1p.write_bytes(d)
    return subprocess.run([str(exe), str(f1p), str(tmp_path / f"{name}.wav"), "10"], capture_output=True, text=True,
                          check=True).stdout


@emu_only
def test_19_6_strict_timing_sixteen_fm6_voices(tmp_path):
    presets = fm6_presets()
    assert len(presets) == 8 and sum(m for _, m in presets) >= 1, presets      # (the table read: ROUND BASS is mono)
    d, chans = fm6_sixteen()
    assert len(chans) == 3 == int(re.search(r"#define ENG_FM6_MAX (\d+)u", (APP / "firmware/src/sloop_glue.c").read_text())[1])
    assert not [ch for ch in chans if presets[ch.sound - FM6_FIRST][1]], "no mono preset (one voice a channel)"
    host = host_render(tmp_path, d)
    assert " at once 16 (FM6 16)" in host, host                 # the firmware's synth sounds 16 FM6 voices at once
    empty, under, level = strict_densest(tmp_path, d)
    assert (empty, under) == ("0", "0") and level > 300


@emu_only
def test_21_5_strict_timing_densest_piece_on_sloop_kits(tmp_path):
    names = re.findall(r'"([^"]+)"', re.search(r"#define DS_KIT_NAME_LIST (.*)",
                                               (APP / "firmware/src/felucca_drumkits.h").read_text())[1])
    glitch = KITS_FIRST + names.index("GLITCH")             # (the costliest kit on the host, synth_test 21.5)
    for kit in (glitch, KITS_FIRST):
        d = dense([kit if r == "drums" else FM6_FIRST + c for c, r in enumerate(ROLES)])
        empty, under, level = strict_densest(tmp_path, d)
        assert (empty, under) == ("0", "0"), f"kit {kit}"
        assert level > 300, "heard"


PHYS_FIRST = 80                                                # (v5 catalog: after FM6's 72..79; US-20)


@emu_only
def test_20_6_strict_timing_densest_piece_all_on_phys(tmp_path):
    names = re.findall(r'^\s*\{"([^"]+)", \{PM_', (APP / "engines/eng_phys.c").read_text(), re.M)
    bowl = PHYS_FIRST + names.index("GLASS BOWL")           # (a bowed modal bank: the costliest of PHYS, DIGITAL and VOICE
    d = dense([bowl] * 8)                                    # a voice, SLOOP's cpu_baseline.txt; every channel on it, 16 voices)
    assert len(d) > 30000
    empty, under, level = strict_densest(tmp_path, d)
    assert (empty, under) == ("0", "0")
    # (a bowed sound retriggered every 16th never swells: quiet on the host too. Heard as loud as the firmware's own
    # code renders it on this computer, so no voice went missing under on-chip timing)
    exe, f1p = tmp_path / "render_piece", tmp_path / "bowl.f1p"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/engines", "-o", str(exe),
                    str(APP / "tests/render_piece.c"), "-lm"], check=True)
    f1p.write_bytes(d)
    r = subprocess.run([str(exe), str(f1p), str(tmp_path / "bowl.wav"), "10"], capture_output=True, text=True, check=True)
    host = float(re.search(r"rms (\d+)", r.stdout)[1])
    assert host > 20 and level > 0.5 * host, (level, host)


def test_9_5_missing_emulator_or_firmware_is_a_visible_skip(tmp_path):
    sh = APP / "tests/emu_e2e.sh"
    r = subprocess.run(["sh", str(sh)], env={**os.environ, "FM1_LIVE": str(tmp_path / "none")}, capture_output=True, text=True)
    assert r.returncode == 0 and r.stdout.startswith("emu e2e: SKIP") and "fm1-live" in r.stdout
    r = subprocess.run(["sh", str(sh)], env={**os.environ, "FM1_LIVE": "/bin/sh", "FM1_FWSC": str(tmp_path / "x.fwsc")},
                       capture_output=True, text=True)
    assert r.returncode == 0 and r.stdout.startswith("emu e2e: SKIP") and ".fwsc" in r.stdout
    gate = (APP / "check.sh").read_text()
    assert re.search(r"^sh tests/emu_e2e\.sh(\s+#.*)?$", gate, re.M), "the gate runs it, its line shown"


def test_9_5_fm1c_emu_starts_the_emulator(tmp_path):
    fake = tmp_path / "fm1-live"
    fake.write_text("#!/bin/sh\necho \"ARGS $*\"\n")
    fake.chmod(0o755)
    fw = tmp_path / "c.fwsc"
    fw.write_bytes(b"x")
    os.environ["FM1C_LIVE"] = str(fake)
    try:
        r = fm1c("emu", "--fwsc", fw, "--flash", tmp_path / "lib/f.bin")
        assert r.returncode == 0 and f"ARGS --fwsc {fw} --usb --flash {tmp_path / 'lib/f.bin'}" in r.stdout, r.stdout + r.stderr
        assert (tmp_path / "lib").is_dir()
        r = fm1c("emu", "--fwsc", tmp_path / "none.fwsc")
        assert r.returncode == 2 and "no COMPOSER firmware" in r.stderr and "Traceback" not in r.stderr
        os.environ["FM1C_LIVE"] = str(tmp_path / "nothing")
        r = fm1c("emu", "--fwsc", fw)
        assert r.returncode == 2 and "get_emu.sh" in r.stderr
    finally:
        os.environ.pop("FM1C_LIVE", None)


def test_7_3_default_port_is_the_fm1s_usb_name(monkeypatch):
    from test_link import fake_rtmidi
    fake_rtmidi(monkeypatch, ["Felucca (emu)"], ["Felucca (emu)"])
    monkeypatch.delenv("FM1C_PORT", raising=False)
    assert isinstance(link.open_port(None), link.MidiPort)
    fake_rtmidi(monkeypatch, ["other"], ["other"])
    with pytest.raises(link.PortError, match="FM-1"):
        link.open_port(None)


ENC_SELECT, ENC_KNOB1, ENC_KNOB2 = 0, 2, 3                  # fm1-emu's matrix encoders (FM-1 order: platform.c)


def fx_all_up(t):
    """encoder turns (emulator FM1_BENCH_KEYS, host lines) setting REVERB, DELAY, CHORUS, DRIVE to 127 on all 8
    channels from the FX screen; each turn played out before the next (fm1-emu moves a contact every 2 ms: a detent
    takes 8 ms, so 128 detents ~1 s, and a SELECT turned earlier would move the row under KNOB1's last detents)"""
    emu_k, host_k, at, row = [], [], t, 0
    for ch in range(8):
        for want in (0, 1, 2, 5):                                 # REVERB, DELAY, CHORUS, DRIVE
            if want != row:
                emu_k.append(f"{at}:e{ENC_SELECT}:{want - row}")
                host_k.append(f"{at} enc 0 {want - row}")
                at += 8 * abs(want - row) + 40
                row = want
            emu_k.append(f"{at}:e{ENC_KNOB1}:127")
            host_k.append(f"{at} enc 2 127")         # (127: host mode takes a turn as a signed byte)
            at += 8 * 127 + 40
        emu_k.append(f"{at}:e{ENC_SELECT}:{-row}")
        host_k.append(f"{at} enc 0 {-row}")
        at += 8 * row + 40
        emu_k.append(f"{at}:e{ENC_KNOB2}:1")
        host_k.append(f"{at} enc 3 1")
        row, at = 0, at + 50
    return emu_k, host_k, at


@emu_only
def test_18_9_strict_timing_densest_piece_every_effect(tmp_path):
    """the densest piece with REVERB, DELAY, CHORUS and DRIVE at 127 on all 8 channels: no empty audio half; the FX
    screen at the end is host mode's, pixel for pixel (every value really set)"""
    d = dense()
    usb, t = usb_script([d], gap=60)
    emu_k, _, at = fx_all_up(t + 600)
    keys = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^", f"{t + 400}:FX", f"{t + 450}:FX^"] + emu_k
    report, lcd, s = emu(tmp_path, at + 10500, usb=usb, keys=",".join(keys), strict=True, wav=True)
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert rms(s, len(s) // 2 - 5 * 44100) > 300, "the densest piece, every effect on, heard"
    lines, h = [], 100
    for f in frames(d):
        lines.append(f"{h} sysex {f.hex()}")
        h += 2
    _, host_k, h2 = fx_all_up(h + 600)
    lines += [f"{h + 200} btn 12", f"{h + 400} btn 2"] + host_k + [f"{h2 + 500} shot", f"{h2 + 600} end"]
    p = tmp_path / "fxref.txt"
    p.write_text("\n".join(lines) + "\n")
    log, _ = run_host(p)
    last = {int(m[1]): m[2] for m in re.finditer(r"SCREEN (40|68|96|180) \d+ [0-9A-F]{4} (\d+)$", log, re.M)}
    assert last == {40: "127", 68: "127", 96: "127", 180: "127"}, f"host: REVERB, DELAY, CHORUS, DRIVE at 127: {last}"
    assert lcd == rgb565(APP / f"build/host/fxref-{h2 + 500}.ppm"), "the emulator's FX screen: every effect at 127"


def dense_sections():
    """dense() with a section on every bar (as many as fit beside the notes, at most 24)"""
    from fm1c.piece import Section, decode
    p = decode(dense())
    while True:
        bars = p.len_ticks // 384
        p.sections = [Section(b * 384, f"BAR{b + 1}") for b in range(min(24, bars))]
        try:
            return encode(p), len(p.sections)
        except PieceError:                                   # (the table needs room: one bar less)
            p.len_ticks -= 384
            for c in p.channels:
                c.notes = [n for n in c.notes if n.tick < p.len_ticks]



from test_sections import test_23_6_strict_timing_densest_piece_a_section_jump_every_bar  # noqa: E402,F401  (run here by tests/emu_e2e.sh)
from test_live import test_24_6_strict_timing_densest_piece_and_4_live_notes  # noqa: E402,F401  (run here by tests/emu_e2e.sh)
from test_steps import test_25_6_strict_timing_8_tracks_64_steps_chords_swing_60  # noqa: E402,F401  (run here by tests/emu_e2e.sh)
