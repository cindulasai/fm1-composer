"""US-28: COMPOSER Studio on GitHub Pages from CI, only once the repo is public (firmware.yml, README)."""
import os, re, socket, subprocess, sys, time, urllib.request
from pathlib import Path
from conftest import APP

WF = (APP.parent / ".github/workflows/firmware.yml").read_text()
README = (APP / "README.md").read_text()
GUARD = "github.ref == 'refs/heads/main' && !github.event.repository.private"


def job(name):
    """the text of one job under jobs: (two-space indented key to the next one)"""
    m = re.search(rf"^  {name}:\n(.*?)(?=^  \S|\Z)", WF, re.S | re.M)
    assert m, f"no job {name}"
    return m[1]


def steps(text):
    """the steps of a job, each one's text"""
    return re.split(r"^      - ", text.split("    steps:\n", 1)[1], flags=re.M)[1:]


def test_28_1_export_after_the_gate_then_pages_artifact_then_a_guarded_deploy_job():
    st = steps(job("build"))
    names = [s.splitlines()[0] for s in st]
    gate = next(i for i, s in enumerate(st) if s.startswith("name: Gate"))
    exp = next(i for i, s in enumerate(st) if "fm1c studio --export _site" in s)
    up = next(i for i, s in enumerate(st) if "actions/upload-pages-artifact@" in s)
    assert gate < exp < up, names
    assert "path: _site" in st[up]
    d = job("pages")
    assert "needs: build" in d and f"if: {GUARD}" in d
    assert re.search(r"permissions:\n\s+pages: write\n\s+id-token: write", d)
    assert "name: github-pages" in d and "actions/deploy-pages@" in d


def test_28_2_red_gate_private_repo_or_another_branch_no_export_no_deploy():
    st = steps(job("build"))
    for s in st:
        if "fm1c studio --export" in s or "upload-pages-artifact" in s:
            assert f"if: {GUARD}" in s, s                         # (no always(): a red gate stops them)
            assert "always()" not in s and "failure()" not in s and "continue-on-error" not in s
    assert f"if: {GUARD}" in job("pages")                          # private, or a dispatch elsewhere: skipped
    assert "always()" not in job("pages")
    assert "deploy-pages" not in job("build"), "the deploy only in its own guarded job"
    on = WF.split("\non:", 1)[1].split("\nconcurrency:", 1)[0]
    assert "pull_request" not in on, "a pull request never deploys"


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def test_28_3_readme_try_it_section_and_its_commands_run(tmp_path):
    sec = README.split("### Try it in your browser", 1)[1].split("\n## ", 1)[0]
    assert "https://cindulasai.github.io/fm1-composer/" in sec and "Once this repo is public" in sec
    block = sec.split("```sh\n", 1)[1].split("```", 1)[0]
    lines = [ln.split("#", 1)[0].strip() for ln in block.splitlines() if ln.strip()]
    assert lines[0] == "./fm1c studio --export site" and lines[1].startswith("python3 -m http.server -d site")
    fw = APP / "build/composer.fwsc"
    env = {**os.environ} if fw.is_file() else {**os.environ, "FM1C_FWSC": str(tmp_path / "fw.fwsc")}
    if not fw.is_file():
        (tmp_path / "fw.fwsc").write_bytes(b"FWSC")
    if not (APP / "studio/web/emu/index.html").is_file():
        import pytest
        pytest.skip("needs studio/web/emu (sh tools/get_web_emu.sh)")
    r = subprocess.run([str(APP / "fm1c"), "studio", "--export", str(tmp_path / "site")], capture_output=True, text=True,
                       env=env, cwd=APP)
    assert r.returncode == 0, r.stderr
    port = free_port()
    srv = subprocess.Popen([sys.executable, "-m", "http.server", "-d", str(tmp_path / "site"), "-b", "127.0.0.1", str(port)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        for _ in range(50):
            try:
                page = urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=2).read().decode()
                break
            except OSError:
                time.sleep(0.1)
        else:
            raise AssertionError("the static server never answered")
        assert "t2m/" in page and "emu/" in page, "Studio's two windows"
        fwsc = urllib.request.urlopen(f"http://127.0.0.1:{port}/firmware/composer.fwsc", timeout=2).read()
        assert fwsc == (fw.read_bytes() if fw.is_file() else b"FWSC")
    finally:
        srv.terminate()
        srv.wait()
