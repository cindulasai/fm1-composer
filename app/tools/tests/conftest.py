import subprocess, wave, struct
from pathlib import Path
import pytest

APP = Path(__file__).resolve().parents[2]


def run_host(script, *cflags, env=None):
    """Run COMPOSER in host mode (a script name, or the path of a .txt); return (log text, wav path)."""
    import os
    r = subprocess.run(["sh", str(APP / "tests/host_e2e.sh"), str(script), *cflags], capture_output=True, text=True,
                       env={**os.environ, **(env or {})})
    assert r.returncode == 0, r.stderr
    name = Path(str(script)).stem
    return (APP / f"build/host/{name}.log").read_text(), APP / f"build/host/{name}.wav"


def screen(log):
    """The SCREEN lines of a host log: [(ms, y, text)]."""
    import re
    return [(ms, y, txt) for ms, y, _, _, txt in screen_full(log)]


def screen_full(log):
    """[(ms, y, x, colour RGB565, text)]"""
    import re
    return [(int(m[1]), int(m[2]), int(m[3]), int(m[4], 16), m[5])
            for m in re.finditer(r"^ *(\d+) ms  SCREEN (\d+) (\d+) ([0-9A-F]{4}) ?(.*)$", log, re.M)]


def wav_frames(path):
    with wave.open(str(path)) as w:
        assert w.getframerate() == 44100 and w.getnchannels() == 2 and w.getsampwidth() == 2
        data = w.readframes(w.getnframes())
    return struct.unpack(f"<{len(data)//2}h", data)


@pytest.fixture
def host():
    return run_host
