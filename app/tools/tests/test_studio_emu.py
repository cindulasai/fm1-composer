"""US-17: the right side of Studio is fm1-emu's own page (unchanged, pinned) running the CI-built COMPOSER, joined to
text2midi's window by Studio's MIDI cable. The FM-1's screen (fm1-emu's Big screen, 240x240) is compared pixel for
pixel with what host mode draws after the same pushes."""
import hashlib, json, re, subprocess, time, urllib.request
from pathlib import Path
import pytest
from conftest import APP
from test_emu import host_ref
from test_studio import body, brief_text, browser, studio, studio_url  # noqa: F401  (fixtures)

FWSC = APP / "build" / "composer.fwsc"
EMU = APP / "studio" / "web" / "emu"
PAGE_FILES = ["index.html", "style.css", "main.js", "persistence.js", "tryout.js", "worklet.js", "jit.js", "fwsc.js",
              "firmwares.json"]
# a test-only tap on fm1-emu's audio: the peak of what its worklet plays (fm1-emu's code is not changed)
AUDIO_TAP = """(() => { const c = AudioNode.prototype.connect;
  AudioNode.prototype.connect = function (dest, ...r) {
    if (window.AudioWorkletNode && this instanceof AudioWorkletNode && !this.__tap) {
      this.__tap = true; const a = this.context.createAnalyser(); a.fftSize = 2048; c.call(this, a);
      const buf = new Float32Array(2048);
      window.__fm1Peak = () => { a.getFloatTimeDomainData(buf); return buf.reduce((m, x) => Math.max(m, Math.abs(x)), 0); };
    }
    return c.call(this, dest, ...r); }; })();"""

pytestmark = pytest.mark.skipif(not (FWSC.exists() and (EMU / "fm1_web.wasm").exists()),
                                reason="needs build/composer.fwsc (from CI) and studio/web/emu (sh tools/get_web_emu.sh)")


def emu(page):
    return next(f for f in page.frames if "/emu/" in f.url)


def t2m(page):
    return next(f for f in page.frames if f.url.rstrip("/").endswith("/t2m"))


def lcd(f):
    """fm1-emu's Big screen canvas, back to 5-6-5 bits per pixel (as test_emu.rgb565 reads host mode's PPM)"""
    return f.evaluate("""() => { const c = document.querySelector('#big');
        if (!c || c.hidden) return null;
        const d = c.getContext('2d').getImageData(0, 0, 240, 240).data, out = new Array(240 * 240 * 3);
        for (let i = 0, j = 0; i < d.length; i += 4) { out[j++] = d[i] >> 3; out[j++] = d[i + 1] >> 2; out[j++] = d[i + 2] >> 3; }
        return out; }""")


def wait_lcd(f, ref, secs=20):
    end = time.time() + secs
    while time.time() < end:
        px = lcd(f)
        if px and bytes(px) == ref:
            return True
        time.sleep(0.3)
    return False


def start(page, f=None):
    f = f or emu(page)
    f.click("#start")
    f.wait_for_function("() => /^COMPOSER · /.test(document.querySelector('#status').textContent)", timeout=30000)
    f.click("#bigscreen")
    return f


def open_example(page, title="funk.classic suite 1"):
    f = t2m(page)
    f.click(f"text={title}")
    f.locator("h2", has_text=title).wait_for()
    return f


def diag(page):
    """what a failed Send leaves to look at (CI): both statuses (the emulator's has its load), its saved flash"""
    try:
        libs = page.evaluate("""async () => { const m = await import('./fm1link.js');
            return (await m.savedLibraries()).map((l) => l.map((p) => [p.seq, p.name, p.bytes.length])); }""")
    except Exception as e:                       # (diagnostics never hide the failure itself)
        libs = f"unreadable: {e}"
    try:
        log = page.evaluate("() => window.studioSendLog || null")
    except Exception as e:
        log = f"unreadable: {e}"
    return (f"studio: {page.text_content('#status')!r}; emulator: {emu(page).text_content('#status')!r}; saved: {libs}; "
            f"send: {log}")


def send(page, secs=90):
    page.click("#send")
    page.wait_for_function("() => /^(On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)",
                           timeout=secs * 1000)
    s = page.text_content("#status")
    if not s.startswith("On the FM-1"):
        print("SEND FAILED:", diag(page))
    return s


