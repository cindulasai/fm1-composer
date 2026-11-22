import re, subprocess
from conftest import APP, run_host, screen, wav_frames


def test_1_1_boot_screen_shows_name_and_empty_library():
    log, _ = run_host("boot")
    texts = [t for _, _, t in screen(log)]
    assert "COMPOSER" in texts
    assert "LIBRARY EMPTY" in texts


def test_1_2_boot_audio_is_silent_stereo_44k():
    _, wav = run_host("boot")
    s = wav_frames(wav)
    assert len(s) > 0
    assert max(abs(v) for v in s) == 0


def test_1_3_identity_is_our_own():
    src = (APP / "firmware/src/composer.c").read_text()
    assert '#define APP_NAME "COMPOSER"' in src
    build = (APP / "tools/build.py").read_text()
    assert re.search(r'^PRODUCT = "FM-1_95\d"', build, re.M)
    assert '"fm1": "FM-1_95"' in build
    store = (APP / "firmware/src/store.h").read_text()
    m = re.search(r"#define STORE_MAGIC (0x[0-9A-F]+)u", store)
    assert m and m.group(1) != "0x59474E44"


def test_1_4_unknown_script_is_refused_cleanly():
    before = sorted(p.name for p in (APP / "build/host").glob("*")) if (APP / "build/host").exists() else []
    r = subprocess.run(["sh", str(APP / "tests/host_e2e.sh"), "no-such-script"], capture_output=True, text=True)
    assert r.returncode == 2
    assert "unknown script" in r.stderr
    assert "Traceback" not in r.stderr
    after = sorted(p.name for p in (APP / "build/host").glob("*")) if (APP / "build/host").exists() else []
    assert after == before
    r = subprocess.run(["sh", str(APP / "tests/host_e2e.sh")], capture_output=True, text=True)
    assert r.returncode == 2 and "unknown script" in r.stderr
