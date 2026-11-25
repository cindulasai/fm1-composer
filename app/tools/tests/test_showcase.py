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


@needs_emu
def test_32_1_no_guide_inside_studio(browser, studio_url):
    page = browser.new_page()
    page.goto(studio_url + "emu/?embed=1&fw=composer")
    page.wait_for_selector("#status", timeout=30000)
    page.wait_for_timeout(2000)
    assert page.locator("#composer-guide").count() == 0
    page.close()


@needs_emu
def test_32_2_catalog_about_says_what_and_where(exported):
    cat = json.loads((exported / "emu" / "firmwares.json").read_text())
    about = next(f for f in cat["firmwares"] if f["id"] == "composer")["about"]
    assert "COMPOSER Studio" in about and "sentence" in about and "8 parts" in about


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