def test_17_1_fm1_emus_own_page_unchanged_boots_composer(studio, tmp_path, studio_url):
    build = (EMU / "BUILD.txt").read_text()
    for name in PAGE_FILES:
        assert re.search(rf"^{re.escape(name)} sha256 {hashlib.sha256((EMU / name).read_bytes()).hexdigest()}$", build, re.M), name
    served = urllib.request.urlopen(studio_url + "emu/?embed=1").read().decode()
    disk = (EMU / "index.html").read_text()
    assert served.replace('<script src="../cable.js"></script>\n', "", 1) == disk           # only the cable added
    for name in PAGE_FILES[1:-1]:
        assert urllib.request.urlopen(studio_url + "emu/" + name).read() == (EMU / name).read_bytes()
    cat = json.load(urllib.request.urlopen(studio_url + "emu/firmwares.json"))
    assert cat["default"] == "composer" and cat["firmwares"][0]["package"] == "../firmware/composer.fwsc"
    assert len(cat["firmwares"]) == len(json.loads((EMU / "firmwares.json").read_text())["firmwares"]) + 1
    page, _, _ = studio()
    f = emu(page)
    f.wait_for_function("() => document.querySelector('#midi').value === 'COMPOSER Studio'")
    start(page, f)
    assert wait_lcd(f, host_ref(tmp_path, "s17empty", []))                                  # LIBRARY EMPTY


def test_17_2_send_lands_in_library_read_back_then_plays(studio, tmp_path):
    page, _, _ = studio(init=AUDIO_TAP)
    open_example(page)
    f = start(page)
    status = send(page)
    assert status.startswith('On the FM-1: "funk.classic suite 1" (read back from its flash; 4 channels')
    piece = bytes(t2m(page).evaluate("() => Array.from(window.studioDesk.fitted().f1p)"))
    assert wait_lcd(f, host_ref(tmp_path, "s17one", [piece]))
    f.click("#big")                                                  # the keyboard goes to fm1-emu's page now
    page.keyboard.down(" ")                                          # its keyboard: Space = PLAY, held as a finger
    time.sleep(0.25)                                                 # does (COMPOSER reads held buttons)
    page.keyboard.up(" ")
    f.wait_for_function("() => window.__fm1Peak && window.__fm1Peak() > 0.01", timeout=15000)


def test_17_2_the_cable_is_a_sysex_midi_input_and_progress_climbs(studio):
    page, _, _ = studio()
    f = emu(page)
    acc = f.evaluate("""async () => { const a = await navigator.requestMIDIAccess({ sysex: true });
        return { sysex: a.sysexEnabled, names: [...a.inputs.values()].map((i) => i.name) }; }""")
    assert acc == {"sysex": True, "names": ["COMPOSER Studio"]} or (acc["sysex"] is True and "COMPOSER Studio" in acc["names"])
    open_example(page)
    start(page, f)
    page.click("#send")
    page.wait_for_function("() => { const b = document.querySelector('#send-progress'); return !b.hidden && b.value > 0.2 && b.value <= 1; }",
                           timeout=30000)
    page.wait_for_function("() => document.querySelector('#status').textContent.startsWith('On the FM-1')", timeout=90000)


def test_17_3_send_reports_what_changed_to_fit(studio):
    page, _, todo = studio()
    f = t2m(page)
    f.click("text=cinematic.orchestral suite 1")
    f.locator("h2", has_text="cinematic.orchestral suite 1").wait_for()
    made = f.evaluate("() => { const m = window.studioDesk.fitted(); return [m.report, m.channels] }")
    start(page)
    status = send(page)
    report = [ln for ln in made[0].split("\n") if ln]
    assert f"{made[1]} channels" in status
    if report:
        assert f"{len(report)} changes to fit" in status and page.is_visible("#report")
        assert page.locator("#report li").all_inner_texts() == report
    else:
        assert "nothing had to change" in status and page.is_hidden("#report")


def test_17_4_send_before_start_twice_and_without_a_piece(studio):
    page, _, _ = studio()
    assert page.is_disabled("#send")                                 # nothing open in text2midi yet
    open_example(page)
    page.wait_for_function("() => !document.querySelector('#send').disabled")
    page.click("#send")
    page.wait_for_function("() => document.querySelector('#status').textContent.includes('Press Start on the FM-1')")
    start(page)
    emu(page).evaluate("""() => { const c = window.composerCable, send = c.send.bind(c); window.__puts = 0;
        c.send = (b) => { if (b[0] === 0xf0 && b[4] === 3) window.__puts++; return send(b); }; }""")
    page.click("#send")
    page.click("#send", force=True)                                  # more presses while sending do nothing
    page.click("#send", force=True)
    page.wait_for_function("() => /^(On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)", timeout=90000)
    assert page.text_content("#status").startswith("On the FM-1"), diag(page)
    assert page.is_enabled("#send") and emu(page).evaluate("() => window.__puts") == 1           # one transfer (one PUT)


