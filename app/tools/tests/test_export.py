"""US-27: `fm1c studio --export DIR` writes Studio as static files (the server's rewrites baked in), served here by a
plain `python -m http.server`, no fm1c. Its AI key (local/) never goes out; text2midi's key panel says the browser."""
import functools, http.server, json, os, re, subprocess, sys, threading, urllib.request
from pathlib import Path
import pytest
from conftest import APP
sys.path.insert(0, str(APP / "tools"))                    # (run alone, as CI's Pages step does)
from fm1c import cli  # noqa: E402
from test_studio import browser, studio  # noqa: F401,E402  (fixtures; studio_url is this module's: the static export)
from test_studio_emu import AUDIO_TAP, EMU, FWSC, emu, open_example, send, start, t2m

WEB = APP / "studio" / "web"
FM1C = str(APP / "fm1c")
FAKE_KEY = "sk-test-FAKE-0123456789abcdef"
REQUIRED = os.environ.get("FM1_REQUIRE_STUDIO") == "1"       # (CI's Pages step: a missing piece fails, never skips)
NEEDS = "needs build/composer.fwsc (from CI) and studio/web/emu (sh tools/get_web_emu.sh)"


def emu_ready():
    return FWSC.is_file() and (EMU / "fm1_web.wasm").is_file() and (EMU / "index.html").is_file()


def need_emu(ready):
    """skip without the firmware / fm1-emu's page; FM1_REQUIRE_STUDIO=1 makes that a failure"""
    if not ready:
        if REQUIRED:
            pytest.fail("FM1_REQUIRE_STUDIO=1: " + NEEDS)
        pytest.skip(NEEDS)


needs_emu = pytest.mark.skipif(not emu_ready() and not REQUIRED, reason=NEEDS)


def export(out, *more, **env):
    return subprocess.run([FM1C, "studio", "--export", str(out), *more], capture_output=True, text=True,
                          env={**os.environ, **env})


def files(d):
    return sorted(p.relative_to(d).as_posix() for p in Path(d).rglob("*") if p.is_file())


@pytest.fixture(scope="module")
def exported(tmp_path_factory):
    need_emu(emu_ready())
    out = tmp_path_factory.mktemp("export") / "Studio site"                 # (a path with a space)
    r = export(out)
    assert r.returncode == 0, r.stderr
    return out


@pytest.fixture(scope="module")
def studio_url(exported):
    """the export behind a plain static server: python's own, not fm1c"""
    p = subprocess.Popen([sys.executable, "-u", "-m", "http.server", "0", "--bind", "127.0.0.1", "--directory",
                          str(exported)], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
    m = re.search(r"port (\d+)", p.stdout.readline())
    assert m
    yield f"http://127.0.0.1:{m.group(1)}/"
    p.terminate()
    p.wait(10)


@pytest.fixture(scope="module")
def served_url():
    """the live `fm1c studio` handler on Studio's own files"""
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(cli._StudioHandler, directory=str(WEB)))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{srv.server_address[1]}/"
    srv.shutdown()
    srv.server_close()


def get(url):
    return urllib.request.urlopen(url).read()


# ---- 27.1: the export, served statically, is the whole Studio ----
@needs_emu
def test_27_1_static_export_boots_composer_and_a_sent_piece_plays(studio, studio_url, exported):
    cat = json.loads(get(studio_url + "emu/firmwares.json"))
    assert cat["default"] == "composer" and cat["firmwares"][0]["package"] == "../firmware/composer.fwsc"
    assert get(studio_url + "firmware/composer.fwsc") == FWSC.read_bytes()
    page, _, _ = studio(init=AUDIO_TAP)
    fw = []
    page.on("response", lambda r: fw.append((r.url, r.status)) if r.url.endswith("/composer.fwsc") else None)
    assert t2m(page).url.startswith(studio_url) and emu(page).url.startswith(studio_url)        # both windows
    emu(page).wait_for_function("() => document.querySelector('#midi').value === 'COMPOSER Studio'")
    open_example(page)
    f = start(page)                                                  # COMPOSER · ... in fm1-emu's status
    assert (studio_url + "firmware/composer.fwsc", 200) in fw, fw
    assert send(page).startswith('On the FM-1: "funk.classic suite 1"')
    f.click("#big")
    page.keyboard.down(" ")                                          # PLAY, held as a finger does
    page.wait_for_timeout(250)
    page.keyboard.up(" ")
    f.wait_for_function("() => window.__fm1Peak && window.__fm1Peak() > 0.01", timeout=15000)


