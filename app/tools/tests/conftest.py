import subprocess, wave, struct
from pathlib import Path
import pytest

APP = Path(__file__).resolve().parents[2]


def run_host(script, *cflags):
    """Run COMPOSER in host mode; return (log text, wav path)."""
    r = subprocess.run(["sh", str(APP / "tests/host_e2e.sh"), script, *cflags], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return (APP / f"build/host/{script}.log").read_text(), APP / f"build/host/{script}.wav"


def screen(log):
    """The SCREEN lines of a host log: [(ms, y, text)]."""
    out = []
    for line in log.splitlines():
        parts = line.split(None, 4)
        if len(parts) == 5 and parts[2] == "SCREEN":
            out.append((int(parts[0]), int(parts[3]), parts[4]))
    return out


def wav_frames(path):
    with wave.open(str(path)) as w:
        assert w.getframerate() == 44100 and w.getnchannels() == 2 and w.getsampwidth() == 2
        data = w.readframes(w.getnframes())
    return struct.unpack(f"<{len(data)//2}h", data)


@pytest.fixture
def host():
    return run_host