def test_17_4_firmware_missing_or_emulator_not_built(studio, studio_url):
    page, _, _ = studio(routes={"**/firmware/composer.fwsc": lambda r: r.fulfill(status=404, body="no")})
    f = emu(page)
    f.click("#start")
    f.wait_for_function("() => /composer\\.fwsc|404|did not/i.test(document.querySelector('#status').textContent)", timeout=20000)
    page2, _, _ = studio(routes={"**/emu/**": lambda r: r.fulfill(status=404, body="no")})
    open_example(page2)
    page2.click("#send")
    page2.wait_for_function("() => document.querySelector('#status').textContent.includes('has not loaded')")


def test_17_4_old_custom_pane_and_form_are_gone():
    web = APP / "studio" / "web"
    assert not (web / "emu.js").exists() and not (web / "studio.js").exists()
    assert "emu-box" not in (web / "index.html").read_text() and 'id="idea"' not in (web / "index.html").read_text()


def test_17_1_get_web_emu_pins_and_records_and_names_missing_tools(tmp_path):
    script = (APP / "tools" / "get_web_emu.sh").read_text()
    commit = re.search(r"^COMMIT=([0-9a-f]{40})$", script, re.M).group(1)
    build = (EMU / "BUILD.txt").read_text()
    assert f"commit {commit}" in build
    assert re.search(r"fm1_web\.wasm sha256 ([0-9a-f]{64})", build).group(1) == hashlib.sha256((EMU / "fm1_web.wasm").read_bytes()).hexdigest()
    r = subprocess.run(["/bin/sh", str(APP / "tools" / "get_web_emu.sh")], capture_output=True, text=True,
                       env={"PATH": "/usr/bin:/bin", "HOME": str(tmp_path)})
    assert r.returncode == 2 and "missing:" in r.stderr and "cargo" in r.stderr


def test_17_2_library_newest_first_after_two_sends(studio):
    page, _, _ = studio()
    open_example(page, "funk.classic suite 1")
    start(page)
    assert send(page).startswith("On the FM-1")
    open_example(page, "funk.classic suite 2")
    assert send(page).startswith('On the FM-1: "funk.classic suite 2"')
    names = page.evaluate("""async () => { const m = await import('./fm1link.js');
        return (await m.savedLibraries()).filter((l) => l.length).map((l) => l.map((p) => p.name)); }""")
    assert ["funk.classic suite 2", "funk.classic suite 1"] in names


@pytest.mark.parametrize("no_open", [False, True])
def test_17_4_fm1c_studio_opens_a_browser_unless_no_open(tmp_path, no_open):
    import os
    flag, opener = tmp_path / "opened", tmp_path / "browser.sh"
    opener.write_text(f'#!/bin/sh\necho "$1" > {flag}\n')
    opener.chmod(0o755)
    p = subprocess.Popen([str(APP / "fm1c"), "studio", "--port", "0", *(["--no-open"] if no_open else [])],
                         stdout=subprocess.PIPE, text=True, env={**os.environ, "BROWSER": str(opener)})
    try:
        url = re.search(r"http://127\.0\.0\.1:\d+/", p.stdout.readline()).group(0)
        end = time.time() + 3
        while time.time() < end and not flag.exists():
            time.sleep(0.1)
        assert flag.exists() != no_open
        if not no_open:
            assert flag.read_text().strip() == url
    finally:
        p.terminate()
        p.wait(10)


