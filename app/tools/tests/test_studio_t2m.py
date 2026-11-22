"""US-16: the left side of Studio is text2midi's own window (its Svelte code copied unchanged, built for the
browser), its commands served by t2m-session and t2m-audio compiled to WebAssembly."""
import hashlib, json, re, subprocess
import pytest
from conftest import APP
from test_studio import IDEA, body, brief_text, browser, studio, studio_url  # noqa: F401  (fixtures)

UI = APP / "studio" / "t2m-ui"
BRIEFS = APP / "studio" / "engine" / "suite" / "briefs"
T2M = APP.parent.parent / "text2midi_v2"


def frame(page):
    return next(f for f in page.frames if f.url.rstrip("/").endswith("/t2m"))


def blob(data):
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def test_16_1_text2midis_window_copied_unchanged_and_its_gallery(studio):
    listed = dict(ln.split(None, 1)[::-1] for ln in (UI / "PROVENANCE.txt").read_text().splitlines()
                  if re.match(r"^[0-9a-f]{40} ", ln))
    commit = re.search(r"commit ([0-9a-f]{40})", (UI / "PROVENANCE.txt").read_text()).group(1)
    assert len(listed) >= 15 and "src/App.svelte" in listed and "src/lib/api.ts" in listed
    for path, sha in listed.items():
        assert blob((UI / path).read_bytes()) == sha, f"{path} differs from text2midi's"
    if (T2M / ".git").exists():                                    # and against text2midi's own history
        for path, sha in listed.items():
            r = subprocess.run(["git", "-C", str(T2M), "rev-parse", f"{commit}:app/{path}"], capture_output=True, text=True)
            assert r.stdout.strip() == sha, path
    page, _, _ = studio()
    f = frame(page)
    titles = [json.loads(p.read_text())["identity"]["title"] for p in BRIEFS.glob("*.json")]
    assert len(f.evaluate("() => window.studioDesk.call('gallery')")) == len(titles)
    for t in titles[:5]:
        assert f.locator(f"text={t}").count() >= 1
    assert "Using MiniMax · MiniMax-M3.1-Flash" in f.inner_text("body")       # the key in local/ai.json, no Settings


def make(f, text=IDEA):
    f.fill("textarea", text)
    f.click("text=Make it")


def test_16_2_make_it_asks_the_ai_and_opens_the_piece(studio):
    page, calls, _ = studio(script=[("reply", brief_text())])
    f = frame(page)
    make(f)
    f.locator("h2", has_text="Dusty Rain").wait_for(timeout=30000)
    req = calls[0].post_data_json
    assert calls[0].url == "https://api.minimax.io/v1/chat/completions"
    assert req["messages"][0]["role"] == "system" and req["messages"][0]["content"].startswith("You turn")
    assert req["messages"][1] == {"role": "user", "content": IDEA} and req["model"] == "MiniMax-M3.1-Flash"
    assert req["max_completion_tokens"] == 8000 and req["reasoning_split"] is True and "max_tokens" not in req
    v = f.evaluate("() => window.studioDesk.call('view')")
    assert v["title"] == "Dusty Rain" and len(v["tracks"]) >= 3
    roll = re.search(r"Piano roll.*?(\d+) notes", f.inner_text("body"), re.S)
    assert roll and int(roll.group(1)) > 0                                # text2midi's piano roll, with notes in it


