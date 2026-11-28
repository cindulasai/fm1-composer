"""US-50: newer FM-1 units carry a UC25HQ80 flash (JEDEC 0xB36014) instead of the Puya P25Q80 (0x856014). The app and
its update loader must accept both (hal/fm1_flash_map.h FL_JEDEC_OK; tests/flash_id_test.c), and the firmware build
refuses to package an app or a loader whose code does not compare against both (tools/build.py need_flash_ids)."""
import importlib.util
from pathlib import Path

import pytest
from conftest import APP

spec = importlib.util.spec_from_file_location("fm1build", APP / "tools" / "build.py")
build = importlib.util.module_from_spec(spec)
spec.loader.exec_module(build)

BOTH = "200d70e: c2 ff 14 60 85 00  r2 = 8740884 <composer.c+0x856014 : 856014>\n" \
       "200d720: c2 ff 14 60 b3 00  r2 = 11755540 <composer.c+0xB36014 : b36014>\n"


def test_50_1_the_build_takes_code_that_knows_both_fm1_flash_chips():
    build.BOARD = "fm1"
    assert build.need_flash_ids(BOTH, "app")
    assert build.need_flash_ids(BOTH.upper(), "loader")


@pytest.mark.parametrize("dis,missing", [
    (BOTH.splitlines()[0], "0xB36014"),                                  # only the old chip: a newer unit saves to RAM
    (BOTH.splitlines()[1], "0x856014"),
    ("200d70e: r2 = 0 <x>", "0x856014, 0xB36014"),
    (BOTH.replace("856014", "8560140").replace("B36014", "B360141").replace("b36014", "b360141"), "0x856014, 0xB36014"),   # a longer number is not it
])
def test_50_2_the_build_refuses_code_missing_a_chip(dis, missing):
    build.BOARD = "fm1"
    for what in ("app", "loader"):
        with pytest.raises(SystemExit) as e:
            build.need_flash_ids(dis, what)
        assert str(e.value) == f"{what}: no check for flash chip id {missing} (FL_JEDEC_OK)"


def test_50_3_every_ci_built_app_on_this_computer_knows_both():
    found = sorted(Path(APP / "build").glob("**/composer.dis"))
    build.BOARD = "fm1"
    for dis in found:                                                     # (none in a fresh clone: CI checks its own)
        assert build.need_flash_ids(dis.read_text(errors="replace"), str(dis))