# ---- 27.2: export == what `fm1c studio` serves ----
def test_27_2_export_equals_the_served_studio(exported, served_url):
    got = files(exported)
    want = sorted([f for f in files(WEB) if not f.startswith("local/") and not Path(f).name.startswith(".")]
                  + ["firmware/composer.fwsc"])
    assert got == want and not (exported / "local").exists()
    page = (exported / "emu" / "index.html").read_text()
    assert page == get(served_url + "emu/index.html").decode() == get(served_url + "emu/").decode()
    assert page.index('<script src="../cable.js"></script>') == page.index("<script")             # the cable first
    assert (exported / "emu" / "firmwares.json").read_bytes() == get(served_url + "emu/firmwares.json")
    assert (exported / "firmware" / "composer.fwsc").read_bytes() == get(served_url + "firmware/composer.fwsc")
    for f in got:
        if f not in ("emu/index.html", "emu/firmwares.json", "firmware/composer.fwsc"):
            assert (exported / f).read_bytes() == (WEB / f).read_bytes() == get(served_url + f), f


# ---- 27.3: hostile ----
@pytest.fixture
def fake(tmp_path):
    """a small Studio tree (FM1C_STUDIO_WEB) with an AI key planted in local/, and a firmware (FM1C_FWSC)"""
    web = tmp_path / "web"
    for rel, text in {"index.html": "<p>studio</p>", "engine/studio.js": "//", "t2m/index.html": "<p>t2m</p>",
                      "emu/index.html": "<html><script src=main.js></script></html>",
                      "emu/firmwares.json": json.dumps({"default": "x", "firmwares": [{"id": "x"}]}),
                      "local/ai.json": json.dumps({"provider": "minimax", "key": FAKE_KEY}), ".env": FAKE_KEY,
                      "emu/.hidden": FAKE_KEY}.items():
        (web / rel).parent.mkdir(parents=True, exist_ok=True)
        (web / rel).write_text(text)
    (web / "key-link.json").symlink_to(web / "local" / "ai.json")         # a link to the key: not followed out
    (tmp_path / "composer.fwsc").write_bytes(b"FWSC")
    return {"FM1C_STUDIO_WEB": str(web), "FM1C_FWSC": str(tmp_path / "composer.fwsc")}


def no_trace(r):
    assert "Traceback" not in r.stderr + r.stdout
    return r


def test_27_3_the_key_never_goes_out(tmp_path, fake):
    out = tmp_path / "out dir"
    r = no_trace(export(out, **fake))
    assert r.returncode == 0, r.stderr
    assert not (out / "local").exists() and FAKE_KEY not in r.stdout + r.stderr
    for f in files(out):
        assert FAKE_KEY.encode() not in (out / f).read_bytes(), f
    assert files(out) == ["emu/firmwares.json", "emu/index.html", "engine/studio.js", "firmware/composer.fwsc",
                          "index.html", "t2m/index.html"]
    assert json.loads((out / "emu" / "firmwares.json").read_text())["firmwares"][0]["id"] == "composer"


