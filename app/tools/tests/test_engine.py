"""US-13: COMPOSER Studio's engine (app/studio/engine, Rust; WASM in the page) makes the same pieces as fm1c."""
import functools, hashlib, http.server, json, os, random, re, struct, subprocess, sys, threading
import pytest
from conftest import APP

sys.path.insert(0, str(APP / "tools"))
from fm1c import cli as fm1cli, fit as fitmod  # noqa: E402
from fm1c.piece import PieceError, encode  # noqa: E402

ENGINE = APP / "studio" / "engine"
BRIEFS = ENGINE / "suite" / "briefs"
SUITE = ["cinematic-orchestral-1", "electronic-house-1", "electronic-techno-1", "funk-classic-1", "hiphop-lofi-1", "jazz-swing-1"]
CARGO_ENV = {**os.environ, "PATH": f"{os.path.expanduser('~/.cargo/bin')}:{os.environ['PATH']}"}


@pytest.fixture(scope="session")
def studio_cli():
    r = subprocess.run(["cargo", "build", "-q", "--release", "-p", "studio", "--manifest-path", str(ENGINE / "Cargo.toml")],
                       capture_output=True, text=True, env=CARGO_ENV)
    assert r.returncode == 0, r.stderr[-3000:]
    return str(ENGINE / "target" / "release" / "studio-cli")


def run(*args, cwd=None):
    return subprocess.run([str(a) for a in args], capture_output=True, text=True, cwd=cwd)


def t2m_or_skip():
    t2m = os.environ.get("FM1C_T2M") or fm1cli.DEFAULT_T2M
    if not os.access(t2m, os.X_OK):
        pytest.skip(f"text2midi's t2m-cli (the reference) is not built at {t2m}")


@pytest.mark.parametrize("seed", [1, 2])
@pytest.mark.parametrize("brief", SUITE)
def test_13_1_make_equals_fm1c_compose(studio_cli, tmp_path, brief, seed):
    t2m_or_skip()
    a, b = tmp_path / "a", tmp_path / "b"
    a.mkdir(); b.mkdir()
    ra = run(APP / "fm1c", "compose", BRIEFS / f"{brief}.json", "--seed", seed, "-o", "x.f1p", cwd=a)
    rb = run(studio_cli, "make", BRIEFS / f"{brief}.json", "--seed", seed, "--name", brief, "-o", "x.f1p", cwd=b)
    assert ra.returncode == 0 and rb.returncode == 0, ra.stderr + rb.stderr
    assert rb.stdout == ra.stdout                      # summary and every report line
    assert (b / "x.f1p").read_bytes() == (a / "x.f1p").read_bytes()


# ---- 13.2: random MIDI files through both fitters ----
NAMES = ["", "Lead", "bass – Fender", "Drums", "Percussion", "perc loop", "Strings", "pad warm", "Arp 1", "chords – Rhodes",
         "counter melody", "FX riser", "drone", "Melody", "Kit", "piano comp", "Café ☕", "\xe9t\xe9", "Synth",
         # (US-22: text2midi's "<id> – <instrument>", AI track ids, instrument names, unknown ones)
         "lead – keys.rhodes", "keys – keys.piano", "keys", "pad2", "texture – fx.riser", "hook – harp.orchestral",
         "x – no.such", "Piano", "Vibes", "drums – kit.standard", "bass – bass.upright", "chords – guitar.nylon",
         "lead – vibes.vibraphone", "counter – keys.rhodes", "rhodes – KEYS.RHODES", "Track 1", "Strings 2"]


def vlq(n):
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append(0x80 | (n & 0x7F))
        n >>= 7
    return bytes(reversed(out))


def meta(t, d):
    return b"\x00\xff" + bytes([t]) + vlq(len(d)) + bytes(d)


