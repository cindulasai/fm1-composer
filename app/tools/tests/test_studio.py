"""US-14: COMPOSER Studio's page: an idea -> the AI (faked here: every request off this computer is intercepted) ->
a brief -> a piece. Served by the real `fm1c studio`, driven in headless Chromium."""
import json, os, re, stat, subprocess, time, urllib.request
import pytest
from pathlib import Path
from conftest import APP
from test_engine import BRIEFS, run, studio_cli  # noqa: F401  (studio_cli: a fixture)

REPO = APP.parent
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
    line = p.stdout.readline()
    m = re.search(r"http://127\.0\.0\.1:\d+/", line)
    assert m, line
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

    def open_(key="test-key", script=(), query=""):
        ctx = browser.new_context(accept_downloads=True)
        ctxs.append(ctx)
        page = ctx.new_page()
        calls, todo = [], list(script)

        def ai(route, request):
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
        page.goto(studio_url + query)
        page.wait_for_selector("body[data-ready='1']")
        return page, calls, todo

    yield open_
    for c in ctxs:
        c.close()


def generate(page, idea=IDEA, seed=None):
    page.fill("#idea", idea)
    if seed is not None:
        page.fill("#seed", str(seed))
    page.click("#generate")


def wait_done(page):
    page.wait_for_function("() => !document.querySelector('#generate').disabled")


def test_14_1_idea_to_brief_to_piece_and_downloads(studio, studio_cli, tmp_path):
    page, calls, _ = studio(script=[("reply", "Here it is:\n```json\n" + brief_text() + "\n```")])
    generate(page, seed=7)
    page.wait_for_selector("#piece:not([hidden])")
    req = calls[0]
    assert req.url == "https://api.minimax.io/v1/chat/completions" and req.headers["authorization"] == "Bearer test-key"
    sent = req.post_data_json
    assert sent["model"] == "MiniMax-M3.1-Flash" and sent["messages"][1] == {"role": "user", "content": IDEA}
    assert sent["max_completion_tokens"] == 8000 and sent["reasoning_split"] is True        # as text2midi's request
    assert sent["messages"][0]["content"].startswith("You turn a short description") and "hiphop.lofi" in sent["messages"][0]["content"]
    assert page.text_content("#brief-title") == "Dusty Rain" and page.text_content("#brief-style") == "hiphop.lofi"
    assert page.text_content("#brief-key") == "A dorian"
    assert page.text_content("#brief-tempo") == "75 BPM" and "lo-fi is usually slow" in page.text_content("#brief-assumptions")
    assert re.match(r"^4 channels, \d+ notes, 32 bars, 75 BPM, \d+ bytes$", page.text_content("#piece-stats"))
    got = {}
    for btn in ("#dl-f1p", "#dl-mid"):
        with page.expect_download() as d:
            page.click(btn)
        got[btn] = open(d.value.path(), "rb").read()
        assert d.value.suggested_filename == "Dusty Rain" + (".f1p" if btn == "#dl-f1p" else ".mid")
    (tmp_path / "answer.txt").write_text(brief_text())
    r = run(studio_cli, "brief", tmp_path / "answer.txt")                  # the brief as the page read it back
    (tmp_path / "b.json").write_text(r.stdout)
    r = run(studio_cli, "make", tmp_path / "b.json", "--seed", 7, "--name", "Dusty Rain", "-o", tmp_path / "x.f1p",
            "--mid", tmp_path / "x.mid")
    assert r.returncode == 0, r.stderr
    assert got["#dl-f1p"] == (tmp_path / "x.f1p").read_bytes() and got["#dl-mid"] == (tmp_path / "x.mid").read_bytes()
    page.click("#again")                                                   # another take: a new seed, no AI call
    assert len(calls) == 1 and page.input_value("#seed") != "7"


