"""US-15: the FM-1 emulator (fm1-emu's browser build) on Studio's page, running the CI-built COMPOSER; pieces pushed
over its MIDI jack. The LCD is compared pixel for pixel with what host mode draws after the same pushes."""
import hashlib, json, os, re, subprocess, time
import pytest
from conftest import APP
from test_emu import host_ref
from test_studio import IDEA, brief_text, browser, generate, studio, studio_url  # noqa: F401  (fixtures)
from test_engine import BRIEFS, run, studio_cli  # noqa: F401

FWSC = APP / "build" / "composer.fwsc"
EMU = APP / "studio" / "web" / "emu"

pytestmark = pytest.mark.skipif(not (FWSC.exists() and (EMU / "fm1_web.wasm").exists()),
                                reason="needs build/composer.fwsc (from CI) and studio/web/emu (sh tools/get_web_emu.sh)")


def lcd(page):
    return page.evaluate("() => window.studioFm1 && window.studioFm1.lcd && Array.from(window.studioFm1.lcd)")


def as_rgb565_bytes(px):
    out = bytearray()
    for p in px:
        out += bytes((p >> 11, (p >> 5) & 63, p & 31))
    return bytes(out)


def wait_lcd(page, ref, secs=15):
    end, got = time.time() + secs, None
    while time.time() < end:
        px = lcd(page)
        if px and (got := as_rgb565_bytes(px)) == ref:
            return True
        time.sleep(0.3)
    return False


def wait_lcd_changed(page, ref, secs=5):
    end = time.time() + secs
    while time.time() < end:
        px = lcd(page)
        if px and as_rgb565_bytes(px) != ref:
            return True
        time.sleep(0.2)
    return False


def level(page):
    return float(page.get_attribute("#emu-level", "data-level") or 0)


def wait_level(page, above=None, below=None, secs=6):
    end = time.time() + secs
    while time.time() < end:
        v = level(page)
        if (above is not None and v > above) or (below is not None and v < below):
            return v
        time.sleep(0.2)
    return level(page)


def boot(studio, **kw):
    page, calls, todo = studio(**kw)
    page.evaluate("() => localStorage.removeItem('composer-studio.flash')")
    page.click("#emu-start")
    page.wait_for_function("() => document.querySelector('#emu-status').textContent.includes('COMPOSER is running')", timeout=20000)
    return page, calls, todo


def made(page, todo, seed=7):
    todo.append(("reply", brief_text()))
    generate(page, seed=seed)
    page.wait_for_selector("#piece:not([hidden])")
    with page.expect_download() as d:
        page.click("#dl-f1p")
    return open(d.value.path(), "rb").read()


def push(page, secs=60):
    page.click("#push")
    page.wait_for_function("() => /on the FM-1|could not|did not|first|already/.test(document.querySelector('#status').textContent)",
                           timeout=secs * 1000)
    return page.text_content("#status")


def test_15_1_page_boots_composer_screen_is_host_modes(studio, tmp_path):
    page, _, _ = boot(studio)
    assert wait_lcd(page, host_ref(tmp_path, "sboot", []))
    assert page.text_content("#emu-library") == "No pieces yet."


def test_15_2_generate_push_listed_read_back_then_play(studio, tmp_path):
    page, _, todo = boot(studio)
    f1p = made(page, todo)
    assert page.is_enabled("#push")
    msg = push(page)
    assert "Dusty Rain" in msg and "on the FM-1" in msg, msg
    assert page.get_attribute("#push-progress", "value") == "100"
    assert "Dusty Rain" in page.text_content("#emu-library")
    assert wait_lcd(page, host_ref(tmp_path, "spush", [f1p]))
    assert level(page) < 0.001
    page.click("#emu-panel [data-key='12']")                       # PLAY on the page's panel
    assert wait_level(page, above=0.002) > 0.002


def test_15_3_panel_and_keyboard_reach_the_fm1(studio, tmp_path):
    page, _, todo = boot(studio)
    a = made(page, todo, seed=1)
    push(page)
    b = made(page, todo, seed=2)
    push(page)
    ref = host_ref(tmp_path, "stwo", [a, b])
    assert wait_lcd(page, ref)
    page.click("[data-enc='0'] >> nth=1")                         # SELECT + : the cursor moves to the older piece
    assert wait_lcd_changed(page, ref)
    page.click("[data-enc='0'] >> nth=0")                         # SELECT - : back
    assert wait_lcd(page, ref)
    page.locator("body").press(" ")                               # space = PLAY
    assert wait_level(page, above=0.002) > 0.002
    page.locator("body").press("Escape")                          # Esc = HOME: stops, back to the library
    assert wait_level(page, below=0.0005) < 0.0005 and wait_lcd(page, ref)