def gen_track(rng, tpq):
    head = bytearray()
    if rng.random() < 0.8:
        nm = rng.choice(NAMES)
        head += meta(3, nm.encode("utf-8") if rng.random() < 0.8 else nm.encode("latin-1", "replace"))
    if rng.random() < 0.3:
        t = rng.choice([0, 1, 100000, 199999, 200000, 500000, 2000000, 2000001, 16000000, 9600000, 3200000, 128000,
                        rng.randint(1, 0xFFFFFF)])
        head += meta(0x51, t.to_bytes(3, "big"))
    if rng.random() < 0.2:
        head += meta(0x58, [4, 2, 24, 8]) + meta(0x59, [rng.randint(0, 3), 0]) + meta(0x01, b"text")
    evs = []                                   # (tick, order, bytes without delta)
    chans = rng.sample(range(16), rng.choice([1, 1, 1, 2, 3]))
    n = rng.choice([0, 1, 5, 40, 200, 600, 1500] + ([7000] if rng.random() < 0.15 else []))
    span = max(1, tpq) * 4 * rng.choice([2, 8, 32, 200])
    pitches = list(range(128)) if rng.random() < 0.5 else [rng.randint(0, 127) for _ in range(3)]
    for i in range(n):
        ch = rng.choice(chans)
        t = rng.randint(0, span)
        if rng.random() < 0.01:
            t += max(1, tpq) * 4 * 1001 * rng.choice([1, 70])
        d = rng.choice([1, 2, max(1, tpq) // 2, max(1, tpq), rng.randint(1, 4 * max(1, tpq)), 70000 * max(1, tpq) // 96])
        p, v = rng.choice(pitches), rng.randint(1, 127) if rng.random() > 0.01 else 0x90
        evs.append((t, 1, bytes([0x90 | ch, p, v])))
        if rng.random() > 0.02:                    # (else never released)
            off = bytes([0x80 | ch, p, rng.randint(0, 127)]) if rng.random() < 0.5 else bytes([0x90 | ch, p, 0])
            evs.append((t + d, 0, off))
    for ch in chans:
        if rng.random() < 0.3: evs.append((rng.randint(0, span), 2, bytes([0xB0 | ch, 7, rng.randint(0, 127)])))
        if rng.random() < 0.3: evs.append((rng.randint(0, span), 2, bytes([0xB0 | ch, 10, rng.randint(0, 127)])))
        if rng.random() < 0.2: evs.append((rng.randint(0, span), 2, bytes([0xE0 | ch, 0, rng.choice([64, 70])])))
        if rng.random() < 0.5: evs.append((rng.choice([0, 0, rng.randint(0, span)]), 2,
                                           bytes([0xC0 | ch, rng.choice([0, 4, 25, 33, 64, 88, 127, rng.randint(0, 127)])])))
        if rng.random() < 0.1: evs.append((rng.randint(0, span), 0, bytes([0x80 | ch, 60, 0])))     # off, never on
    if rng.random() < 0.1:
        evs.append((rng.randint(0, span), 2, b"\xf0\x03\x7d\x01\xf7"))
    evs.sort(key=lambda e: (e[0], e[1]))
    body, t, last = bytearray(head), 0, None
    for tick, _, ev in evs:
        body += vlq(tick - t)
        t = tick
        if ev[0] < 0xF0 and ev[0] == last and rng.random() < 0.5:
            body += ev[1:]                         # running status
        else:
            body += ev if ev[0] != 0xF0 else b"\xf0" + vlq(len(ev) - 1) + ev[1:]
            last = ev[0] if ev[0] < 0xF0 else None
    body += b"\x00\xff\x2f\x00"
    return b"MTrk" + struct.pack(">I", len(body)) + bytes(body)


def gen_midi(rng):
    tpq = rng.choice([0, 1, 7, 24, 96, 96, 120, 480, 960])
    ntr = rng.choice([1, 2, 4, 6, 9, 12])
    b = b"MThd" + struct.pack(">IHHH", 6, 1, ntr, tpq) + b"".join(gen_track(rng, tpq) for _ in range(ntr))
    r = rng.random()
    if r < 0.03:
        b = b[:rng.randint(0, len(b))]             # truncated
    elif r < 0.06:
        i = rng.randrange(len(b)); b = b[:i] + bytes([rng.randrange(256)]) + b[i + 1:]       # one byte flipped
    elif r < 0.08:
        bad = rng.choice([meta(0x59, [9, 0]), meta(0x51, [1]), meta(0x54, [0xA0, 0, 0, 0, 0]), meta(0x54, [0, 60, 0, 0, 0]),
                          b"\x00\xf4", meta(0x20, []), meta(0x00, [1]), b"\x00\xf8\x05"])
        trk = b"\x00\x90\x3c\x40\x60\x80\x3c\x00" + bad + b"\x00\xff\x2f\x00"
        b = b"MThd" + struct.pack(">IHHH", 6, 1, 1, 96) + b"MTrk" + struct.pack(">I", len(trk)) + trk
    return b


def fm1c_fit(path, name, poly):
    """what `fm1c convert PATH --name NAME --poly N -o x.f1p` prints and writes, in process; None = refused"""
    try:
        piece, rep = fitmod.fit(str(path), name, fitmod.Limits(poly=poly))
    except (fitmod.FitError, PieceError):
        return None
    if not any(c.notes for c in piece.channels):
        return None
    data = encode(piece)
    notes = sum(len(c.notes) for c in piece.channels)
    out = (f"x.f1p: '{piece.name}', {len(piece.channels)} channels, {notes} notes, {piece.len_ticks // 384} bars, "
           f"{piece.tempo_x10 / 10:g} BPM, {len(data)} bytes, {rep.reductions} reductions\n")
    return data, out + "".join(f"  - {ln}\n" for ln in rep.lines)


def engine_fit(cli, path, name, poly, cwd):
    r = run(cli, "convert", path, "--name", name, "--poly", poly, "-o", "x.f1p", cwd=cwd)
    assert "panicked" not in r.stderr, r.stderr
    if r.returncode:
        assert r.returncode == 2 and r.stderr.startswith("studio: ") and r.stderr.count("\n") == 1, r.stderr
        return None
    return (cwd / "x.f1p").read_bytes(), r.stdout


def test_13_2_random_midi_files_fit_as_fm1c_does(studio_cli, tmp_path):
    rng = random.Random(1302)
    kinds = {"same": 0, "refused": 0}
    for i in range(300):
        p = tmp_path / f"m{i}.mid"
        p.write_bytes(gen_midi(rng))
        name, poly = rng.choice(["song", "Café ☕ 𝄞 ünïcode name that is long", "  ", "a" * 40]), rng.choice([6, 6, 1, 3, 16])
        want, got = fm1c_fit(p, name, poly), engine_fit(studio_cli, p, name, poly, tmp_path)
        assert (want is None) == (got is None), f"file {i}: fm1c {'refused' if want is None else 'made it'}, engine {got}"
        if want:
            assert got[1] == want[1], f"file {i}"
            assert got[0] == want[0], f"file {i}"
        kinds["refused" if want is None else "same"] += 1
    assert kinds["same"] > 200 and kinds["refused"] > 10, kinds


@pytest.mark.parametrize("case", ["truncated", "no notes", "only past bar 999"])
def test_13_4_hostile_midi(studio_cli, tmp_path, case):
    good = b"MThd" + struct.pack(">IHHH", 6, 1, 1, 96)
    trk = {"truncated": b"\x00\x90\x3c\x40\x60\x80\x3c\x00\x00\xff\x2f\x00",
           "no notes": b"\x00\xff\x51\x03\x07\xa1\x20\x00\xff\x2f\x00",
           "only past bar 999": vlq(96 * 4 * 1000) + b"\x90\x3c\x40\x60\x80\x3c\x00\x00\xff\x2f\x00"}[case]
    data = good + b"MTrk" + struct.pack(">I", len(trk)) + trk
    if case == "truncated":
        data = data[:30]
    p = tmp_path / "h.mid"
    p.write_bytes(data)
    assert fm1c_fit(p, "h", 6) is None and engine_fit(studio_cli, p, "h", 6, tmp_path) is None
    r = run(studio_cli, "convert", p, "--name", "h", "-o", "x.f1p", cwd=tmp_path)
    assert {"truncated": "ends", "no notes": "has no notes", "only past bar 999": "no notes left to play"}[case] in r.stderr
    assert not (tmp_path / "x.f1p").exists()


def suite_brief(**change):
    b = json.loads((BRIEFS / "hiphop-lofi-1.json").read_text())
    for k, v in change.items():
        if k == "style": b["identity"]["style"] = v
        if k == "tracks": b["tracks"] = v; [s.__setitem__("tracks", v) for s in b["form"]]
    return json.dumps(b)


HOSTILE_BRIEFS = {
    "empty": ("", "not a brief"),
    "not JSON": ("tempo 90, sad", "not a brief"),
    "1 MB": ('{"x": "' + "a" * (1 << 20) + '"}', "at most 256 KB"),
    "unknown style": (suite_brief(style="polka.martian"), "cannot be composed"),
    "0 tracks": (suite_brief(tracks=[]), "cannot be composed"),
}


@pytest.mark.parametrize("case", list(HOSTILE_BRIEFS))
def test_13_4_hostile_briefs(studio_cli, tmp_path, case):
    text, msg = HOSTILE_BRIEFS[case]
    (tmp_path / "b.json").write_text(text)
    r = run(studio_cli, "make", tmp_path / "b.json", "--seed", 1, "--name", "x", "-o", "x.f1p", cwd=tmp_path)
    assert r.returncode == 2 and "panicked" not in r.stderr and msg in r.stderr and r.stderr.count("\n") == 1, r.stderr
    assert not (tmp_path / "x.f1p").exists()


def test_13_4_ai_answers_read_back_or_refused(studio_cli, tmp_path):
    brief = (BRIEFS / "hiphop-lofi-1.json").read_text()
    (tmp_path / "a.txt").write_text("<think>hmm</think>\nHere you go:\n```json\n" + brief + "\n```")
    r = run(studio_cli, "brief", tmp_path / "a.txt")
    assert r.returncode == 0 and json.loads(r.stdout)["identity"]["style"] == "hiphop.lofi"
    (tmp_path / "body.json").write_text(json.dumps({"choices": [{"message": {"content": brief}}]}))
    assert run(studio_cli, "reply", tmp_path / "body.json").returncode == 0
    for bad, msg in [("no json here", "no JSON object found"), ('{"error": {"message": "invalid api key"}}', "invalid api key")]:
        (tmp_path / "x.txt").write_text(bad)
        r = run(studio_cli, "reply" if "error" in bad else "brief", tmp_path / "x.txt")
        assert r.returncode == 2 and msg in r.stderr and "panicked" not in r.stderr, r.stderr
    p = run(studio_cli, "prompt")
    assert p.returncode == 0 and "hiphop.lofi" in p.stdout and "JSON Schema" in p.stdout


# ---- 13.3: the same engine in a browser ----
@pytest.fixture(scope="session")
def web_root():
    r = subprocess.run(["sh", str(APP / "studio" / "build.sh")], capture_output=True, text=True, env=CARGO_ENV)
    assert r.returncode == 0, r.stderr[-3000:]
    root = APP / "studio" / "web"
    h = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(root))
    http.server.SimpleHTTPRequestHandler.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), h)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{srv.server_address[1]}"
    srv.shutdown()