def test_14_2_unusable_answer_asked_once_more(studio):
    page, calls, _ = studio(script=[("reply", "Sorry, I can only write poems."), ("reply", brief_text())])
    generate(page)
    page.wait_for_selector("#piece:not([hidden])")
    assert len(calls) == 2
    assert calls[1].post_data_json["messages"][1]["content"] == (
        IDEA + "\n\nYour previous answer could not be used (no JSON object found). Answer again with only the JSON brief.")


def test_14_2_two_unusable_answers_is_an_error(studio):
    page, calls, _ = studio(script=[("reply", "no"), ("reply", '{"version": 1}')])
    generate(page)
    wait_done(page)
    assert len(calls) == 2 and "could not be used" in page.text_content("#status")
    assert page.get_attribute("#status", "class") == "error" and page.is_hidden("#piece")


HOSTILE = {   # case: (script, query, idea, message)
    "401": ([("status", 401, '{"error": {"message": "invalid api key"}}')], "", IDEA, "refused the key (401)"),
    "500": ([("status", 500, '{"base_resp": {"status_code": 1, "status_msg": "overloaded"}}')], "", IDEA, "answered 500: overloaded"),
    "network down": ([("abort",)], "", IDEA, "Could not reach the AI (MiniMax)"),
    "timeout": ([("hang",)], "?timeout=1500", IDEA, "took longer than 1.5 s"),
    "empty idea": ([], "", "   ", "Describe the music first"),
    "2001 chars": ([], "", "x" * 2001, "Keep the idea under 2000 characters (it has 2001)"),
}


@pytest.mark.parametrize("case", list(HOSTILE))
def test_14_3_hostile_each_one_clear_message_page_stays_usable(studio, case):
    script, query, idea, msg = HOSTILE[case]
    page, calls, todo = studio(script=script, query=query)
    generate(page, idea=idea)
    page.wait_for_function("m => document.querySelector('#status').textContent.includes(m)", arg=msg)
    wait_done(page)
    assert page.get_attribute("#status", "class") == "error" and page.is_hidden("#piece")
    assert len(calls) == len(script)
    todo.append(("reply", brief_text()))                     # then it works
    generate(page)
    page.wait_for_selector("#piece:not([hidden])")


def test_14_3_no_key_opens_settings_and_a_saved_key_works(studio):
    page, calls, todo = studio(key=None)
    generate(page)
    page.wait_for_selector("#settings[open]")
    assert "Add your AI key" in page.text_content("#status") and not calls
    page.fill("#ai-key", "typed-key")
    page.click("#settings-save")
    todo.append(("reply", brief_text()))
    generate(page)
    page.wait_for_selector("#piece:not([hidden])")
    assert calls[0].headers["authorization"] == "Bearer typed-key"


def test_14_3_double_click_one_request_in_flight(studio):
    page, calls, _ = studio(script=[("slow", brief_text()), ("reply", brief_text())])
    page.fill("#idea", IDEA)
    page.dblclick("#generate", force=True)
    page.click("#generate", force=True)
    page.wait_for_selector("#piece:not([hidden])")
    wait_done(page)
    assert len(calls) == 1


def test_14_3_other_providers_request_shape(studio):
    page, calls, todo = studio(key=None)
    page.evaluate("""() => localStorage.setItem('composer-studio.ai', JSON.stringify(
        {provider: 'anthropic', key: 'ak', model: '', base: ''}))""")
    todo.append(("status", 200, json.dumps({"content": [{"type": "text", "text": brief_text()}]})))
    generate(page)
    page.wait_for_selector("#piece:not([hidden])")
    r = calls[0]
    assert r.url == "https://api.anthropic.com/v1/messages" and r.headers["x-api-key"] == "ak"
    assert "reasoning_split" not in r.post_data_json and r.post_data_json["max_tokens"] == 8000
    assert r.post_data_json["system"].startswith("You turn") and r.post_data_json["messages"] == [{"role": "user", "content": IDEA}]


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
    assert page.text_content("#engine") == "Engine ready"


