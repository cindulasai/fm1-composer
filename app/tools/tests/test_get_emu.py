"""US-1: get_emu.sh installs a checksummed fm1-emu, refuses a bad archive, reuses an installed one."""
import hashlib, os, subprocess, tarfile
from conftest import APP

SCRIPT = APP / "tools/get_emu.sh"


def make_archive(tmp, with_live=True):
    d = tmp / "pkg/fm1-emu-x"
    d.mkdir(parents=True)
    names = ["fm1-emu"] + (["fm1-live"] if with_live else [])
    for n in names:
        f = d / n
        f.write_text("#!/bin/sh\necho fake\n")
        f.chmod(0o755)
    a = tmp / "emu.tar.gz"
    with tarfile.open(a, "w:gz") as t:
        t.add(tmp / "pkg", arcname="pkg")
    return a, hashlib.sha256(a.read_bytes()).hexdigest()


def run(tmp, url, sum_, dest):
    env = dict(os.environ, FM1_EMU_URL=url, FM1_EMU_SUM=sum_, FM1_EMU_DIR=str(dest))
    return subprocess.run(["sh", str(SCRIPT)], capture_output=True, text=True, env=env)


def test_1_1_get_emu_installs_checked_archive(tmp_path):
    a, s = make_archive(tmp_path)
    dest = tmp_path / "out"
    r = run(tmp_path, a.as_uri(), s, dest)
    assert r.returncode == 0, r.stderr
    assert (dest / "fm1-live").exists() and r.stdout.strip().endswith("fm1-live")
    # already there: reused, no download (a broken URL must not matter)
    r = run(tmp_path, "file:///nonexistent/x.tar.gz", "0" * 64, dest)
    assert r.returncode == 0 and r.stdout.strip().endswith("fm1-live")


def test_1_4_get_emu_refuses_bad_checksum_and_bad_archive(tmp_path):
    a, s = make_archive(tmp_path)
    r = run(tmp_path, a.as_uri(), "0" * 64, tmp_path / "o1")
    assert r.returncode == 1 and "checksum mismatch" in r.stderr
    assert not (tmp_path / "o1/fm1-live").exists()
    b = tmp_path / "b"; b.mkdir()
    a2, s2 = make_archive(b, with_live=False)
    r = run(tmp_path, a2.as_uri(), s2, tmp_path / "o2")
    assert r.returncode == 1 and "not in the archive" in r.stderr
    r = run(tmp_path, "file:///nonexistent/x.tar.gz", s, tmp_path / "o3")
    assert r.returncode == 1 and "download failed" in r.stderr
