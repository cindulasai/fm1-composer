from pathlib import Path
"""US-34: live send. A piece sent while one plays takes over at the next bar line, keeping the sounds, mutes, solos and
effects changed on the device; it is saved once playback stops. Host mode: pieces pushed over SysEx (the real transfer
path), keys from a script, asserted on the screen text, the SysEx replies and the sound."""
import re, struct, sys
import pytest
from conftest import APP, run_host, wav_frames
from test_ui import piece, push, script, at, rms, BTN, ENC
from test_fx import fx_page
from test_studio import browser, studio, studio_url  # noqa: F401  (fixtures, for 34.5)

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, Channel, Note, encode  # noqa: E402

BTN = {**BTN, "FX": 2}
ENC = {**ENC, "KNOB3": 4}
FOOT, BAR_MS = 222, 2000                       # the footer's row; a bar at 120 BPM


def beta(name="beta", bars=4):
    """3 channels: PAD (alpha's channel 1 is LEAD), BASS, DRUMS; 100 BPM"""
    chans = [Channel(r, [Note(t * 96, (36 if r == "drums" else 50 + c * 5) + t % 2, 90, 100) for t in range(bars * 4)])
             for c, r in enumerate(["pad", "bass", "drums"])]
    return encode(Piece(name, 1000, bars * 384, chans))


def replies(log, cmd):
    """the ms of each SysEx reply with this command"""
    return [int(m[1]) for m in re.finditer(rf"^ *(\d+) ms  SYSEX out F07D4631{cmd:02X}", log, re.M)]


def foot(log, ms0, ms1=10 ** 9):
    return [(ms, txt) for ms, y, txt in __import__("conftest").screen(log) if y == FOOT and ms0 <= ms <= ms1]


def strip(log, ms0, ms1, ch):
    """the texts drawn on channel ch's strip (0-based) between ms0 and ms1"""
    return [txt for y, txt in at(log, ms0, ms1) if y == 64 + ch * 19]


def sound_of(log, ms0, ms1, ch, role):
    names = [t for t in strip(log, ms0, ms1, ch) if t and t not in (role, f" {ch + 1}", f">{ch + 1}", "M", "S")]
    return names[-1] if names else None


def session(tmp_path, name, extra, end, first=None, env=None):
    """alpha (8 channels, 120 BPM, 8 bars) pushed and playing from t + 50; then the extra lines (ms after t)"""
    lines, t = push(100, first or piece("alpha"))
    lines += [f"{t + 50} btn {BTN['PLAY']}"]
    out = []
    for ms, what in extra:
        if isinstance(what, bytes):                    # a piece pushed from ms
            l, _ = push(t + ms, what)
            out += l
        else:
            out.append(f"{t + ms} {what}")
    return run_host(script(tmp_path, name, lines + out, t + end), env=env), t


def test_34_3_switch_at_the_next_bar_footer_next_then_live_saved_once_stopped(tmp_path):
    (log, wav), t = session(tmp_path, "live1", [(600, beta()), (3000, f"btn {BTN['PLAY']}"), (3300, f"btn {BTN['HOME']}")], 3800)
    staged = [ms for ms in replies(log, 0x4F) if ms > t]
    assert len(staged) == 1 and t + 600 < staged[0] < t + 700, "staged while playing"
    play0 = t + 51                                     # (the press is read on the next pass)
    nxt = [ms for ms, txt in foot(log, t + 600) if txt == "NEXT: beta"]
    live = [ms for ms, txt in foot(log, t + 600) if txt == "LIVE: beta"]
    assert nxt and staged[0] <= nxt[0] <= staged[0] + 10, f"NEXT: beta as it arrives ({nxt})"
    assert live and play0 + BAR_MS - 5 <= live[0] <= play0 + BAR_MS + 15, f"LIVE: beta at alpha's bar 2 ({live}, {play0 + BAR_MS})"
    assert nxt[0] < live[0]
    assert (4, "beta") in at(log, live[0], live[0] + 50), "the header names it"
    assert not strip(log, live[0], t + 2990, 3) and strip(log, live[0], t + 2990, 2), "3 strips now"
    assert not [ms for ms in replies(log, 0x45) if t < ms < t + 3000], "never saved while playing"
    saved = [ms for ms in replies(log, 0x45) if ms >= t + 3000]
    assert len(saved) == 1 and saved[0] < t + 3100, "saved once stopped, told unasked"
    assert (FOOT, "SAVED") in at(log, t + 3000, t + 3290)
    lib = at(log, t + 3300)
    assert (4, "2 PIECES") in lib and (28, "> beta") in lib and (48, "  alpha") in lib, "newest first"
    f = wav_frames(wav)
    sw = (play0 + BAR_MS) / 1000
    assert rms(f, sw - 0.2, sw) > 300 and rms(f, sw, sw + 0.2) > 300, "sound on both sides of the switch"