@pytest.fixture(scope="session")
def page(web_root, pw_browser):
    pg = pw_browser.new_page()
    pg.goto(web_root + "/engine/")
    pg.evaluate("async () => { window.E = await import('/engine/studio.js'); await window.E.default(); }")
    yield pg
    pg.close()


def test_13_3_wasm_make_equals_fm1c_compose(page, tmp_path):
    t2m_or_skip()
    brief = BRIEFS / "hiphop-lofi-1.json"
    assert run(APP / "fm1c", "compose", brief, "--seed", 1, "-o", tmp_path / "x.f1p").returncode == 0
    r = page.evaluate("""([b, n]) => { const t = performance.now(); const m = E.make(b, 1, n);
        return {f1p: Array.from(m.f1p), ms: performance.now() - t, midi: m.midi.length, ch: m.channels, notes: m.notes} }""",
                      [brief.read_text(), "hiphop-lofi-1"])
    assert bytes(r["f1p"]) == (tmp_path / "x.f1p").read_bytes()
    assert r["ms"] < 1000 and r["midi"] > 1000 and r["ch"] == 4 and r["notes"] > 100


def test_13_4_wasm_errors_are_messages_not_traps(page):
    r = page.evaluate("""() => { const out = [];
        for (const b of ['', 'nope', '{"version": 1}']) { try { E.make(b, 1, 'x'); out.push('made'); } catch (e) { out.push(String(e)); } }
        try { E.convert(new Uint8Array([77, 84, 104, 100]), 'x'); out.push('made'); } catch (e) { out.push(String(e)); }
        return out; }""")
    assert all(x != "made" and "unreachable" not in x and "RuntimeError" not in x for x in r), r
    assert "not a brief" in r[0] and "MIDI" in r[3]
    assert page.evaluate("() => E.prompt().length") > 1000          # the module still works after the errors


