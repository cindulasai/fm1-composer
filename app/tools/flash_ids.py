# SPDX-License-Identifier: GPL-3.0-only
"""The flash chips each board's firmware must recognise (hal/fm1_flash_map.h FL_JEDEC_OK), checked in the built code.

The FM-1 ships with a Puya P25Q80 (JEDEC 0x856014); newer units carry a UC25HQ80 (0xB36014). The app, its update
loader too, must compare against both: else a newer unit saves to RAM only, refusing updates. tools/build.py calls
need_flash_ids on each disassembly it makes; a missing id stops the build."""
import re

FLASH_IDS = {"fm1": ("856014", "b36014"), "smk37": ("eb6014", "856014")}


def need_flash_ids(dis, what, board="fm1"):
    """whether `dis` (a disassembly) compares against every flash chip id of `board`; a missing one: SystemExit naming it"""
    missing = [i for i in FLASH_IDS[board] if not re.search(r"\b(0x)?" + i + r"\b", dis, re.I)]
    if missing:
        raise SystemExit(f"{what}: no check for flash chip id {', '.join('0x' + m.upper() for m in missing)} (FL_JEDEC_OK)")
    return True
