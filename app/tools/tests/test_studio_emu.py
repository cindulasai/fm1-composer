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


def send(page, secs=90):
    page.click("#send")
    page.wait_for_function("() => /^(On the FM-1|Sending stopped)/.test(document.querySelector('#status').textContent)",
                           timeout=secs * 1000)
    return page.text_content("#status")


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
    assert page.text_content("#status").startswith("On the FM-1")
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


def test_17_2_a_resend_that_never_arrives_is_not_called_saved(studio):
    page, _, _ = studio()
    open_example(page)
    f = start(page)
    assert send(page).startswith("On the FM-1")
    f.evaluate("() => { window.composerCable.send = () => {}; }")      # the cable drops everything now
    status = send(page, secs=120)
    assert status.startswith("Sending stopped: the FM-1 did not save"), status
