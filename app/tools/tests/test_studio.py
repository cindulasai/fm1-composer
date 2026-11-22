"""COMPOSER Studio, served by the real `fm1c studio` and driven in headless Chromium: text2midi's window (left) and
fm1-emu's page (right). The AI is faked: every request off this computer is intercepted. Here: the fixtures, the key
file (14.4) and the server (14.5); text2midi's side in test_studio_t2m.py, the FM-1's in test_studio_emu.py."""
import json, os, re, stat, subprocess, time, urllib.request
import pytest
from pathlib import Path
from conftest import APP
from test_engine import BRIEFS, run, studio_cli  # noqa: F401  (studio_cli: a fixture)

REPO = APP.parent
AI_HOSTS = re.compile(r"^https://(api\.minimax\.io|api\.deepseek\.com|api\.anthropic\.com|api\.openai\.com|[a-z.]*googleapis\.com|api\.groq\.com|api\.x\.ai|api\.mistral\.ai|openrouter\.ai)/")
IDEA = "slow melancholic lo-fi in D minor, 70 bpm, dusty piano"
ASSUMED = {"field": "/time/tempo_bpm", "because": "lo-fi is usually slow", "confidence": 0.6}


def brief_text():
    b = json.loads((BRIEFS / "hiphop-lofi-1.json").read_text())
    b["identity"]["title"] = "Dusty Rain"
    b["assumptions"] = [ASSUMED]
    return json.dumps(b)


def body(text):
    return json.dumps({"choices": [{"message": {"role": "assistant", "content": text}}]})


@pytest.fixture(scope="module")
def studio_url():
    p = subprocess.Popen([str(APP / "fm1c"), "studio", "--no-open", "--port", "0"], stdout=subprocess.PIPE, text=True)
    seen = ""
    for _ in range(50):                                            # (a first run builds Studio before serving it)
        line = p.stdout.readline()
        seen += line
        m = re.search(r"http://127\.0\.0\.1:\d+/", line)
        if m or not line:
            break
    assert m, seen
    yield m.group(0)
    p.terminate()
    p.wait(10)


@pytest.fixture(scope="module")
def browser(pw_browser):
    return pw_browser


@pytest.fixture
def studio(browser, studio_url):
    """open(key, script, query) -> (page, calls); script: what the fake AI does for each request, in order"""
    ctxs = []

    def open_(key="test-key", script=(), query="", routes=None, init=None):
        ctx = browser.new_context(accept_downloads=True)
        ctxs.append(ctx)
        if init:
            ctx.add_init_script(init)
        page = ctx.new_page()
        calls, todo = [], list(script)

        def ai(route, request):
            if not AI_HOSTS.search(request.url):                  # (fm1-emu's look at its firmware hub: offline here)
                return route.abort("internetdisconnected")
            calls.append(request)
            what = todo.pop(0) if todo else ("status", 500, "{}")
            if what[0] == "reply":
                route.fulfill(status=200, content_type="application/json", body=body(what[1]))
            elif what[0] == "slow":
                time.sleep(1.0)
                route.fulfill(status=200, content_type="application/json", body=body(what[1]))
            elif what[0] == "status":
                route.fulfill(status=what[1], content_type="application/json", body=what[2])
            elif what[0] == "abort":
                route.abort("internetdisconnected")
            # "hang": never answered

        page.route("**/local/ai.json", lambda r: r.fulfill(status=200, content_type="application/json",
                                                           body=json.dumps({"provider": "minimax", "key": key}))
                   if key else r.fulfill(status=404, body="no"))
        page.route(re.compile(r"^https://"), ai)
        for pattern, handler in (routes or {}).items():
            page.route(pattern, handler)
        page.goto(studio_url + query)
        t2m = page.frame_locator("#t2m")
        t2m.locator("text=funk.classic suite 1").wait_for(timeout=30000)        # text2midi's gallery is up
        return page, calls, todo

    yield open_
    for c in ctxs:
        c.close()


# ---- 14.4: the key file ----
def key_script(tmp_path, **env):
    e = {k: v for k, v in os.environ.items() if k != "MINIMAX_API_KEY"}
    e.update({"T2M_ENV": str(tmp_path / "none.env"), "STUDIO_KEY_OUT": str(tmp_path / "local" / "ai.json"), **env})
    return subprocess.run(["sh", str(APP / "tools" / "studio_key.sh")], capture_output=True, text=True, env=e)