@pytest.mark.parametrize("case,expect", [
    ("non-empty", "is not empty (--force replaces it)"),
    ("a file", "is a file, not a folder"),
    ("unwritable", "cannot write there"),
    ("unwritable parent", "Permission denied"),
    ("firmware missing", "composer-fwsc artifact"),
    ("not built", "app/studio/build.sh"),
    ("no emu page", "tools/get_web_emu.sh"),
    ("studio's own folder", "Studio's own folder"),
    ("force without export", "--force goes with --export"),
])
def test_27_3_refusals_exit_2_one_line(tmp_path, fake, case, expect):
    out, web = tmp_path / "out", Path(fake["FM1C_STUDIO_WEB"])
    if case == "non-empty":
        out.mkdir()
        (out / "keep.txt").write_text("mine")
    elif case == "a file":
        out.write_text("a file")
    elif case.startswith("unwritable"):
        out.mkdir(mode=0o500)
        if case == "unwritable parent":
            out = out / "inner"
    elif case == "firmware missing":
        fake["FM1C_FWSC"] = str(tmp_path / "missing.fwsc")
    elif case == "not built":
        (web / "engine" / "studio.js").unlink()
    elif case == "no emu page":
        (web / "emu" / "index.html").unlink()
    elif case == "studio's own folder":
        out = web / "t2m"
    try:
        if case == "force without export":
            r = subprocess.run([FM1C, "studio", "--force"], capture_output=True, text=True)
        else:
            r = export(out, **fake)
    finally:
        os.chmod(tmp_path, 0o700)
        if (tmp_path / "out").is_dir():
            os.chmod(tmp_path / "out", 0o700)
    no_trace(r)
    assert r.returncode == 2 and expect in r.stderr and len(r.stderr.strip().splitlines()) == 1, r.stderr
    if case == "non-empty":
        assert (out / "keep.txt").read_text() == "mine"                  # nothing touched without --force


def test_27_3_force_replaces_and_spaces_work(tmp_path, fake):
    out = tmp_path / "my site" / "studio here"
    out.mkdir(parents=True)
    (out / "old.txt").write_text("old")
    (out / "old dir").mkdir()
    (out / "old dir" / "x").write_text("x")
    r = no_trace(export(out, "--force", **fake))
    assert r.returncode == 0, r.stderr
    assert not (out / "old.txt").exists() and not (out / "old dir").exists()
    assert (out / "firmware" / "composer.fwsc").read_bytes() == b"FWSC" and str(out) in r.stdout


# ---- 27.4: the key panel says the browser, not a keychain ----
def settings_text(browser, url):
    ctx = browser.new_context()
    try:
        page = ctx.new_page()
        page.route("**/local/ai.json", lambda r: r.fulfill(status=404, body="no"))
        page.route(re.compile(r"^https://"), lambda r: r.abort("internetdisconnected"))
        page.goto(url)
        f = page.frame_locator("#t2m")
        f.locator("text=funk.classic suite 1").wait_for(timeout=30000)
        f.locator("button.gear").click()
        f.locator("p.privacy").wait_for()
        return f.locator(".panel").inner_text(), f.locator("p.privacy").inner_text()
    finally:
        ctx.close()


@pytest.mark.parametrize("where", ["exported", "served"])
def test_27_4_key_panel_says_this_browser(browser, request, where):
    url = request.getfixturevalue("studio_url" if where == "exported" else "served_url")
    panel, privacy = settings_text(browser, url)
    assert privacy == ("Your key is kept in this browser (its local storage), never in a file or sent to us. "
                       "Only the words you type in the description box are sent to the provider.")
    assert "keychain" not in panel.lower()


def test_27_4_no_keychain_in_studios_t2m_build_and_its_sources_unchanged():
    for f in files(WEB / "t2m"):
        assert b"keychain" not in (WEB / "t2m" / f).read_bytes().lower(), f
    assert b"kept in this browser" in b"".join((WEB / "t2m" / f).read_bytes() for f in files(WEB / "t2m"))
    # text2midi's Settings.svelte itself still says keychain: only the build changes it (16.1 checks every file)
    assert "system keychain" in (APP / "studio" / "t2m-ui" / "src" / "Settings.svelte").read_text()


def test_27_3_a_stale_firmware_in_studios_tree_is_replaced_by_the_real_one(tmp_path, fake):
    """Studio's tree already has firmware/composer.fwsc (an old one): the export still works and carries FM1C_FWSC's"""
    stale = Path(fake["FM1C_STUDIO_WEB"]) / "firmware" / "composer.fwsc"
    stale.parent.mkdir()
    stale.write_bytes(b"OLD")
    out = tmp_path / "out"
    r = no_trace(export(out, **fake))
    assert r.returncode == 0, r.stderr
    assert (out / "firmware" / "composer.fwsc").read_bytes() == b"FWSC"
