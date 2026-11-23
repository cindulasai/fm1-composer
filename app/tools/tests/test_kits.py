"""US-21: SLOOP's 32 synthesised drum kits. The tables are what their generators write (SLOOP's gen_drumkits.py,
copied unchanged); on a drum channel PRESETS turns through the 35 kits, their names on the strip (host mode).
The kits' sound: tests/synth_test.c (21.1, 21.2, 21.4, 21.5)."""
import re
from conftest import APP, run_host, screen
from test_tables import regen
from test_ui import BTN, ENC, catalog, library_with


def test_21_1_drum_kit_tables_in_sync(tmp_path):
    assert regen("gen_drumkits.py", APP / "firmware/src/felucca_drumkits.h", tmp_path)
    assert regen("gen_drum_tables.py", APP / "firmware/src/drum_tables.h", tmp_path)


def test_21_1_kits_are_sloops_in_its_order(tmp_path):
    h = (APP / "firmware/src/felucca_drumkits.h").read_text()
    names = re.search(r"#define DS_KIT_NAME_LIST (.*)", h)[1]
    sloop = re.findall(r'"([^"]+)"', names)
    assert len(sloop) == 32 and "#define DS_NKITS 32u" in h
    assert catalog(tmp_path)[40:72] == sloop, "the 32 kits follow v4's 40 sounds, in SLOOP's order"


def test_21_3_presets_on_a_drum_channel_turns_through_35_kits(tmp_path):
    names = catalog(tmp_path)
    kits = names[:3] + names[40:72]
    assert kits[:3] == ["STD KIT", "TIGHT KIT", "BOOM KIT"]
    steps = [(50, f"btn {BTN['PLAY']}"), (100, f"enc {ENC['SELECT']} 2")]       # channel 3: drums (STD KIT)
    steps += [(200 + 40 * i, f"enc {ENC['PRESETS']} 1") for i in range(35)]
    steps += [(200 + 40 * 35, f"enc {ENC['PRESETS']} -1")]
    (log, wav), t = library_with(tmp_path, ["kits"], steps, 4000, name="kits35")
    y3 = 64 + 2 * 19
    seen = []
    for ms, y, txt in screen(log):
        if y == y3 and ms >= t + 100 and txt in names and (not seen or seen[-1] != txt):
            seen.append(txt)
    assert seen[0] == "STD KIT" and seen[1:36] == kits[1:] + ["STD KIT"], seen
    assert seen[36] == kits[-1], "and back: STD KIT turned down is the last kit"
    assert len(set(seen[:35])) == 35


def test_21_3_presets_elsewhere_skips_the_kits(tmp_path):
    names = catalog(tmp_path)
    assert names.index("SAW LEAD") == 3 and names[:3] == ["STD KIT", "TIGHT KIT", "BOOM KIT"]
    steps = [(50, f"btn {BTN['PLAY']}"), (200, f"enc {ENC['PRESETS']} -1"), (300, f"enc {ENC['PRESETS']} 1"),
             (400, f"enc {ENC['PRESETS']} 37")]                                   # channel 1: lead
    (log, _), t = library_with(tmp_path, ["lead"], steps, 700, name="lead1")
    seen = [txt for ms, y, txt in screen(log) if y == 64 and ms >= t + 200 and txt in names]
    assert seen[0] == names[-1], "SAW LEAD turned down: past the 3 kits, round to the last sound (FM6)"
    assert seen[1] == "SAW LEAD" and seen[2] == names[72], "36 sounds up, then past the 32 kits to FM6's first"
    assert not [x for x in seen if names.index(x) in range(40, 72) or names.index(x) < 3], seen