# ---- 13.5: the copies are text2midi_v2's files ----
def blob_id(data):
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def test_13_5_copied_crates_equal_text2midi_v2_at_the_named_commit():
    prov = (ENGINE / "PROVENANCE.md").read_text()
    commit = re.search(r"^commit ([0-9a-f]{40})$", prov, re.M).group(1)
    listed = dict((p, b) for b, p in re.findall(r"^([0-9a-f]{40})  (\S+)$", prov, re.M))
    assert len(listed) > 100
    here = {str(f.relative_to(ENGINE)): blob_id(f.read_bytes())
            for d in ("crates", "suite") for f in (ENGINE / d).rglob("*") if f.is_file()}
    assert here == listed                       # nothing changed, added or missing
    repo = os.environ.get("T2M_REPO") or os.path.expanduser("~/Claude/Projects/text2midi_v2")
    if os.path.isdir(os.path.join(repo, ".git")):
        r = run("git", "-C", repo, "ls-tree", "-r", commit, *sorted({p.split("/")[0] + "/" + p.split("/")[1] for p in listed}))
        assert r.returncode == 0, r.stderr
        theirs = dict((ln.split("\t")[1], ln.split()[2]) for ln in r.stdout.splitlines())
        assert theirs == listed


def test_13_2_oversize_cut_with_notes_on_every_bar_line(studio_cli, tmp_path):
    body = bytearray()
    for i in range(1000):                                  # 6-note chords on every beat: ~36 KB, so it is cut at a bar
        for k, p in enumerate((48, 52, 55, 60, 64, 67)):
            body += vlq(96 if (k == 0 and i) else 0) + bytes([0x90, p, 90])
        for k, p in enumerate((48, 52, 55, 60, 64, 67)):
            body += vlq(0) + bytes([0x80, p, 0])
    body += b"\x00\xff\x2f\x00"
    p = tmp_path / "dense.mid"
    p.write_bytes(b"MThd" + struct.pack(">IHHH", 6, 1, 1, 96) + b"MTrk" + struct.pack(">I", len(body)) + bytes(body))
    want, got = fm1c_fit(p, "dense", 6), engine_fit(studio_cli, p, "dense", 6, tmp_path)
    assert want and "cut to" in want[1] and got == want


