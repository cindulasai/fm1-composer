"""US-4: the committed tables are exactly what their generators write (the chip has no FPU: tables are code)."""
import subprocess, sys
from conftest import APP


def regen(script, target, tmp_path):
    out = tmp_path / target.name
    r = subprocess.run([sys.executable, str(APP / "tools" / script), str(out)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return out.read_text() == target.read_text()


def test_4_2_synth_tables_in_sync(tmp_path):
    assert regen("gen_synth_tables.py", APP / "firmware/src/synth_tables.h", tmp_path)


def test_4_2_engine_tables_in_sync(tmp_path):
    assert regen("gen_engine_tables.py", APP / "engines/fe_tables.h", tmp_path)