def test_15_4_push_before_start_and_twice(studio):
    page, _, todo = studio()
    made(page, todo)
    assert push(page) == "Start the FM-1 first."
    page.click("#emu-start")
    page.wait_for_function("() => document.querySelector('#emu-status').textContent.includes('COMPOSER is running')", timeout=20000)
    page.evaluate("() => { document.querySelector('#push').click(); document.querySelector('#push').click(); }")
    page.wait_for_function("() => document.querySelector('#status').textContent.includes('on the FM-1')", timeout=60000)
    assert page.text_content("#emu-library").count("Dusty Rain") == 1


def test_15_4_a_32_kb_piece(studio, studio_cli, tmp_path):
    page, _, _ = boot(studio)
    body = bytearray()                                            # chords on every beat: cut to fit 32 KB
    for i in range(1200):
        for k, p in enumerate((48, 52, 55, 60, 64, 67)):
            body += (b"\x60" if (k == 0 and i) else b"\x00") + bytes([0x90, p, 90])
        for p in (48, 52, 55, 60, 64, 67):
            body += b"\x00" + bytes([0x80, p, 0])
    body += b"\x00\xff\x2f\x00"
    import struct
    mid = tmp_path / "Dense.mid"
    mid.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 1, 1, 96) + b"MTrk" + struct.pack(">I", len(body)) + bytes(body))
    page.set_input_files("#midi-file", str(mid))
    page.wait_for_selector("#piece:not([hidden])")
    assert int(re.search(r"(\d+) bytes", page.text_content("#piece-stats")).group(1)) > 30000
    assert "on the FM-1" in push(page, secs=90)
    assert "Dense" in page.text_content("#emu-library")


def test_15_4_reload_mid_push_library_whole_or_without_it(studio, tmp_path):
    page, _, todo = boot(studio)
    first = made(page, todo, seed=1)
    push(page)
    made(page, todo, seed=2)
    page.click("#push")
    time.sleep(1.0)                                               # the second push half way
    page.reload()
    page.wait_for_selector("body[data-ready='1']")
    page.click("#emu-start")
    page.wait_for_function("() => document.querySelector('#emu-status').textContent.includes('COMPOSER is running')", timeout=20000)
    assert page.text_content("#emu-library").count("Dusty Rain") == 1        # kept from before, the cut push not there
    assert wait_lcd(page, host_ref(tmp_path, "sreload", [first]))


def test_15_5_get_web_emu_pins_and_records_and_names_missing_tools(tmp_path):
    script = (APP / "tools" / "get_web_emu.sh").read_text()
    commit = re.search(r"^COMMIT=([0-9a-f]{40})$", script, re.M).group(1)
    if (APP.parent / "SPEC.md").exists():                       # (not in a copy of app/ alone)
        assert re.search(r"fm1-emu's web build \(MIT, commit " + commit[:12], (APP.parent / "SPEC.md").read_text())
    build = (EMU / "BUILD.txt").read_text()
    assert f"commit {commit}" in build
    assert re.search(r"sha256 ([0-9a-f]{64})", build).group(1) == hashlib.sha256((EMU / "fm1_web.wasm").read_bytes()).hexdigest()
    r = subprocess.run(["/bin/sh", str(APP / "tools" / "get_web_emu.sh")], capture_output=True, text=True,
                       env={"PATH": "/usr/bin:/bin", "HOME": str(tmp_path)})
    assert r.returncode == 2 and "missing:" in r.stderr and "cargo" in r.stderr


@pytest.mark.parametrize("fw,msg", [("garbage", "The emulator stopped: boot:"), ("missing", "firmware/composer.fwsc is missing (404)")])
def test_15_4_a_bad_or_missing_firmware_says_so_and_start_works_again(studio, fw, msg):
    page, _, _ = studio()
    page.route("**/firmware/composer.fwsc", lambda r: r.fulfill(status=200, body=b"not a package" * 100) if fw == "garbage"
               else r.fulfill(status=404, body="no"))
    page.click("#emu-start")
    page.wait_for_function("m => document.querySelector('#emu-status').textContent.includes(m)", arg=msg, timeout=20000)
    assert page.is_enabled("#emu-start") and page.text_content("#emu-start") == "Start the FM-1"
    page.unroute("**/firmware/composer.fwsc")
    page.click("#emu-start")                                      # the real firmware now: it starts
    page.wait_for_function("() => document.querySelector('#emu-status').textContent.includes('COMPOSER is running')", timeout=20000)


@pytest.mark.parametrize("no_open", [False, True])
def test_15_5_fm1c_studio_opens_a_browser_unless_no_open(tmp_path, no_open):
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