# ---- 22.4: the sounds, roles and mix through both fitters ----
def test_22_4_every_suite_brief_fits_as_fm1c_does(studio_cli, tmp_path):
    t2m_or_skip()
    briefs = sorted(BRIEFS.glob("*.json"))
    assert len(briefs) == 36
    sounds = set()
    for brief in briefs:
        a, b = tmp_path / f"a-{brief.stem}.f1p", tmp_path / f"b-{brief.stem}.f1p"
        ra = run(APP / "fm1c", "compose", brief, "--seed", 1, "-o", a.name, cwd=tmp_path)
        rb = run(studio_cli, "make", brief, "--seed", 1, "--name", brief.stem, "-o", b.name, cwd=tmp_path)
        assert ra.returncode == 0 and rb.returncode == 0, ra.stderr + rb.stderr
        assert rb.stdout.replace(b.name, a.name) == ra.stdout, brief.stem
        assert a.read_bytes() == b.read_bytes(), brief.stem
        assert "role default sound" not in ra.stdout, ra.stdout
        sounds |= {a.read_bytes()[48 + 16 * i + 1] for i in range(a.read_bytes()[5])}
    assert 255 not in sounds and len(sounds) >= 12, sounds          # every part has its instrument's sound


def test_22_4_random_midi_files_fit_as_fm1c_does(studio_cli, tmp_path):
    rng = random.Random(2204)
    same = 0
    for i in range(300):
        p = tmp_path / f"m{i}.mid"
        p.write_bytes(gen_midi(rng))
        want, got = fm1c_fit(p, "song", 6), engine_fit(studio_cli, p, "song", 6, tmp_path)
        assert want == got, f"file {i}"
        same += want is not None
    assert same > 200


