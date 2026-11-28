"""US-40: Studio looks like one app. text2midi's window (left), Studio's bar and the FM-1's surround (right) share one
palette (app/studio/web/theme.css, defined once); the left panel's scrollbars are slim and rounded; the controls are
today's, in today's order; tiny and wide windows keep both panels in view. Real served Studio, headless Chromium with
its scrollbars drawn (Playwright hides them by default)."""
import io
import os, re
import pytest
from pathlib import Path
from PIL import Image, ImageChops, ImageStat
from conftest import APP
from test_studio import studio, studio_url  # noqa: F401  (fixtures)

ROOT = APP.parent
WEB = APP / "studio" / "web"
REF = Path(__file__).parent / "look"                      # the scrolled panel's scrollbar, as it should look
TOKENS = ["--c-bg", "--c-surface", "--c-surface-2", "--c-line", "--c-text", "--c-muted", "--c-accent", "--c-accent-ink",
          "--c-thumb", "--c-radius", "--c-radius-sm", "--c-font"]
LIGHT_BG = (243, 241, 236)                                # --c-bg, light


@pytest.fixture(scope="module")
def browser(pw_browser):
    """a second Chromium from the same Playwright, scrollbars shown (they are what 40.2 is about)"""
    b = pw_browser.browser_type.launch(ignore_default_args=["--hide-scrollbars"])
    yield b
    b.close()


def t2m(page):
    return next(f for f in page.frames if f.url.rstrip("/").endswith("/t2m"))


def open_piece(page, title="funk.classic suite 1"):
    f = t2m(page)
    f.click(f"text={title}")
    f.locator("h2", has_text=title).wait_for()
    return f


def look(page, scheme, w=1440, h=900):
    page.set_viewport_size({"width": w, "height": h})
    page.emulate_media(color_scheme=scheme)
    page.wait_for_timeout(300)


TOKENS_JS = "(names) => names.map((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim())"
STYLE_JS = """([sel, props]) => { const e = document.querySelector(sel), s = getComputedStyle(e);
    return props.map((p) => s.getPropertyValue(p)); }"""


# ---- 40.1: one palette ----
def test_40_1_one_palette_on_both_sides_defined_once(studio):
    page, _, _ = studio()
    f = open_piece(page)
    seen = {}
    for scheme in ("light", "dark"):
        look(page, scheme)
        top, left = page.evaluate(TOKENS_JS, TOKENS), f.evaluate(TOKENS_JS, TOKENS)
        assert all(top) and top == left, (scheme, top, left)                  # the same custom properties, both frames
        seen[scheme] = top
        st = lambda fr, sel, *p: fr.evaluate(STYLE_JS, [sel, list(p)])        # noqa: E731
        desk = st(page, "body", "background-color")
        assert st(page, "header", "background-color") in (desk, ["rgba(0, 0, 0, 0)"])    # the bar sits on the desk
        surface = st(page, "#t2m", "background-color")                                    # the left panel's surround
        assert st(f, "body", "background-color") == surface == st(f, "aside", "background-color") != desk
        assert st(page, "body", "color") == st(f, "body", "color")                        # text
        assert st(page, "body", "font-family") == st(f, "body", "font-family")            # one font stack
        assert st(page, "body", "font-size") == st(f, "body", "font-size") == ["14px"]    # and type scale
        accent = st(page, "#send", "background-color", "color")
        assert accent == st(f, "button.primary", "background-color", "color")            # Send to FM-1 = Make it / Play
        assert st(f, "li button.active", "background-color", "color") == accent           # the chosen example
        frame = ["border-top-left-radius", "border-top-color", "border-top-width", "box-shadow"]
        assert st(page, "#t2m", *frame) == st(page, "#fm1", *frame)                       # both panels framed alike
        assert st(page, "#fm1", "background-color") == ["rgb(24, 25, 27)"]                # fm1-emu's own --room
    assert seen["light"][:6] != seen["dark"][:6]                                          # two themes, one palette
    # defined once: only theme.css gives colours; text2midi's tokens map onto it; its own files stay unchanged (16.1)
    hexes = lambda p: re.findall(r"#[0-9a-fA-F]{3,8}\b", re.sub(r"/\*.*?\*/", "", p.read_text(), flags=re.S))  # noqa: E731
    assert not hexes(WEB / "style.css") and not hexes(APP / "studio" / "t2m-ui" / "studio" / "theme.css")
    assert re.findall(r"--c-bg:", (WEB / "theme.css").read_text()) and \
        '<link rel="stylesheet" href="theme.css">' in (WEB / "index.html").read_text()
    assert '@import "../../web/theme.css";' in (APP / "studio" / "t2m-ui" / "studio" / "theme.css").read_text()