def test_14_4_key_file_written_private_and_never_printed(tmp_path):
    r = key_script(tmp_path, MINIMAX_API_KEY="abc.DEF-123_xyz")
    out = tmp_path / "local" / "ai.json"
    assert r.returncode == 0 and "abc.DEF" not in r.stdout + r.stderr
    assert json.loads(out.read_text()) == {"provider": "minimax", "key": "abc.DEF-123_xyz"}
    assert stat.S_IMODE(out.stat().st_mode) == 0o600
    (tmp_path / "t.env").write_text('OTHER=1\nMINIMAX_API_KEY="from.env-file"\n')
    r = key_script(tmp_path, T2M_ENV=str(tmp_path / "t.env"))
    assert r.returncode == 0 and json.loads(out.read_text())["key"] == "from.env-file"
    r = key_script(tmp_path, T2M_ENV=str(tmp_path / "t.env"), MINIMAX_API_KEY="exported.key")     # the exported one wins
    assert r.returncode == 0 and json.loads(out.read_text())["key"] == "exported.key"


@pytest.mark.parametrize("env,msg", [({}, "no key"), ({"MINIMAX_API_KEY": 'a"b; rm -rf ~'}, "characters")])
def test_14_4_key_script_refuses(tmp_path, env, msg):
    r = key_script(tmp_path, **env)
    assert r.returncode == 2 and msg in r.stderr and not (tmp_path / "local" / "ai.json").exists()


def test_14_4_the_key_is_in_no_tracked_file():
    top = subprocess.run(["git", "-C", str(APP), "rev-parse", "--show-toplevel"], capture_output=True, text=True)
    if top.returncode or Path(top.stdout.strip()) != REPO:
        pytest.skip("not a git checkout (a copy of app/): nothing here is tracked")
    assert subprocess.run(["git", "-C", str(REPO), "check-ignore", "-q", "app/studio/web/local/ai.json"]).returncode == 0
    keys = []
    local = APP / "studio" / "web" / "local" / "ai.json"
    if local.exists():
        keys.append(json.loads(local.read_text())["key"])
    envf = os.path.expanduser("~/Claude/Projects/text2midi_v2/.env")
    if os.path.exists(envf):
        keys += [ln.split("=", 1)[1].strip().strip("\"'") for ln in open(envf) if ln.startswith("MINIMAX_API_KEY=")]
    keys = [k.encode() for k in keys if len(k) >= 16]
    if not keys:
        pytest.skip("no real key on this computer to look for")
    files = subprocess.run(["git", "-C", str(REPO), "ls-files", "-z"], capture_output=True).stdout.split(b"\0")
    hits = [f.decode() for f in files if f and (REPO / f.decode()).is_file()
            and any(k in (REPO / f.decode()).read_bytes() for k in keys)]
    assert not hits, f"the AI key is in tracked files: {hits}"   # (names only; the key is never shown)


# ---- 14.5: fm1c studio ----
def test_14_5_fm1c_studio_serves_the_page_on_this_computer_only(studio_url, studio):
    assert studio_url.startswith("http://127.0.0.1:")
    html = urllib.request.urlopen(studio_url).read().decode()
    assert "<title>COMPOSER Studio</title>" in html
    w = urllib.request.urlopen(studio_url + "engine/studio_bg.wasm")
    assert w.headers["Content-Type"] == "application/wasm" and w.headers["Cache-Control"] == "no-store"
    page, _, _ = studio()
    assert page.frame_locator("#t2m").locator("text=Make it").count() == 1


@pytest.mark.parametrize("args,msg", [(["--port", "70000"], "--port must be 0 to 65535"), (["--port", "x"], "invalid int")])
def test_14_5_fm1c_studio_bad_port(args, msg):
    r = run(APP / "fm1c", "studio", "--no-open", *args)
    assert r.returncode == 2 and msg in r.stderr and "Traceback" not in r.stderr


def test_14_5_fm1c_studio_port_in_use(studio_url):
    port = studio_url.rsplit(":", 1)[1].strip("/")
    r = run(APP / "fm1c", "studio", "--no-open", "--port", port)
    assert r.returncode == 2 and "cannot serve COMPOSER Studio on 127.0.0.1" in r.stderr


