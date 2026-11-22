"""fm1c emu: COMPOSER running in fm1-emu (joakimeriksson/fm1-emu, MIT) with its USB port on this computer, so
`fm1c push` reaches it like a real FM-1; its library is kept in a flash file between runs."""
from __future__ import annotations

import os
import subprocess
from pathlib import Path

APP = Path(__file__).resolve().parents[2]
FLASH = Path.home() / ".fm1c" / "emu-flash.bin"


class EmuError(Exception):
    pass


def locate(fwsc: str | None = None, live: str | None = None) -> tuple[str, str]:
    """(fm1-live, composer.fwsc); EmuError says how to get the one that is missing."""
    live = live or os.environ.get("FM1C_LIVE") or str(APP.parent / "vendor/fm1-emu/fm1-live")
    fwsc = fwsc or os.environ.get("FM1C_FWSC") or str(APP / "build/composer.fwsc")
    if not (os.path.isfile(live) and os.access(live, os.X_OK)):
        raise EmuError(f"fm1-emu not found at {live} (install it: sh app/tools/get_emu.sh)")
    if not os.path.isfile(fwsc):
        raise EmuError(f"no COMPOSER firmware at {fwsc} (CI builds it: gh run download -n composer-fwsc -D app/build)")
    return live, fwsc


def run(fwsc: str | None, flash: str | None, live: str | None = None) -> int:
    live, fwsc = locate(fwsc, live)
    flash = os.path.abspath(flash or FLASH)
    os.makedirs(os.path.dirname(flash), exist_ok=True)
    print(f"COMPOSER in fm1-emu (library kept in {flash}); push with: fm1c push PIECE.f1p   (Ctrl-C quits)", flush=True)
    child = subprocess.Popen([live, "--fwsc", fwsc, "--usb", "--flash", flash])
    try:
        return child.wait()
    except KeyboardInterrupt:                    # Ctrl-C: the emulator gets it too; a normal way to quit
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
        return 0