def test_17_4_get_web_emu_refuses_a_changed_checkout(tmp_path):
    import os, shutil
    src = Path(os.environ.get("FM1_EMU_SRC") or os.path.expanduser("~/Claude/Projects/fm1-composer/vendor/fm1-emu-src"))
    if not (src / ".git").exists() or not (shutil.which("cargo") or os.path.exists(os.path.expanduser("~/.cargo/bin/cargo"))):
        pytest.skip("no fm1-emu clone (sh tools/get_web_emu.sh makes one) or no cargo")
    clone = tmp_path / "emu"
    subprocess.run(["git", "clone", "-q", "--shared", str(src), str(clone)], check=True)
    commit = re.search(r"^COMMIT=([0-9a-f]{40})$", (APP / "tools" / "get_web_emu.sh").read_text(), re.M).group(1)
    subprocess.run(["git", "-C", str(clone), "checkout", "-q", "--detach", commit], check=True)
    with open(clone / "web" / "main.js", "a") as f:
        f.write("\n// changed\n")
    r = subprocess.run(["sh", str(APP / "tools" / "get_web_emu.sh")], capture_output=True, text=True,
                       env={**os.environ, "FM1_EMU_SRC": str(clone)})
    assert r.returncode == 2 and "has local changes; not building it" in r.stderr


def test_17_2_a_slow_emulator_gets_the_piece_slower_and_saves_it(studio):
    """fm1-emu holds a second of MIDI backlog and drops the rest: when it runs slower than real time (its status line's
    load above 100%), Send slows down with it"""
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    assert f.evaluate("() => window.composerCable.load()") == 1, "1 until fm1-emu shows a load"
    f.wait_for_function("() => / · load \\d+%/.test(document.querySelector('#status').textContent)", timeout=30000)
    text, load = f.evaluate("() => [document.querySelector('#status').textContent, window.composerCable.load()]")
    assert load == max(0.01, int(re.search(r" · load (\d+)%", text)[1]) / 100), (text, load)
    f.evaluate("""() => { const c = window.composerCable, send = c.send.bind(c); window.sent = [];
        c.load = () => 3;                                   // three times slower than real time
        c.send = (b) => { window.sent.push([performance.now(), b.length]); send(b); }; }""")
    assert send(page, secs=180).startswith("On the FM-1")
    sent = f.evaluate("() => window.sent")
    rate = sum(n for _, n in sent[:-1]) / (sent[-1][0] - sent[0][0])
    assert rate < 2.5 / 3 * 1.05, f"{rate:.2f} bytes/ms: at most a third of 2.5"


def test_29_1_a_stalled_emulator_holds_send_back_and_the_piece_still_saves(studio):
    """fm1-emu's emulated clock stops for 3 s (a busy computer): Send keeps at most ~0.4 s of bytes ahead of it (1200
    + one frame), never the second's worth fm1-emu would drop, then finishes, and the piece is read back"""
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    f.wait_for_function("() => Number.isFinite(window.composerCable.emulatedMs())", timeout=30000)
    text, ms = f.evaluate("() => [document.querySelector('#status').textContent, window.composerCable.emulatedMs()]")
    assert ms == float(re.search(r"· (\d+(?:\.\d+)?) s$", text.strip())[1]) * 1000, (text, ms)
    f.evaluate("""() => { const c = window.composerCable, real = c.emulatedMs.bind(c), send = c.send.bind(c);
        window.sent = []; let frozen = null, t0 = null, off = 0;
        c.emulatedMs = () => {                      // stopped for 3 s of wall time from the first byte, then on again
            const now = performance.now(), e = real() - off;
            if (t0 !== null && now - t0 < 3000) { if (frozen === null) frozen = e; return frozen; }
            if (frozen !== null && t0 !== null) { off += e - frozen; frozen = null; t0 = -1e12; return real() - off; }
            return e; };
        c.send = (b) => { if (t0 === null) t0 = performance.now(); window.sent.push([performance.now(), b.length]); send(b); }; }""")
    assert send(page, secs=180).startswith("On the FM-1")
    sent = f.evaluate("() => window.sent")
    t0 = sent[0][0]
    during = sum(n for t, n in sent if t - t0 < 3000)
    biggest = max(n for _, n in sent)
    assert during <= 1200 + biggest, f"{during} bytes sent while the emulator stood still"
    assert any(t - t0 >= 3000 for t, _ in sent), "Send went on once the emulator ran again"


def test_29_2_no_emulated_time_paces_by_the_wall_clock_as_before(studio):
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    f.evaluate("""() => { const c = window.composerCable, send = c.send.bind(c); window.sent = [];
        c.emulatedMs = () => NaN; c.load = () => 1;
        c.send = (b) => { window.sent.push([performance.now(), b.length]); send(b); }; }""")
    assert send(page, secs=120).startswith("On the FM-1")
    sent = f.evaluate("() => window.sent")
    rate = sum(n for _, n in sent[:-1]) / (sent[-1][0] - sent[0][0])
    assert 2.5 * 0.7 < rate < 2.5 * 1.05, f"{rate:.2f} bytes/ms: the wall-clock pace, 2.5"