def test_16_3_takes_lock_undo_redo_reseed_as_in_text2midi(studio):
    page, _, _ = studio()
    f = frame(page)
    f.click("text=funk.classic suite 1")
    f.locator("h2", has_text="funk.classic suite 1").wait_for()
    view = lambda: f.evaluate("() => window.studioDesk.call('view')")              # noqa: E731
    a = view()
    f.click("button:has-text('New take')")
    f.wait_for_function("s => window.studioDesk.call('view').seed !== s", arg=a["seed"])
    b = view()
    assert b["tracks"] != a["tracks"] and b["can_undo"]
    f.click("button:has-text('Undo')")
    f.wait_for_function("s => window.studioDesk.call('view').seed === s", arg=a["seed"])
    assert view()["tracks"] == a["tracks"]
    f.click("button:has-text('Redo')")
    f.wait_for_function("s => window.studioDesk.call('view').seed === s", arg=b["seed"])
    lead = b["tracks"][0]["id"]
    f.evaluate("""() => { const c = document.querySelector("canvas[aria-label^='Arrangement']"), r = c.getBoundingClientRect();
        c.dispatchEvent(new MouseEvent('click', { bubbles: true, clientX: r.left + 40, clientY: r.top + 26 + 32 })); }""")
    # (text2midi's own handler: a click on a part's name, lane 1 left of 168 px, locks it)
    f.wait_for_function("t => window.studioDesk.call('view').tracks.find((x) => x.id === t).locked", arg=lead)
    c = view()
    f.click("button:has-text('New take')")                           # a new take keeps the locked part as it is
    f.wait_for_function("s => window.studioDesk.call('view').seed !== s", arg=c["seed"])
    d = view()
    track = lambda v, i: next(t for t in v["tracks"] if t["id"] == i)                 # noqa: E731
    assert track(d, lead)["notes"] == track(c, lead)["notes"]
    assert any(track(d, t["id"])["notes"] != t["notes"] for t in c["tracks"] if t["id"] != lead)
    other = c["tracks"][1]["id"]
    takes = f.evaluate("t => window.studioInvoke('takes', { track: t, section: null, count: 3 })", other)
    assert len(takes) == 3 and len({json.dumps(x["notes"]) for x in takes}) > 1
    kept = f.evaluate("t => window.studioInvoke('keep_take', { track: t, section: null, take: 2 })", other)
    assert track(kept, other)["notes"] == takes[1]["notes"] and kept["can_undo"]          # take 2 = the second (1-based)


def test_16_4_preview_plays_and_stops(studio):
    page, _, _ = studio()
    f = frame(page)
    f.click("text=funk.classic suite 1")
    f.locator("h2", has_text="funk.classic suite 1").wait_for()
    f.click("button:has-text('Play')")
    f.wait_for_function("() => window.studioDesk.level() > 0.01", timeout=10000)
    p1 = f.evaluate("() => window.studioDesk.call('transport')")
    f.wait_for_timeout(500)
    p2 = f.evaluate("() => window.studioDesk.call('transport')")
    assert p1["playing"] and p2["position"] > p1["position"]
    f.click("button:has-text('Stop')")
    f.wait_for_function("() => window.studioDesk.level() < 0.0001", timeout=2000)
    assert not f.evaluate("() => window.studioDesk.call('transport')")["playing"]


@pytest.mark.parametrize("case,script,note", [
    ("401", [("status", 401, '{"error":{"message":"bad key"}}')],
     "The language model did not help (MiniMax refused the key (401)), so the plain keyword reader was used."),
    ("garbage", [("reply", "no json here"), ("reply", "still none")], "the answer could not be used"),
    ("down", [("abort",)], "The language model did not help (could not reach MiniMax)"),
    ("500", [("status", 500, '{"error":{"message":"overloaded"}}')], "(MiniMax answered 500: overloaded)"),
    ("minimax-error", [("status", 502, '{"base_resp":{"status_msg":"quota used up"}}')], "(MiniMax answered 502: quota used up)"),
])
def test_16_5_ai_failures_fall_back_to_text2midis_offline_reader(studio, case, script, note):
    page, calls, _ = studio(script=script)
    f = frame(page)
    make(f)
    f.locator(f"text={note}").first.wait_for(timeout=30000)
    assert len(calls) == len(script)
    assert f.evaluate("() => window.studioDesk.call('view').title")                      # a piece all the same


def test_16_5_no_key_says_so_and_uses_the_offline_reader(studio):
    page, calls, _ = studio(key=None)
    f = frame(page)
    assert "Using" not in f.inner_text("body") or "MiniMax" not in f.inner_text("body")
    make(f)
    f.locator("text=No AI model is set up").first.wait_for(timeout=30000)
    assert not calls


def test_16_5_projects_round_trip_and_reload_mid_flow(studio):
    page, calls, todo = studio(script=[("hang",)])
    f = frame(page)
    f.click("text=funk.classic suite 2")
    f.locator("h2", has_text="funk.classic suite 2").wait_for()
    name = f.evaluate("() => window.studioInvoke('save_project')")
    assert name == "funk-classic-suite-2-1.t2m.json"
    assert f.evaluate("() => window.studioInvoke('saved_projects')")[0] == name
    with pytest.raises(Exception, match="not a saved project name"):
        f.evaluate("() => window.studioInvoke('open_project', { name: '../x' })")
    make(f)                                                        # the AI never answers: reload in the middle
    page.reload()
    page.frame_locator("#t2m").locator("text=funk.classic suite 1").wait_for(timeout=30000)
    f = frame(page)
    assert f.evaluate("n => window.studioInvoke('open_project', { name: n })", name)["title"] == "funk.classic suite 2"


def opened(page, title="funk.classic suite 1"):
    f = frame(page)
    f.click(f"text={title}")
    f.locator("h2", has_text=title).wait_for()
    return f