# ---- 40.2: slim scrollbars ----
SCROLLERS_JS = """() => [...document.querySelectorAll('*')].filter((e) => {
    const s = getComputedStyle(e), o = s.overflowX + s.overflowY;
    return /auto|scroll/.test(o) && (e.scrollHeight > e.clientHeight + 1 || e.scrollWidth > e.clientWidth + 1); })
  .map((e) => { const s = getComputedStyle(e), b = (p) => parseFloat(s.getPropertyValue(p));
    return { name: e.tagName.toLowerCase() + (e.className ? '.' + String(e.className).split(' ')[0] : ''),
             v: e.scrollHeight > e.clientHeight + 1 ? e.offsetWidth - e.clientWidth - b('border-left-width') - b('border-right-width') : 0,
             h: e.scrollWidth > e.clientWidth + 1 ? e.offsetHeight - e.clientHeight - b('border-top-width') - b('border-bottom-width') : 0 }; })"""
RULES_JS = """() => { const out = []; const walk = (rules) => { for (const r of rules) {
    if (r.cssRules && !r.selectorText) { out.push('@' + (r.conditionText || '') + '{'); walk(r.cssRules); out.push('}'); }
    else out.push(r.cssText); } };
  for (const sh of document.styleSheets) walk(sh.cssRules); return out.join('\\n'); }"""


def strip(page, f, scheme):
    """the left panel's (examples) scrollbar, scrolled part way: 8 px wide, 360 px tall, as on screen"""
    look(page, scheme)
    f.evaluate("() => { const a = document.querySelector('aside'); a.scrollTop = 300; }")
    page.mouse.move(1000, 10)                                     # (no hover on the thumb)
    page.wait_for_timeout(300)
    box = page.locator("#t2m").bounding_box()
    a = f.evaluate("() => { const r = document.querySelector('aside').getBoundingClientRect(); return [r.right, r.top]; }")
    x, y = box["x"] + 1 + a[0] - 1 - 8, box["y"] + 1 + a[1]       # iframe border, the aside's own right border
    return Image.open(io.BytesIO(page.screenshot(clip={"x": x, "y": y, "width": 8, "height": 360}))).convert("RGB")


@pytest.mark.parametrize("scheme", ["light", "dark"])
def test_40_2_slim_rounded_scrollbars_in_every_scrolling_area_of_the_left_panel(studio, scheme):
    page, _, _ = studio()
    f = open_piece(page)
    look(page, scheme)
    areas = f.evaluate(SCROLLERS_JS)
    names = {a["name"] for a in areas}
    assert any(n.split(".")[0] == "aside" for n in names) and any(n.startswith("div.host") for n in names), areas  # examples, arrangement
    for a in areas:
        assert 0 < a["v"] <= 8 or a["v"] == 0 and a["h"] > 0, a                            # slim, not 15 px
        assert a["h"] <= 8, a
    main = f.evaluate("() => { const m = document.querySelector('main'); return [m.scrollWidth, m.clientWidth]; }")
    assert main[0] <= main[1]                                                              # no stray sideways bar
    css = f.evaluate(RULES_JS)
    assert re.search(r"::-webkit-scrollbar \{[^}]*width: 8px;[^}]*height: 8px", css)
    assert re.search(r"::-webkit-scrollbar-thumb \{[^}]*border-radius: 8px", css)          # rounded thumb
    assert re.search(r"::-webkit-scrollbar-track \{[^}]*background(-color)?: (transparent|none|rgba\(0, 0, 0, 0\))", css)    # the panel shows through
    assert re.search(r"@not selector\(::-webkit-scrollbar\)\{\n\* \{ scrollbar-width: thin; scrollbar-color: var\(--c-thumb\) transparent", css)
    got = strip(page, f, scheme)
    ref = REF / f"aside-scrollbar-{scheme}.png"
    if os.environ.get("FM1_LOOK_UPDATE") == "1":                                         # re-store the look on purpose
        REF.mkdir(exist_ok=True); got.save(ref)
    want = Image.open(ref).convert("RGB")
    assert got.size == want.size
    diff = ImageStat.Stat(ImageChops.difference(got, want)).mean
    assert max(diff) <= 6, (scheme, diff)                                                  # the stored look, ± AA
    px = got.load()
    col = [px[3, yy] for yy in range(got.size[1])]
    thumb = [i for i, c in enumerate(col) if c != col[0] and c != col[-1]] or [i for i, c in enumerate(col) if c != col[-1]]
    assert thumb and px[0, thumb[0]] != px[3, thumb[0] + 4]                                # corner ≠ middle: rounded


