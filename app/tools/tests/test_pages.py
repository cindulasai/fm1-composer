"""US-28: COMPOSER Studio on GitHub Pages from CI, only once the repo is public (firmware.yml, README)."""
import os, re, shlex, socket, subprocess, sys, textwrap, time
from pathlib import Path
import pytest
from conftest import APP
from test_studio import browser, studio  # noqa: F401  (fixtures; studio_url is this module's: the README's export)
from test_studio_emu import FWSC, emu, start, t2m
from test_export import emu_ready, need_emu

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
    test = next(i for i, s in enumerate(st) if "tools/tests/test_export.py -k test_27_1" in s)
    assert gate < test < exp < up, names
    assert "FM1_REQUIRE_STUDIO=1" in st[test] and f"if: {GUARD}" in st[test]
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


def test_28_1_required_mode_turns_a_missing_firmware_into_a_failure(tmp_path):
    """FM1_REQUIRE_STUDIO=1 (CI's Pages step): no firmware / emu page -> the test fails; without it, it skips"""
    probe = tmp_path / "test_probe.py"
    probe.write_text(textwrap.dedent("""
        from test_export import need_emu
        def test_missing():
            need_emu(False)
    """))
    env = {**os.environ, "PYTHONPATH": os.pathsep.join([str(APP / "tools"), str(APP / "tools" / "tests")])}
    run = lambda **e: subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(probe)],
                                     capture_output=True, text=True, env={**env, **e}, cwd=tmp_path)
    env.pop("FM1_REQUIRE_STUDIO", None)
    r = run(FM1_REQUIRE_STUDIO="1")
    assert r.returncode == 1 and "1 failed" in r.stdout and "FM1_REQUIRE_STUDIO=1: needs" in r.stdout, r.stdout[-600:]
    r = run()
    assert r.returncode == 0 and "1 skipped" in r.stdout, r.stdout[-600:]


def free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def readme_commands():
    sec = README.split("### Try it in your browser", 1)[1].split("\n## ", 1)[0]
    block = sec.split("```sh\n", 1)[1].split("```", 1)[0]
    return sec, [ln.split("#", 1)[0].strip() for ln in block.splitlines() if ln.strip()]


@pytest.fixture(scope="module")
def studio_url(tmp_path_factory):
    """the README's two commands, run as written (only the port is a free one): export, then python's server"""
    need_emu(emu_ready())
    _, (exp, srv) = readme_commands()
    where = tmp_path_factory.mktemp("readme")
    r = subprocess.run([str(APP / "fm1c"), *shlex.split(exp)[1:]], capture_output=True, text=True, cwd=where)
    assert r.returncode == 0, r.stderr                                    # (./fm1c from app; `site` lands here)
    port = free_port()
    argv = shlex.split(srv)
    assert argv[-1] == "8000"
    p = subprocess.Popen([sys.executable if argv[0] == "python3" else argv[0], *argv[1:-1], str(port)], cwd=where,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    for _ in range(100):
        try:
            socket.create_connection(("127.0.0.1", port), timeout=0.2).close()
            break
        except OSError:
            time.sleep(0.1)
    yield f"http://127.0.0.1:{port}/"
    p.terminate()
    p.wait(10)


def test_28_3_readme_try_it_section_and_its_commands_open_studio(studio, studio_url):
    sec, lines = readme_commands()
    assert "https://cindulasai.github.io/fm1-composer/" in sec and "Once this repo is public" in sec
    assert lines[0] == "./fm1c studio --export site" and lines[1] == "python3 -m http.server -d site 8000"
    page, _, _ = studio()
    assert t2m(page).url.startswith(studio_url) and emu(page).url.startswith(studio_url)    # both windows, in a browser
    start(page)                                                                          # fm1-emu boots COMPOSER
    assert page.request.get(studio_url + "firmware/composer.fwsc").body() == FWSC.read_bytes()