@pytest.mark.parametrize("args,msg", [(["--port", "70000"], "--port must be 0 to 65535"), (["--port", "x"], "invalid int")])
def test_14_5_fm1c_studio_bad_port(args, msg):
    r = run(APP / "fm1c", "studio", "--no-open", *args)
    assert r.returncode == 2 and msg in r.stderr and "Traceback" not in r.stderr


def test_14_5_fm1c_studio_port_in_use(studio_url):
    port = studio_url.rsplit(":", 1)[1].strip("/")
    r = run(APP / "fm1c", "studio", "--no-open", "--port", port)
    assert r.returncode == 2 and "cannot serve COMPOSER Studio on 127.0.0.1" in r.stderr


def test_14_3_deepseek_request_shape(studio):
    page, calls, todo = studio(key=None)
    page.evaluate("""() => localStorage.setItem('composer-studio.ai', JSON.stringify(
        {provider: 'deepseek', key: 'dk', model: '', base: ''}))""")
    todo.append(("reply", brief_text()))
    generate(page)
    page.wait_for_selector("#piece:not([hidden])")
    r = calls[0]
    assert r.url == "https://api.deepseek.com/chat/completions" and r.headers["authorization"] == "Bearer dk"
    sent = r.post_data_json
    assert sent["model"] == "deepseek-chat" and sent["max_tokens"] == 8000
    assert "max_completion_tokens" not in sent and "reasoning_split" not in sent


def test_14_1_open_a_midi_file(studio, studio_cli, tmp_path):
    page, calls, _ = studio()
    mid = tmp_path / "My Song.mid"
    assert run(studio_cli, "make", BRIEFS / "funk-classic-1.json", "--seed", 3, "--name", "x", "-o", tmp_path / "x.f1p",
               "--mid", mid).returncode == 0
    page.set_input_files("#midi-file", str(mid))
    page.wait_for_selector("#piece:not([hidden])")
    r = run(studio_cli, "convert", mid, "--name", "My Song", "-o", tmp_path / "y.f1p")
    with page.expect_download() as d:
        page.click("#dl-f1p")
    assert open(d.value.path(), "rb").read() == (tmp_path / "y.f1p").read_bytes() and not calls
    assert page.text_content("#piece-name") == "My Song" and page.is_hidden("#brief") and page.is_disabled("#again")
    assert "My Song.mid fitted to the FM-1" in page.text_content("#status")


@pytest.mark.parametrize("size,msg", [(8 * 1024 * 1024, "not a readable MIDI file"), (8 * 1024 * 1024 + 1, "larger than the 8 MB")])
def test_14_3_midi_file_size_limit(studio, tmp_path, size, msg):
    page, _, _ = studio()
    p = tmp_path / "big.mid"
    p.write_bytes(b"\0" * size)
    page.set_input_files("#midi-file", str(p))
    page.wait_for_function("m => document.querySelector('#status').textContent.includes(m)", arg=msg)
    assert page.get_attribute("#status", "class") == "error"


def test_14_3_settings_say_where_the_key_comes_from(studio):
    page, _, _ = studio()
    page.click("#settings-open")
    assert "local/ai.json" in page.text_content("#settings-source")
    assert page.get_attribute("#ai-model", "placeholder") == "MiniMax-M3.1-Flash"
    page.select_option("#ai-provider", "openai")
    assert page.get_attribute("#ai-model", "placeholder") == "model name"
    page.select_option("#ai-provider", "minimax")
    page.fill("#ai-key", "mine")
    page.fill("#ai-model", "my-model")
    page.fill("#ai-base", "https://example.test/v1")
    page.click("#settings-save")
    page.click("#settings-open")
    assert page.text_content("#settings-source") == "" and page.input_value("#ai-key") == "mine"
    assert page.input_value("#ai-model") == "my-model" and page.input_value("#ai-base") == "https://example.test/v1"