# ---- 40.3: simplicity kept ----
CONTROLS_JS = """() => [...document.querySelectorAll('button, input, select, textarea, a[href], summary')].filter((e) => {
    const r = e.getBoundingClientRect(), s = getComputedStyle(e);
    return r.width > 0 && r.height > 0 && s.visibility !== 'hidden' && !e.closest('[hidden]'); })
  .map((e) => e.tagName.toLowerCase() + ':' + (e.getAttribute('aria-label') || e.getAttribute('placeholder') || e.textContent)
    .trim().replace(/\\s+/g, ' ').slice(0, 40))"""
EXAMPLES = [f"button:{s} suite {i} {s}"[:47] for s in
            ("cinematic.orchestral", "electronic.house", "electronic.techno", "funk.classic", "hiphop.boombap",
             "hiphop.lofi", "hiphop.trap", "jazz.swing", "latin.bossa", "pop.generic", "reggae.roots", "rock.classic")
            for i in (1, 2, 3)]
# captured from main before US-40 (the same served Studio, 1440 x 900)
TOP = ["button:Send to FM-1 →", "a:text2midi", "a:fm1-emu", "a:The Firmware Hub"]   # (+ where hardware installs come from)
GALLERY = ["button:AI settings", "textarea:Describe the music", "button:Make it", "button:Change", "input:Filter examples"] + EXAMPLES
PIECE = GALLERY + ["button:Play", "button:Undo", "button:Redo", "button:New take", "select:How quarter-tones are written",
                   "button:Save", "button:Export MIDI", "button:Mute lead", "button:Solo lead", "button:Mute chords",
                   "button:Solo chords", "button:Mute bass", "button:Solo bass", "button:Mute drums", "button:Solo drums",
                   "input:", "select:lead (keys.rhodes)chords (guitar.clean)b", "button:Try 3 takes"]


def test_40_3_same_controls_labels_and_order_as_before(studio):
    page, _, _ = studio()
    look(page, "light")
    f = t2m(page)
    assert page.evaluate(CONTROLS_JS) == TOP
    assert f.evaluate(CONTROLS_JS) == GALLERY
    open_piece(page)
    page.wait_for_timeout(300)
    assert f.evaluate(CONTROLS_JS) == PIECE


# ---- 40.4: tiny and wide windows; the README screenshot ----
@pytest.mark.parametrize("w,h", [(360, 640), (2560, 1440)])
def test_40_4_tiny_and_wide_no_sideways_scroll_no_overlap(studio, w, h):
    page, _, _ = studio()
    open_piece(page)
    for scheme in ("light", "dark"):
        look(page, scheme, w, h)
        doc = page.evaluate("() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]")
        assert doc[0] <= doc[1], (w, doc)
        boxes = {s: page.locator(s).bounding_box() for s in ("header", "#t2m", "#fm1", "footer")}
        for s, b in boxes.items():
            assert b and b["width"] > 0 and b["height"] > 100 or s in ("header", "footer"), (s, b)
            assert b["x"] >= 0 and b["x"] + b["width"] <= w + 0.5, (s, b)
        names = list(boxes)
        for i, a in enumerate(names):
            for c in names[i + 1:]:
                A, B = boxes[a], boxes[c]
                apart = (A["x"] + A["width"] <= B["x"] + 0.5 or B["x"] + B["width"] <= A["x"] + 0.5 or
                         A["y"] + A["height"] <= B["y"] + 0.5 or B["y"] + B["height"] <= A["y"] + 0.5)
                assert apart, (w, a, c, A, B)
        side = boxes["#t2m"]["y"] == boxes["#fm1"]["y"]
        assert side if w > 900 else boxes["#fm1"]["y"] >= boxes["#t2m"]["y"] + boxes["#t2m"]["height"]   # side by side / stacked


@pytest.mark.parametrize("png", ["docs/studio.png", "app/docs/studio.png"])
def test_40_4_readme_screenshot_retaken_in_the_new_look(png):
    data = (ROOT / png).read_bytes()
    assert data[:8] == b"\x89PNG\r\n\x1a\n" and len(data) < 400_000
    im = Image.open(io.BytesIO(data)).convert("RGB")
    assert all(abs(a - b) <= 2 for a, b in zip(im.getpixel((2, 2)), LIGHT_BG))           # Studio's desk, light



def test_40_3_the_page_says_the_firmware_is_in_the_emulator_and_where_hardware_installs_come_from(studio):
    page, _, _ = studio()
    look(page, "light")
    note = page.locator("footer #install")
    assert note.is_visible()
    text = note.inner_text()
    assert "already loaded in this emulator" in text and "not been run on hardware yet" in text
    assert note.locator("a").get_attribute("href") == "https://thefirmwarehub.com/"
    assert note.locator("a").get_attribute("rel") == "noopener"