def smf(tracks, raw=b""):
    """a MIDI file: per track (name, channel, program or None), four notes; `raw` events lead the last track"""
    out = []
    for k, (name, ch, prog) in enumerate(tracks):
        body = meta(3, name.encode("utf-8")) + (bytes([0, 0xC0 | ch, prog]) if prog is not None else b"")
        body += (raw if k == len(tracks) - 1 else b"")
        for j in range(4):
            body += vlq(0 if j == 0 else 48) + bytes([0x90 | ch, 48 + j + k, 90]) + vlq(48) + bytes([0x80 | ch, 48 + j + k, 0])
        out.append(b"MTrk" + struct.pack(">I", len(body) + 4) + body + b"\x00\xff\x2f\x00")
    return b"MThd" + struct.pack(">IHHH", 6, 1, len(out), 96) + b"".join(out)


def test_22_4_every_program_and_instrument_fits_as_fm1c_does(studio_cli, tmp_path):
    from fm1c import sounds
    files = [smf([("Track", 0, p), ("Loop perc", 1, p), ("Drum", 9, p), ("hit", 9, None)]) for p in range(128)]
    files += [smf([(f"x – {a}", 0, None), (a.upper(), 9, None), (a, 2, 33)])
              for a in list(sounds.ALIASES) + list(sounds.INSTRUMENTS)]
    files += [smf([("bass – bass.upright", 0, None), ("lead – vibes.vibraphone", 1, None),
                   ("chords – guitar.nylon", 2, None), ("pad 3", 3, 8), ("counter – keys.rhodes", 4, None),
                   ("arp", 5, 4), ("drone", 6, 24), ("fx – kit.standard", 7, None)]),
              smf([("lead – kit.standard", 0, 4), ("drums – keys.piano", 1, 25), ("perc", 2, 64),
                   ("Track 9", 3, None), ("", 4, None), ("keys – who.knows", 5, None)]),
              smf([("Track", 0, None)], raw=b"\x00\xc0\x85"),
              smf([("Track", 3, None)], raw=b"\x00\xb3\x07\x00\x00\xb3\x0a\x7f\x00\xb3\x07\x50\x00\xc3\x7f\x00\xc3\x01")]
    for i, data in enumerate(files):
        p = tmp_path / f"t{i}.mid"
        p.write_bytes(data)
        want = fm1c_fit(p, "t", 6)
        assert want is not None and engine_fit(studio_cli, p, "t", 6, tmp_path) == want, f"file {i}: {want}"