def test_16_3_export_midi_downloads_every_file(studio):
    page, _, _ = studio()
    f = opened(page)
    with page.expect_download() as d:
        got = f.evaluate("() => window.studioInvoke('export_midi', { profile: 'approx' })")
    assert got and all(re.fullmatch(r"Downloads/[^/]+\.mid", p) for p in got), got
    assert d.value.suggested_filename == got[0].split("/", 1)[1]


def test_16_4_mute_silences_the_preview(studio):
    page, _, _ = studio()
    f = opened(page)
    f.click("button:has-text('Play')")
    f.wait_for_function("() => window.studioDesk.level() > 0.01", timeout=10000)
    f.evaluate("() => window.studioInvoke('mix', { muted: [], soloed: [], looping: false })")   # nothing muted: heard
    f.wait_for_timeout(300)
    f.wait_for_function("() => window.studioDesk.level() > 0.01", timeout=5000)
    f.evaluate("() => window.studioInvoke('mix', { muted: [true, true, true, true, true, true, true, true], soloed: [], looping: false })")
    f.wait_for_function("() => window.studioDesk.level() < 0.0005", timeout=5000)


def test_16_5_settings_offer_text2midis_providers_and_check_the_key(studio):
    page, _, _ = studio()
    f = frame(page)
    info = f.evaluate("() => window.studioInvoke('ai_settings')")
    assert [p["id"] for p in info["providers"]] == ["anthropic", "openai", "gemini", "deepseek", "groq", "xai", "mistral",
                                                   "openrouter", "minimax", "ollama", "custom"]
    by = {p["id"]: p for p in info["providers"]}
    assert by["custom"]["needs_url"] and not by["minimax"]["needs_url"] and not by["ollama"]["needs_key"]
    assert by["openai"]["needs_key"] and by["anthropic"]["needs_key"] and by["minimax"]["needs_key"]
    assert by["minimax"]["models"][0] == "MiniMax-M3.1-Flash" and "base" not in by["minimax"]
    with pytest.raises(Exception, match="spaces or control characters"):
        f.evaluate("() => window.studioInvoke('save_ai_settings', { settings: { provider: 'deepseek', model: 'deepseek-chat', base_url: null }, key: 'ab cd' })")
    with pytest.raises(Exception, match="unknown provider"):
        f.evaluate("() => window.studioInvoke('save_ai_settings', { settings: { provider: 'nope', model: 'x', base_url: null }, key: null })")


def test_16_2_anthropic_request_from_the_browser(studio):
    b = json.loads(brief_text())
    reply = json.dumps({"content": [{"type": "text", "text": json.dumps(b)}]})
    page, calls, _ = studio(routes={"https://api.anthropic.com/**": lambda r: (calls.append(r.request),
                            r.fulfill(status=200, content_type="application/json", body=reply))})
    f = frame(page)
    info = f.evaluate("() => window.studioInvoke('save_ai_settings', { settings: { provider: 'anthropic', model: 'claude-haiku-4-5', base_url: null }, key: 'sk-ant-test' })")
    assert info["active"] == "Anthropic (Claude) · claude-haiku-4-5" and info["has_key"]
    make(f)
    f.locator("h2", has_text="Dusty Rain").wait_for(timeout=30000)
    r = calls[-1]
    assert r.url == "https://api.anthropic.com/v1/messages"
    h = r.headers
    assert h["x-api-key"] == "sk-ant-test" and h["anthropic-version"] == "2023-06-01"
    assert h["anthropic-dangerous-direct-browser-access"] == "true"
    assert r.post_data_json["system"].startswith("You turn") and r.post_data_json["messages"] == [{"role": "user", "content": IDEA}]


def test_16_5_test_ai_without_a_model_and_deepseeks_request(studio):
    page, calls, _ = studio(key=None)
    f = frame(page)
    with pytest.raises(Exception, match="Choose a provider and model, and save a key, first."):
        f.evaluate("() => window.studioInvoke('test_ai')")
    f.evaluate("() => window.studioInvoke('save_ai_settings', { settings: { provider: 'deepseek', model: 'deepseek-chat', base_url: null }, key: 'dk-test' })")
    with pytest.raises(Exception):
        f.evaluate("() => window.studioInvoke('test_ai')")             # the fake AI answers 500 to anything unscripted
    r = calls[-1]
    assert r.url == "https://api.deepseek.com/chat/completions" and r.headers["authorization"] == "Bearer dk-test"
    assert r.post_data_json["max_tokens"] == 8000 and "reasoning_split" not in r.post_data_json