def test_29_2_an_emulated_clock_that_never_moves_waits_30_s_in_all_then_ends(studio):
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    f.wait_for_function("() => Number.isFinite(window.composerCable.emulatedMs())", timeout=30000)
    f.evaluate("""() => { const c = window.composerCable, send = c.send.bind(c), stuck = c.emulatedMs();
        window.sent = []; c.emulatedMs = () => stuck;
        c.send = (b) => { window.sent.push([performance.now(), b.length]); send(b); }; }""")
    status = send(page, secs=150)
    assert status.startswith("On the FM-1") or status.startswith("Sending stopped"), status     # a status: no hang
    sent = f.evaluate("() => window.sent")
    span = (sent[-1][0] - sent[0][0]) / 1000
    assert 29 < span < 29 + sum(n for _, n in sent) / 2500 + 5, f"{span:.1f} s: 30 s of waiting in all, then on"


def test_29_2_a_press_mid_send_does_nothing_and_the_bar_hides_after(studio):
    """1.5 s into a Send (Studio's 500 ms timer has run since): another press starts no second transfer; once done,
    the progress bar is hidden"""
    page, _, _ = studio()
    open_example(page)
    start(page)
    emu(page).evaluate("""() => { const c = window.composerCable, send = c.send.bind(c); window.__puts = 0;
        c.send = (b) => { if (b[0] === 0xf0 && b[4] === 3) window.__puts++; return send(b); }; }""")
    page.click("#send")
    page.wait_for_timeout(1500)
    assert not page.is_hidden("#send-progress"), "still sending"
    assert page.is_disabled("#send"), "Send is disabled while it sends"
    page.click("#send", force=True)
    page.wait_for_function("() => /^(On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)", timeout=90000)
    assert page.text_content("#status").startswith("On the FM-1"), diag(page)
    assert emu(page).evaluate("() => window.__puts") == 1, "one transfer"
    assert page.is_hidden("#send-progress")


def test_29_2_a_slow_emulator_gets_a_longer_read_back_before_did_not_save(studio):
    """load 300% and nothing arrives: Send reads back for 20 x 3 tries of 500 ms (30 s), not 10 s, before it gives up"""
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    f.evaluate("""() => { const c = window.composerCable; window.sent = [];
        c.load = () => 3; c.emulatedMs = () => NaN; c.send = (b) => { window.sent.push(performance.now()); }; }""")
    status = send(page, secs=150)
    assert status.startswith("Sending stopped: the FM-1 did not save"), status
    last, end = f.evaluate("() => window.sent[window.sent.length - 1]"), page.evaluate("() => performance.now()")
    assert end - last >= 28000, f"{(end - last) / 1000:.1f} s of reading back"


def test_29_3_the_read_back_lasts_10_s_of_the_emulators_own_time(studio):
    """fm1-emu runs at a third of real time (a computer too busy for it) and nothing arrives: Send reads back until
    fm1-emu has run 10 s of its own time (~30 s here), not 10 s of the wall clock, then says it did not save"""
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    f.wait_for_function("() => Number.isFinite(window.composerCable.emulatedMs())", timeout=30000)
    f.evaluate("""() => { const c = window.composerCable, t0 = performance.now(), e0 = c.emulatedMs(); window.sent = [];
        c.emulatedMs = () => e0 + (performance.now() - t0) / 3;      // a third of real time
        c.send = (b) => { window.sent.push(performance.now()); }; }""")
    status = send(page, secs=150)
    assert status.startswith("Sending stopped: the FM-1 did not save"), status
    last, end = f.evaluate("() => window.sent[window.sent.length - 1]"), page.evaluate("() => performance.now()")
    assert 28000 <= end - last <= 40000, f"{(end - last) / 1000:.1f} s of reading back: 10 s of fm1-emu's time"
    log = page.evaluate("() => window.studioSendLog")
    assert log["eEnd"] - log["eSent"] >= 10000 and log["bytes"] > 0, log


def test_17_2_a_resend_that_never_arrives_is_not_called_saved(studio):
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    assert send(page).startswith("On the FM-1")
    f.evaluate("() => { window.composerCable.send = () => {}; }")      # the cable drops everything now
    status = send(page, secs=120)
    assert status.startswith("Sending stopped: the FM-1 did not save"), status
