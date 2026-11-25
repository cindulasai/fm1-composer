"""US-32: the way in. The root README leads with the live Studio; COMPOSER's tile on fm1-emu's own page explains
COMPOSER and links to Studio (added by Studio's cable.js; fm1-emu's files unchanged)."""
import json, re
from pathlib import Path
from conftest import APP
from test_export import exported, needs_emu, studio_url  # noqa: F401  (fixtures: the static export, served)
from test_studio import browser  # noqa: F401

ROOT = APP.parent
LIVE = "https://cindulasai.github.io/fm1-composer/"


def tile_page(browser, url):
    page = browser.new_page()
    page.goto(url)
    page.wait_for_selector("li .head b", timeout=30000)
    return page


@needs_emu
def test_32_1_composer_tile_guides_to_studio(browser, studio_url):
    page = tile_page(browser, studio_url + "emu/?fw=composer")
    page.wait_for_selector("#composer-guide", timeout=15000)
    assert page.locator("#composer-guide").count() == 1
    card = page.locator("li", has=page.locator("#composer-guide"))
    assert card.locator(".head b").text_content() == "COMPOSER"                       # on COMPOSER's tile, only
    text = page.text_content("#composer-guide")
    for want in ("Its library starts empty here", "COMPOSER Studio", "Send to FM-1", "PLAY", "ARP", "EDIT", "full guide"):
        assert want in text, want
    # built as elements, not flattened to text: the big button, the Studio link and the full guide link; the controls bold
    links = page.locator("#composer-guide a")
    assert links.count() == 3, links.all_text_contents()
    assert [b.strip() for b in page.locator("#composer-guide b").all_text_contents()] == [
        "Start in three steps:", "Send to FM-1", "PLAY", "ARP", "EDIT", "LFO", "REC", "FX"]
    cta = page.locator("#composer-guide a.composer-cta")                               # the big button, first
    assert "open COMPOSER Studio" in cta.text_content() and cta.evaluate("a => a.href") == studio_url
    assert page.evaluate("() => document.querySelector('#composer-guide').firstElementChild.className") == "composer-cta"
    for a in page.locator("#composer-guide a", has_text="COMPOSER Studio").all():
        assert a.evaluate("a => a.href") == studio_url                                 # ../ on the same site
    link = cta
    with page.expect_popup() as pop:
        link.click()
    studio = pop.value
    studio.wait_for_selector("#send", timeout=30000)                                   # Studio itself opened
    assert studio.url == studio_url
    page.close(); studio.close()


def tiles(page):
    """{firmware name: its tile's text}, once the list has stopped growing"""
    n = -1
    for _ in range(20):
        page.wait_for_timeout(250)
        m = page.locator("li .head b").count()
        if m == n:
            break
        n = m
    return {li.locator(".head b").inner_text(): li.inner_text() for li in page.locator("li", has=page.locator(".head b")).all()}


@needs_emu
def test_32_1_other_tiles_unchanged_without_the_cable(browser, studio_url):
    """the page with Studio's cable.js and without it (blocked): every tile but COMPOSER's has the same text; COMPOSER's
    gains the guide"""
    page = tile_page(browser, studio_url + "emu/?fw=composer")
    page.wait_for_selector("#composer-guide", timeout=15000)
    cabled = tiles(page)
    page.close()
    bare_page = browser.new_page()
    bare_page.route("**/cable.js", lambda r: r.abort())
    bare_page.goto(studio_url + "emu/?fw=composer")
    bare_page.wait_for_selector("li .head b", timeout=30000)
    bare = tiles(bare_page)
    assert bare_page.locator("#composer-guide").count() == 0, "blocked: no cable, no guide"
    bare_page.close()
    assert set(cabled) == set(bare) and "COMPOSER" in cabled and len(cabled) > 1, (sorted(cabled), sorted(bare))
    changed = sorted(k for k in cabled if cabled[k] != bare[k])
    assert changed == ["COMPOSER"], changed
    assert "Start in three steps:" in cabled["COMPOSER"] and "Start in three steps:" not in bare["COMPOSER"]


@needs_emu
def test_32_1_no_guide_inside_studio(browser, studio_url):
    page = browser.new_page()
    page.goto(studio_url + "emu/?embed=1&fw=composer")
    page.wait_for_selector("#status", timeout=30000)
    page.wait_for_timeout(2000)
    assert page.locator("#composer-guide").count() == 0
    page.close()


def composer_about_ok(cat):
    """COMPOSER first and the default, its about saying what it does and where its pieces are made; others kept"""
    assert cat["default"] == "composer" and cat["firmwares"][0]["id"] == "composer"
    assert [f["id"] for f in cat["firmwares"]].count("composer") == 1
    about = cat["firmwares"][0]["about"]
    for want in ("8 parts", "sentence", "COMPOSER Studio", "this site's home page", "MIDI IN"):
        assert want in about, want


FAKE_CATALOG = {"default": "sloop", "firmwares": [{"id": "sloop", "name": "SLOOP", "about": "a looper"},
                                                   {"id": "composer", "name": "old", "about": "stale"},
                                                   {"id": "felucca", "name": "Felucca", "about": "synth"}]}


def test_32_2_catalog_about_says_what_and_where_unit(tmp_path):
    """fm1c's catalog rewrite (_composer_first) on a small catalog, no fm1-emu: the one function both `fm1c studio`
    (served: checked here through its handler) and the export (_export) use"""
    import functools, http.server, inspect, sys, threading, urllib.request
    sys.path.insert(0, str(APP / "tools"))
    from fm1c import cli
    cat = json.loads(cli._composer_first(json.loads(json.dumps(FAKE_CATALOG))))
    composer_about_ok(cat)
    assert [f for f in cat["firmwares"] if f["id"] != "composer"] == [FAKE_CATALOG["firmwares"][0], FAKE_CATALOG["firmwares"][2]]
    assert "_composer_first(" in inspect.getsource(cli._export) and "_composer_first(" in inspect.getsource(cli._StudioHandler)
    (tmp_path / "emu").mkdir()
    (tmp_path / "emu" / "firmwares.json").write_text(json.dumps(FAKE_CATALOG))
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(cli._StudioHandler, directory=str(tmp_path)))
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    try:
        with urllib.request.urlopen(f"http://127.0.0.1:{srv.server_port}/emu/firmwares.json", timeout=10) as r:
            composer_about_ok(json.loads(r.read()))
    finally:
        srv.shutdown(); srv.server_close()


@needs_emu
def test_32_2_catalog_about_says_what_and_where(exported):
    composer_about_ok(json.loads((exported / "emu" / "firmwares.json").read_text()))


def test_32_3_readme_leads_with_the_live_studio():
    text = (ROOT / "README.md").read_text()
    lead = text.split("\n## ")[0]
    assert LIVE in lead.split("\n", 3)[2]                                              # the line under the title
    assert f"[![" in lead and f"](docs/studio.png)]({LIVE})" in lead                    # screenshot links to it
    assert "## Try it in three steps" in text and "Send to FM-1" in text and "emulator only" in text
    for target in re.findall(r"\]\((?!https?://)([^)#]+)", text):
        assert (ROOT / target).exists(), target
    png = (ROOT / "docs" / "studio.png").read_bytes()
    assert png[:8] == b"\x89PNG\r\n\x1a\n" and len(png) < 400_000