def test_34_2_device_changes_carry_over_others_take_the_new_pieces_own(tmp_path):
    keys = [(200, f"enc {ENC['KNOB1']} 1"), (250, f"enc {ENC['PRESETS']} 5"),           # channel 2's sound
            (300, "keys 0x10"), (350, "keys 0x0"),                                       # mute channel 3
            (400, f"btndown {BTN['SEL']}"), (420, "keys 0x80"), (440, "keys 0x0"), (460, f"btnup {BTN['SEL']}"),   # solo 5
            (500, f"enc {ENC['KNOB1']} -1"), (520, f"btn {BTN['FX']}"), (550, f"enc {ENC['KNOB1']} 20"),
            (580, f"btn {BTN['HOME']}"),                                                 # channel 1's reverb +20
            (700, beta()), (2600, f"btn {BTN['FX']}"), (2800, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live2", keys, 3000)
    chosen = sound_of(log, t + 250, t + 299, 1, "BASS")
    assert chosen and chosen != "SQR BASS"
    rev = fx_page(log, t + 550, t + 579)[2]["REVERB"]
    live = min(ms for ms, txt in foot(log, t + 700) if txt == "LIVE: beta")
    (ref, _), r = session(tmp_path, "live2ref", [], 400, first=beta())
    own = sound_of(ref, r + 50, r + 390, 0, "PAD")
    assert own, "beta's own channel 1 sound"
    after = (live, t + 2590)
    assert sound_of(log, *after, 0, "PAD") == own, "channel 1's sound unchanged here: beta's own"
    assert sound_of(log, *after, 1, "BASS") == chosen, "channel 2's chosen sound carries over (melodic to melodic)"
    assert "M" in strip(log, *after, 2), "channel 3's mute carries over"
    assert "S" not in [x for c in range(3) for x in strip(log, *after, c)], "the solo on channel 5 (beta has none) is dropped"
    head, _, v = fx_page(log, t + 2600, t + 2790)
    assert head == "FX 1 PAD" and v["REVERB"] == rev, f"channel 1's changed reverb carries over ({v}, {rev})"


def test_34_4_newer_wins_corrupt_refused_stopped_saved_as_today(tmp_path):
    bad = bytearray(beta("bad"))
    bad[-1] ^= 0xFF                                    # its CRC
    extra = [(400, beta()), (900, beta("gamma")), (2400, bytes(bad)), (5000, f"btn {BTN['PLAY']}"),
             (5300, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live3", extra, 5800)
    texts = [txt for ms, txt in foot(log, t + 400)]
    assert "NEXT: beta" in texts and "NEXT: gamma" in texts and "LIVE: beta" not in texts, texts
    lives = [ms for ms, txt in foot(log, t + 400) if txt.startswith("LIVE: ")]
    assert [txt for ms, txt in foot(log, t + 400) if txt.startswith("LIVE: ")] == ["LIVE: gamma"], "one switch, the newer"
    assert t + 51 + BAR_MS - 5 <= lives[0] <= t + 51 + BAR_MS + 15
    naks = [m for m in re.finditer(r"^ *(\d+) ms  SYSEX out F07D46317F", log, re.M) if int(m[1]) > t + 2400]
    assert naks, "the corrupt piece refused"
    assert [txt for ms, txt in foot(log, t + 2100, t + 4990) if txt.startswith(("LIVE", "NEXT"))] == [], \
        "the playing piece untouched by it"
    lib = at(log, t + 5300)
    assert (4, "2 PIECES") in lib and (28, "> gamma") in lib and all("beta" not in x for _, x in lib)
    lines, t2 = push(100, piece("alpha"))              # sent while stopped: saved at once, top of LIBRARY (as before)
    l2, t3 = push(t2 + 100, beta())
    log2, _ = run_host(script(tmp_path, "live3b", lines + l2, t3 + 300))
    assert replies(log2, 0x45) and not replies(log2, 0x4F) and (28, "> beta") in at(log2, t2)


BETA_BAR_MS = 2400                                 # a bar of beta (100 BPM)


def test_34_4_chained_live_sends_switch_at_the_next_bar_replaced_live_piece_not_saved(tmp_path):
    """alpha plays, beta goes LIVE (unsaved, unedited), gamma sent: gamma takes over at beta's next bar (only device
    edits hold a switch: decision 55); stopped, gamma (newest) is saved and beta, replaced before it was saved, is not"""
    extra = [(600, beta()), (2600, beta("gamma")), (7000, f"btn {BTN['PLAY']}"), (7300, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live9", extra, 7800)
    texts = [txt for ms, txt in foot(log, t + 600, t + 6990) if txt.startswith(("LIVE", "NEXT"))]
    assert texts == ["NEXT: beta", "LIVE: beta", "NEXT: gamma", "LIVE: gamma"], texts
    b_live = min(ms for ms, txt in foot(log, t + 600) if txt == "LIVE: beta")
    g_live = min(ms for ms, txt in foot(log, t + 600) if txt == "LIVE: gamma")
    assert b_live + BETA_BAR_MS - 15 <= g_live <= b_live + BETA_BAR_MS + 15, (b_live, g_live)
    assert (4, "gamma") in at(log, g_live, g_live + 50), "the header names it"
    assert not [ms for ms in replies(log, 0x45) if t < ms < t + 7000], "never saved while playing"
    assert len([ms for ms in replies(log, 0x45) if ms >= t + 7000]) == 1, "one save once stopped: gamma"
    lib = at(log, t + 7300)
    assert (4, "2 PIECES") in lib and (28, "> gamma") in lib and (48, "  alpha") in lib, lib
    assert all("beta" not in x for _, x in lib), "beta was replaced before it was saved"


def test_34_4_a_live_piece_edited_on_the_device_holds_the_next_until_stop(tmp_path):
    """beta goes LIVE, then a device edit on it (its voice, KNOB3), then gamma: NEXT AT STOP: gamma (34.7 holds for a
    piece that itself went live); stopped, beta with its edit is saved, then gamma, newest at the top"""
    extra = [(600, beta()), (2300, f"enc {ENC['KNOB3']} 1"), (2600, beta("gamma")), (7000, f"btn {BTN['PLAY']}"),
             (7300, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live10", extra, 7800)
    assert [txt for ms, txt in foot(log, t + 2300, t + 2390) if txt.startswith("VOICE 1 ")], "beta's voice edited"
    texts = [txt for ms, txt in foot(log, t + 2400, t + 6990)]
    assert "NEXT AT STOP: gamma" in texts and "LIVE: gamma" not in texts and "NEXT: gamma" not in texts, texts
    lib = at(log, t + 7300)
    assert (4, "3 PIECES") in lib and (28, "> gamma") in lib and (48, "  beta") in lib, lib


def test_34_4_library_full_plays_live_then_says_so_and_keeps_it(tmp_path):
    small = lambda n: encode(Piece(n, 1200, 384, [Channel("lead", [Note(0, 60, 96, 100)])]))
    lines, t = [], 100
    for k in range(73):                                # every library sector used
        l, t = push(t, small(f"p{k:02}"))
        lines += l
    lines += [f"{t + 50} btn {BTN['PLAY']}"]
    l, _ = push(t + 400, beta())
    lines += l + [f"{t + 3000} btn {BTN['PLAY']}", f"{t + 3300} btn {BTN['HOME']}"]
    log, _ = run_host(script(tmp_path, "live4", lines, t + 3800))
    assert len(replies(log, 0x45)) == 73
    assert [txt for ms, txt in foot(log, t + 400) if txt.startswith("LIVE")] == ["LIVE: beta"], "plays live"
    assert (FOOT, "LIBRARY FULL") in at(log, t + 3000, t + 3290)
    full = re.findall(r"^ *(\d+) ms  SYSEX out F07D46317F", log, re.M)
    assert [int(ms) for ms in full if int(ms) >= t + 3000], "the sender is told: full"
    assert (FOOT, "UNSAVED EDITS: HOME AGAIN TO DISCARD") in at(log, t + 3300), "kept, playing-only (not in LIBRARY)"
    assert (4, "beta") in at(log, t + 2000, t + 2990)


# ---- decision 53: unsaved edits are never dropped by a live switch ----
def test_34_7_unsaved_edits_hold_the_switch_until_stop_old_edits_intact(tmp_path):
    """an edit on the playing piece (its voice, KNOB3) and SAVE pressed while playing, then a piece sent: no switch at
    the bar (footer NEXT AT STOP: beta); stopped, alpha's edit is saved, then beta, newest at the top; alpha re-read
    from the library still has the edit"""
    flash = str(tmp_path / "flash.bin")
    extra = [(300, f"enc {ENC['KNOB3']} 1"), (400, f"btn {BTN['SAVE']}"), (600, beta()),
             (4500, f"btn {BTN['PLAY']}"), (4800, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live6", extra, 5000, env={"HOST_FLASH": flash})
    turned = [txt for ms, txt in foot(log, t + 300, t + 390) if txt.startswith("VOICE 1 ")]
    assert turned, "the voice edited"
    texts = [txt for ms, txt in foot(log, t + 600, t + 4490)]
    assert "NEXT AT STOP: beta" in texts and not [x for x in texts if x.startswith(("LIVE", "NEXT: "))], texts
    assert (4, "beta") not in at(log, t + 600, t + 4490), "alpha plays on past every bar line"
    lib = at(log, t + 4800)
    assert (4, "2 PIECES") in lib and (28, "> beta") in lib and (48, "  alpha") in lib, "both kept, beta newest"
    log2, _ = run_host(script(tmp_path, "live6b", [f"100 enc {ENC['SELECT']} 1", f"200 btn {BTN['PLAY']}",
                                                     f"500 enc {ENC['KNOB3']} 1"], 900), env={"HOST_FLASH": flash})
    assert (4, "alpha") in at(log2, 200, 490)
    again = [txt for ms, txt in foot(log2, 500, 890) if txt.startswith("VOICE 1 ")]
    assert again and again[-1] != turned[-1], f"alpha's saved edit re-read: {turned[-1]} then one more turn: {again}"


def test_34_7_unsaved_edits_without_save_stay_open_after_stop(tmp_path):
    """an edit, no SAVE: no switch; stopped, beta is saved at the top and alpha stays open with its edit unsaved
    (HOME still asks)"""
    extra = [(300, f"enc {ENC['KNOB3']} 1"), (600, beta()), (4500, f"btn {BTN['PLAY']}"), (4800, f"btn {BTN['HOME']}")]
    (log, _), t = session(tmp_path, "live7", extra, 5000)
    assert "NEXT AT STOP: beta" in [txt for ms, txt in foot(log, t + 600, t + 4490)]
    assert not [txt for ms, txt in foot(log, t + 600) if txt.startswith("LIVE")]
    assert [ms for ms in replies(log, 0x45) if t + 4500 <= ms < t + 4600], "beta saved once stopped"
    assert (FOOT, "UNSAVED EDITS: HOME AGAIN TO DISCARD") in at(log, t + 4800), "alpha's edit still there"


def test_34_7_without_unsaved_edits_it_still_switches_at_the_bar(tmp_path):
    """a sound changed (a choice, not an edit of the piece) and nothing waiting to save: the switch comes at the bar"""
    (log, _), t = session(tmp_path, "live8", [(300, f"enc {ENC['PRESETS']} 3"), (600, beta())], 2600)
    live = [ms for ms, txt in foot(log, t + 600) if txt == "LIVE: beta"]
    assert live and t + 51 + BAR_MS - 5 <= live[0] <= t + 51 + BAR_MS + 15, live
    assert "NEXT AT STOP: beta" not in [txt for ms, txt in foot(log, t + 600)]


# ---- the footer as pixels: Studio reads it off fm1-emu's screen (34.5) ----
def footer_px(text):
    """the footer's RGB565 pixels (240 x 16) as gfx.c draws text there, from the firmware's own font"""
    sys.path.insert(0, str(APP / "tools"))
    import base64, gen_studio_font
    f = gen_studio_font.arrays(gen_studio_font.SRC.read_text())
    data, out = base64.b64decode(f["data"]), [0] * (240 * 16)
    g = (120 >> 3) << 11 | (120 >> 2) << 5 | (120 >> 3)
    ramp = [((g >> 11) * a // 15) << 11 | (((g >> 5) & 63) * a // 15) << 5 | ((g & 31) * a // 15) for a in range(16)]
    ks = [(ord(c) if f["first"] <= ord(c) <= f["last"] else 63) - f["first"] for c in text]
    x = int((240 - sum(f["adv"][k] for k in ks)) / 2)
    for k in ks:
        bw = f["bw"][k]
        for y in range(16):
            for gx in range(bw):
                b = data[f["off"][k] + y * ((bw + 1) // 2) + gx // 2]
                v, px = (b & 15) if gx & 1 else b >> 4, x - f["pad"] + gx
                if v and 0 <= px < 240:
                    out[y * 240 + px] = ramp[v]
        x += f["adv"][k]
    return out


def lcd16(rgb):
    """test_emu.rgb565's bytes (5, 6, 5 bits a channel) as RGB565 words"""
    return [rgb[i] << 11 | rgb[i + 1] << 5 | rgb[i + 2] for i in range(0, len(rgb), 3)]


def test_34_5_studio_font_is_the_firmwares():
    sys.path.insert(0, str(APP / "tools"))
    import gen_studio_font
    assert (APP / "studio/web/font.js").read_text() == gen_studio_font.render(gen_studio_font.arrays(gen_studio_font.SRC.read_text())), \
        "studio/web/font.js is stale: python3 tools/gen_studio_font.py"


def test_34_5_studio_reads_the_footer_off_the_screen_host_drawn(tmp_path, pw_browser):
    """Studio's reader (fm1link.js footerSays) on the screen host mode draws after a live switch: LIVE: <name> yes,
    NEXT: <name> / another name no; and its name is the piece's as the FM-1 shows it"""
    from test_emu import rgb565
    (log, _), t = session(tmp_path, "live5", [(600, beta("Beta Dub 7")), (2500, "shot")], 2600)
    px = lcd16(rgb565(APP / f"build/host/live5-{t + 2500}.ppm"))
    assert px[222 * 240:238 * 240] == footer_px("LIVE: Beta Dub 7"), "the Python model of the footer is the firmware's"
    web = APP / "studio" / "web"
    page = pw_browser.new_page()
    def serve(route):
        name = route.request.url.split("/", 3)[3].split("?")[0]
        if name.endswith(".js") and (web / name).is_file():
            route.fulfill(path=str(web / name), content_type="text/javascript")
        else:
            route.fulfill(body="<!doctype html><title>t</title>", content_type="text/html")
    page.route("http://studio.test/**", serve)
    page.goto("http://studio.test/blank")
    got = page.evaluate("""async ([px, piece]) => { const m = await import('./fm1link.js'), lcd = Uint16Array.from(px);
        const name = m.screenName(Uint8Array.from(piece));
        return [name, m.footerSays(lcd, `LIVE: ${name}`), m.footerSays(lcd, `NEXT: ${name}`), m.footerSays(lcd, 'LIVE: Beta Dub 8'),
                m.footerSays(null, 'x'), Array.from(m.footerPixels('LIVE: Beta Dub 7'))]; }""", [px, list(beta("Beta Dub 7"))])
    assert got[:5] == ["Beta Dub 7", True, False, False, False] and got[5] == footer_px("LIVE: Beta Dub 7")
    page.close()


STUDIO_EMU = (APP / "build" / "composer.fwsc").exists() and (APP / "studio" / "web" / "emu" / "fm1_web.wasm").exists()


@pytest.mark.skipif(not STUDIO_EMU, reason="needs build/composer.fwsc (from CI) and studio/web/emu (sh tools/get_web_emu.sh)")
def test_34_5_studio_send_while_the_fm1_plays_next_bar_then_live(studio, tmp_path):
    """Studio: a piece on the FM-1 and playing, Send another: "Plays from the next bar", then "Live on the FM-1: <name>"
    read back from its screen (its footer, pixel for pixel as the firmware draws it); once stopped, it is in the flash"""
    import time
    from test_studio_emu import emu, open_example, start, send, diag
    page, _, _ = studio()
    open_example(page, "funk.classic suite 1")
    f = start(page)
    assert send(page).startswith("On the FM-1")
    f.click("#big")
    page.keyboard.down(" "); time.sleep(0.25); page.keyboard.up(" ")              # PLAY: it plays suite 1
    open_example(page, "funk.classic suite 2")
    page.evaluate("""() => { window.__said = []; new MutationObserver(() => window.__said.push(document.querySelector('#status').textContent))
        .observe(document.querySelector('#status'), { childList: true, characterData: true, subtree: true }); }""")
    page.click("#send")
    page.wait_for_function("() => /^(Live on the FM-1|On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)",
                           timeout=120000)
    said = page.evaluate("() => window.__said")
    assert page.text_content("#status").startswith('Live on the FM-1: "funk.classic suite 2"'), diag(page)
    assert any(s.startswith("Plays from the next bar") for s in said), said
    ok = emu(page).evaluate("""async () => { const m = await import('../fm1link.js'); const lcd = window.composerCable.screen();
        return m.footerSays(lcd, 'LIVE: funk.classic suite 2'); }""")
    assert ok, "the FM-1's footer: LIVE: <name>"
    page.keyboard.down(" "); time.sleep(0.25); page.keyboard.up(" ")              # stopped: saved to its flash
    page.wait_for_function("""async () => { const m = await import('./fm1link.js');
        return (await m.savedLibraries()).some((l) => l.length && l[0].name === 'funk.classic suite 2'); }""", timeout=30000)


@pytest.mark.skipif(__import__("os").environ.get("FM1_EMU_RUN") != "1", reason="run by tests/emu_e2e.sh (test_emu.py)")
def test_34_6_strict_timing_live_switch_on_the_densest_piece(tmp_path):
    """the densest piece playing, the densest again (another name) sent over USB while it plays: under on-chip timing
    it takes over at the bar line (footer LIVE: <name>) with no empty audio half"""
    from fm1c.piece import decode
    from test_emu import dense, emu, replies, rms as erms, usb_script
    d = dense()
    p2 = decode(d)
    p2.name = "dense two"
    d2 = encode(p2)
    usb, t = usb_script([d], gap=60)
    keys = [f"{t + 200}:PLAY", f"{t + 250}:PLAY^"]                                   # open it: it plays
    usb2, t2 = usb_script([d2], t=t + 1000, gap=60)
    end = t2 + 4000                                                                  # (a bar at 140 BPM: 1.7 s)
    report, lcd, s = emu(tmp_path, end, usb=usb + ";" + usb2, keys=",".join(keys), strict=True, wav=True)
    assert 0x4F in replies(report), "staged while playing"
    m = re.search(r"usb audio: .*\((\d+) empty, (\d+) of them underruns\)", report)
    assert m and m[1] == "0" and m[2] == "0", report[-800:]
    assert lcd16(lcd)[222 * 240:238 * 240] == footer_px("LIVE: dense two"), "switched at the bar line"
    assert erms(s, len(s) // 2 - 2 * 44100) > 300, "the new piece heard"


def test_34_readme_documents_live_send_as_the_screen_says():
    """app/README.md explains Send while playing with the footers the firmware shows (lv_status: "<what>: <name>")"""
    here = Path(__file__).resolve().parents[2]
    readme, src = (here / "README.md").read_text(), (here / "firmware/src/composer.c").read_text()
    assert "Send while it plays" in readme and "next bar line" in readme
    for what in ("NEXT", "LIVE", "NEXT AT STOP"):
        assert f'lv_status("{what}"' in src, what
        assert f"`{what}: <name>`" in readme, what


def test_34_8_readme_and_fm1c_push_lines_say_next_bar_saved_at_stop_and_replaced_as_the_firmware_does():
    """fm1c's push lines and app/README.md's push and Studio paragraphs: a piece pushed while playing takes over at the
    next bar (the footers the firmware draws), is saved when playback stops, and what replaced means (decision 55)"""
    here = Path(__file__).resolve().parents[2]
    readme, src = (here / "README.md").read_text(), (here / "firmware/src/composer.c").read_text()
    cli, shell = (here / "tools/fm1c/cli.py").read_text(), (here / "studio/web/shell.js").read_text()
    push = readme[readme.index("If you push while a piece is playing"):readme.index("## Limits")]
    flat = " ".join(push.split())
    for what in ("NEXT", "LIVE", "NEXT AT STOP"):
        assert f'lv_status("{what}"' in src and f"`{what}: <name>`" in push, what
    assert "takes over at the next bar" in flat and "saved to the library when playback stops" in flat, flat
    assert "`<a>: replaced by <b> before it was saved, not in LIBRARY` and exits 1" in flat, flat
    assert 'f"{waiting}: replaced by {name} before it was saved, not in LIBRARY"' in cli
    assert 'staged — takes over at the next bar (NEXT: {name}, then LIVE: {name}), "' in cli and \
        '"saved when playback stops"' in cli
    studio = " ".join(readme[readme.index("**Send while it plays**"):readme.index("Both apps run unchanged")].split())
    assert "replaces the live one at the next bar" in studio and "`(replaces <name>, not saved yet)`" in studio, studio
    assert "(replaces ${" in shell and ", not saved yet)" in shell, "Studio's status says it"


# ---- Studio's Send while playing, against a scripted FM-1 screen (34.4, 34.9) ----
# fm1-emu's page replaced by a stand-in with the cable's interface (cable.js): its screen shows the footers a script
# gives, each drawn as the firmware draws it (fm1link.js footerPixels), timed from Send's END frame
FAKE_EMU = """<!doctype html><meta charset="utf-8"><title>fake fm1-emu</title><script type="module">
import { footerPixels } from '../fm1link.js';
let screen = null, script = [];
const watchers = new Set();
window.__shown = [];
window.__show = (text) => {
  const l = new Uint16Array(240 * 240); l.set(footerPixels(text), 222 * 240); screen = l;
  window.__shown.push([Date.now(), text]);
  for (const w of watchers) w(screen);
};
window.__script = (s) => { script = s; };
window.composerCable = { name: 'COMPOSER Studio', connected: () => true, load: () => 1, emulatedMs: () => NaN,
  screen: () => screen, onScreen(fn) { watchers.add(fn); return () => watchers.delete(fn); },
  send(b) { if (b[4] === 5) for (const [ms, text] of script) setTimeout(() => window.__show(text), ms); } };
</script>"""


def fake_studio(studio):
    import re as _re
    page, _, _ = studio(routes={_re.compile(r"/emu/\?embed=1$"): lambda r: r.fulfill(body=FAKE_EMU, content_type="text/html")})
    page.evaluate("""() => { window.__said = []; new MutationObserver(() => window.__said.push([Date.now(),
        document.querySelector('#status').textContent])).observe(document.querySelector('#status'),
        { childList: true, characterData: true, subtree: true }); }""")
    return page


def fake_send(page, title, script):
    """open the example, script the FM-1's footers ($ = the piece's name as the FM-1 shows it), Send; the final status"""
    from test_studio_emu import open_example, t2m, emu
    open_example(page, title)
    f1p = bytes(t2m(page).evaluate("() => Array.from(window.studioDesk.fitted().f1p)"))
    name = "".join(chr(c) if 32 <= c <= 126 else "?" for c in f1p[16:40].split(b"\0")[0])
    emu(page).evaluate("s => window.__script(s)", [[ms, txt.replace("$", name)] for ms, txt in script])
    page.click("#send")
    page.wait_for_function("() => /^(Live on the FM-1|On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)",
                           timeout=60000)
    return page.text_content("#status"), name


def test_34_4_studio_says_which_unsaved_live_piece_a_send_replaces(studio):
    """suite 1 goes LIVE; suite 2 sent while the footer shows LIVE: suite 1: its final status adds
    (replaces funk.classic suite 1, not saved yet)"""
    page = fake_studio(studio)
    s1, n1 = fake_send(page, "funk.classic suite 1", [(100, "NEXT: $"), (600, "LIVE: $")])
    assert s1.startswith('Live on the FM-1: "funk.classic suite 1" (read back'), s1
    s2, _ = fake_send(page, "funk.classic suite 2", [(100, "NEXT: $"), (600, "LIVE: $")])
    assert s2.startswith(f'Live on the FM-1: "funk.classic suite 2" (replaces {n1}, not saved yet) (read back'), s2


def test_34_9_studio_resending_the_live_name_waits_for_the_new_switch(studio):
    """suite 1 LIVE, then suite 1 sent again: the LIVE: suite 1 still up from before is not the new switch; the status
    says Live only once the FM-1 draws LIVE: <it> again (after NEXT: <it>)"""
    from test_studio_emu import emu
    page = fake_studio(studio)
    fake_send(page, "funk.classic suite 1", [(100, "NEXT: $"), (600, "LIVE: $")])
    s, n = fake_send(page, "funk.classic suite 1", [(1500, "NEXT: $"), (2500, "LIVE: $")])
    __import__("time").sleep(3)                      # (the script played out: its last footer is the new switch)
    switched = emu(page).evaluate("() => window.__shown")[-1]
    assert switched[1] == f"LIVE: {n}"
    said = [ms for ms, txt in page.evaluate("() => window.__said") if txt.startswith("Live on the FM-1")]
    assert said and said[-1] >= switched[0], ("Live said before the new switch", said, switched, s)
    assert s.startswith(f'Live on the FM-1: "funk.classic suite 1" (replaces {n}, not saved yet)'), s


def test_34_9_studio_sees_a_switch_whose_footer_is_overwritten_at_once(studio):
    """NEXT: <it>, a knob turned (VOICE 1 ...), LIVE: <it> at the switch and a knob turned again 5 ms later: the
    switch was read back from the screens, so the status still ends Live on the FM-1"""
    page = fake_studio(studio)
    s, _ = fake_send(page, "funk.classic suite 1",
                     [(100, "NEXT: $"), (400, "VOICE 1 SAW"), (900, "LIVE: $"), (905, "VOICE 1 SQR")])
    assert s.startswith('Live on the FM-1: "funk.classic suite 1"'), s


@pytest.mark.skipif(not STUDIO_EMU, reason="needs build/composer.fwsc (from CI) and studio/web/emu (sh tools/get_web_emu.sh)")
def test_34_4_studio_chained_live_sends_on_the_real_fm1(studio):
    """fm1-emu running COMPOSER: suite 1 plays, suite 2 goes LIVE, suite 1 sent again before any stop: it too switches
    at the next bar (no NEXT AT STOP: nothing was edited on the FM-1) and says it replaces suite 2, not saved yet;
    once stopped, suite 2 is not in the flash (decision 55)"""
    import time
    from test_studio_emu import open_example, start, send, diag
    page, _, _ = studio()
    open_example(page, "funk.classic suite 1")
    f = start(page)
    assert send(page).startswith("On the FM-1")
    f.click("#big")
    page.keyboard.down(" "); time.sleep(0.25); page.keyboard.up(" ")              # PLAY: suite 1 plays
    done = "() => /^(Live on the FM-1|On the FM-1|Plays when|Sending stopped)/.test(document.querySelector('#status').textContent)"
    open_example(page, "funk.classic suite 2")
    page.click("#send")
    page.wait_for_function(done, timeout=120000)
    assert page.text_content("#status").startswith('Live on the FM-1: "funk.classic suite 2"'), diag(page)
    open_example(page, "funk.classic suite 1")
    page.click("#send")
    page.wait_for_function(done, timeout=120000)
    st = page.text_content("#status")
    assert st.startswith('Live on the FM-1: "funk.classic suite 1" (replaces funk.classic suite 2, not saved yet)'), diag(page)
    page.keyboard.down(" "); time.sleep(0.25); page.keyboard.up(" ")              # stopped: the live one is saved
    page.wait_for_function("""async () => { const m = await import('./fm1link.js'), ls = await m.savedLibraries();
        return ls.some((l) => l.length >= 2 && l[0].name === 'funk.classic suite 1'); }""", timeout=30000)
    names = page.evaluate("""async () => { const m = await import('./fm1link.js');
        return (await m.savedLibraries()).flat().map((p) => p.name); }""")
    assert "funk.classic suite 2" not in names, names


def test_34_5_studio_frames_a_piece_of_whole_256_byte_chunks_exactly(pw_browser):
    """fm1link.js transferFrames: a 512-byte piece is START, two 256-byte DATA frames (offsets 0, 256), END: no empty
    DATA frame past the end (a piece whose length is a multiple of 256)"""
    web = APP / "studio" / "web"
    page = pw_browser.new_page()
    def serve(route):
        name = route.request.url.split("/", 3)[3].split("?")[0]
        if name.endswith(".js") and (web / name).is_file():
            route.fulfill(path=str(web / name), content_type="text/javascript")
        else:
            route.fulfill(body="<!doctype html><title>t</title>", content_type="text/html")
    page.route("http://studio.test/**", serve)
    page.goto("http://studio.test/blank")
    n = page.evaluate("""async () => { const m = await import('./fm1link.js');
        return [512, 513, 255].map((len) => m.transferFrames(new Uint8Array(len)).map((f) => f.length)); }""")
    page.close()
    assert len(n[0]) == 4 and n[0][1] == n[0][2], n[0]                      # START, DATA 0, DATA 256, END
    assert len(n[1]) == 5 and len(n[2]) == 3, n                               # 513: one more byte, one more frame
