"""US-7: fm1c push / list / delete / pull through the real command, against COMPOSER's own transfer code and
library (app/tests/xfer_host: xfer.c + lib.c on this computer, flash kept in a file)."""
import struct, subprocess, sys
import pytest
from hypothesis import given, settings, strategies as st
from conftest import APP

sys.path.insert(0, str(APP / "tools"))
from fm1c import link  # noqa: E402
from fm1c.piece import Piece, Channel, Note, encode, decode  # noqa: E402
from test_cli import fm1c  # noqa: E402


@pytest.fixture(scope="module")
def host_bin(tmp_path_factory):
    exe = tmp_path_factory.mktemp("xh") / "xfer_host"
    subprocess.run(["cc", "-O1", "-Wno-unused-function", f"-I{APP}/firmware/src", f"-I{APP}/firmware/hal",
                    "-o", str(exe), str(APP / "tests/xfer_host.c")], check=True)
    return exe


@pytest.fixture
def port(host_bin, tmp_path):
    return f"exec:{host_bin} {tmp_path / 'flash.bin'}"


def piece(name, n=40, chans=1):
    return encode(Piece(name, 1200, (n * 24 // 384 + 1) * 384, [Channel("lead", [Note(i * 24, 48 + i % 24, 20, 100) for i in range(n)])] * chans))


def write(tmp_path, name, data):
    p = tmp_path / f"{name}.f1p"
    p.write_bytes(data)
    return p


def test_7_3_push_list_delete_pull(tmp_path, port):
    a, b = write(tmp_path, "alpha", piece("alpha")), write(tmp_path, "beta tune", piece("beta tune", 300))
    r = fm1c("push", a, b, "--port", port)
    assert r.returncode == 0, r.stderr
    assert "pushed 2, verified 2" in r.stdout and "alpha: #" in r.stdout
    r = fm1c("list", "--port", port)
    assert r.returncode == 0, r.stderr
    lines = r.stdout.splitlines()
    assert "beta tune" in lines[0] and "alpha" in lines[1] and lines[2].startswith("2 pieces, ")   # newest first
    pid = int(lines[1].split()[0][1:])
    out = tmp_path / "back.f1p"
    r = fm1c("pull", pid, "-o", out, "--port", port)
    assert r.returncode == 0 and out.read_bytes() == a.read_bytes(), r.stderr
    r = fm1c("delete", pid, "--port", port)
    assert r.returncode == 0 and f"deleted #{pid}" in r.stdout
    r = fm1c("list", "--port", port)
    assert "alpha" not in r.stdout and "1 piece," in r.stdout


def test_7_2_staged_while_playing_then_saved(host_bin, tmp_path):
    p = link.ExecPort(f"{host_bin} {tmp_path / 'f.bin'}")
    lk = link.Link(p, timeout=3)
    try:
        p.line("play 1")
        assert lk.put(piece("during")) == ("staged", None)
        assert lk.info()["staged"] and lk.info()["count"] == 0
        with pytest.raises(link.LinkError) as e:
            lk.put(piece("second"))
        assert e.value.code == 9 and "busy" in str(e.value)
        p.line("play 0")
        pid = lk.wait_saved(5)
        assert [x["name"] for x in lk.list()] == ["during"] and lk.list()[0]["id"] == pid
        assert lk.put(piece("after"))[0] == "saved"
    finally:
        p.close()


def test_7_4_hostile_cli(tmp_path, port):
    junk = tmp_path / "junk.f1p"
    junk.write_bytes(b"not a piece at all")
    for args, code, text in [
        (("push", junk, "--port", port), 2, "junk"),
        (("push", tmp_path / "missing.f1p", "--port", port), 2, "missing.f1p"),
        (("push", tmp_path, "--port", port), 2, ""),
        (("push", "--port", port), 2, "required"),
        (("delete", "12345", "--port", port), 1, "no such piece"),
        (("delete", "x", "--port", port), 2, "invalid int"),
        (("pull", "777", "--port", port), 1, "no piece #777"),
        (("list", "--port", "exec:/nonexistent/prog"), 2, "cannot run"),
        (("list", "--port", "zz-no-such-port-zz"), 2, "no MIDI port matching"),
        (("list", "--port", "exec:sleep 30", "--timeout", "0.3"), 1, "no answer"),
    ]:
        r = fm1c(*args)
        assert r.returncode == code, (args, r.stdout, r.stderr)
        assert text in r.stderr and "Traceback" not in r.stderr, (args, r.stderr)
        assert len(r.stderr.strip().splitlines()) <= 1, r.stderr
    a = write(tmp_path, "keep", piece("keep"))
    assert fm1c("push", a, "--port", port).returncode == 0
    pid = fm1c("list", "--port", port).stdout.split()[0][1:]
    out = tmp_path / "exists.f1p"
    out.write_bytes(b"mine")
    r = fm1c("pull", pid, "-o", out, "--port", port)
    assert r.returncode == 2 and out.read_bytes() == b"mine" and "already exists" in r.stderr


def test_7_4_library_full_is_refused(tmp_path, port):
    big = piece("big", 2720, 2)                         # 32724 bytes + header: 9 sectors each, 8 fit in 73
    assert 4096 * 8 - 64 < len(big) <= 32768
    files = [write(tmp_path, f"big{i}", big) for i in range(9)]
    r = fm1c("push", *files, "--port", port)
    assert r.returncode == 1
    assert "pushed 8, verified 8, refused 1" in r.stdout and "library full" in r.stderr


@settings(max_examples=300, deadline=None)
@given(st.binary(max_size=600))
def test_7_4_pack7_round_trip_and_frames(b):
    assert link.unpack7(link.pack7(b)) == b
    assert all(x < 0x80 for x in link.pack7(b))
    f = link.frame(0x04, b)
    assert link.parse(f) == (0x04, b)
    bad = bytearray(f)
    bad[6] ^= 0x01
    assert link.parse(bytes(bad)) is None or link.parse(bytes(bad)) != (0x04, b)


def test_7_4_parse_rejects_others():
    assert link.parse(b"\xF0\x00\x20\x59\x11\x00\xF7") is None          # the updater's
    assert link.parse(b"\xF0\x7D\x46\x31\x01\xF7") is None              # too short
    assert link.parse(b"\xF0\x7D\x46\x31\x41\x80\x00\x00\x00\x00\xF7") is None
    with pytest.raises(ValueError):
        link.unpack7(b"\x00")


def fake_rtmidi(monkeypatch, outs, ins):
    m = type(sys)("rtmidi")
    class Out:
        def get_ports(self): return outs
    class In:
        def get_ports(self): return ins
        def ignore_types(self, **k): pass
        def set_callback(self, f): m.cb = f
        def open_port(self, k): pass
    Out.open_port = lambda self, k: None
    m.MidiOut, m.MidiIn = Out, In
    monkeypatch.setitem(sys.modules, "rtmidi", m)
    return m


def test_7_4_no_ports_at_all_says_none(monkeypatch):
    fake_rtmidi(monkeypatch, [], [])
    with pytest.raises(link.PortError, match=r"\(ports: none\)"):
        link.MidiPort("FM-1")


def test_7_4_a_port_that_only_sends_is_not_enough(monkeypatch):
    fake_rtmidi(monkeypatch, ["FM-1_950 (emu)"], [])
    with pytest.raises(link.PortError, match="no MIDI port matching"):
        link.MidiPort("FM-1")


def test_7_3_midi_port_keeps_only_sysex(monkeypatch):
    m = fake_rtmidi(monkeypatch, ["FM-1_950 (emu)"], ["FM-1_950 (emu)"])
    port = link.MidiPort("fm-1")
    m.cb(([0x90, 60, 100], 0), None)
    m.cb(([0xF0, 0x7D, 0xF7], 0), None)
    assert port.q.get_nowait() == bytes([0xF0, 0x7D, 0xF7]) and port.q.empty()


class Fake:
    """a port answering what `reply(cmd)` returns (a frame or None)"""
    def __init__(self, reply):
        self.sent, self.q, self.reply = [], [], reply
    def send(self, f):
        self.sent.append(f[4])
        r = self.reply(f[4], self.sent.count(f[4]))
        if r:
            self.q.append(r)
    def recv(self, t):
        return self.q.pop(0) if self.q else None


ACK = lambda cmd: link.frame(0x40 | cmd, struct.pack("<I", 0))  # noqa: E731


def test_7_4_a_lost_info_reply_is_asked_again():
    info = link.frame(0x41, bytes([1, 0]) + struct.pack("<II", 0, 0) + bytes([0]))
    p = Fake(lambda c, k: info if k == 2 else None)
    assert link.Link(p, timeout=0.05).info()["count"] == 0 and p.sent == [1, 1]


def test_7_4_end_is_sent_once_when_its_answer_is_lost():
    p = Fake(lambda c, k: ACK(c) if c in (3, 4) else None)
    with pytest.raises(link.LinkError, match="no answer"):
        link.Link(p, timeout=0.05).put(b"x" * 10)
    assert p.sent.count(5) == 1 and p.sent.count(8) == 1 and p.sent[-1] == 8   # END once, then one ABORT


def test_7_4_begin_and_delete_are_sent_once():
    for call, cmd in ((lambda lk: lk.put(b"x" * 10), 3), (lambda lk: lk.delete(4), 6)):
        p = Fake(lambda c, k: None)
        with pytest.raises(link.LinkError, match="no answer"):
            call(link.Link(p, timeout=0.05))
        assert p.sent.count(cmd) == 1, cmd     # (a repeated DELETE would meet "no such piece")


def test_7_2_refusal_while_waiting_for_the_save():
    p = Fake(lambda c, k: None)
    p.q.append(link.frame(0x7F, bytes([5, 7])))
    with pytest.raises(link.LinkError) as e:
        link.Link(p).wait_saved(1.0)
    assert e.value.code == 7


def test_7_2_push_during_playback_no_wait(port, tmp_path, monkeypatch):
    monkeypatch.setenv("XFER_PLAYING", "1")
    f = tmp_path / "a.f1p"
    f.write_bytes(encode(Piece("a", 1200, 384, [Channel("lead", [Note(0, 60, 90, 96)])])))
    r = fm1c("push", f, "--no-wait", "--wait", "0.5", "--port", port)
    assert r.returncode == 0 and "staged" in r.stdout, r.stdout + r.stderr
    r = fm1c("push", f, "--wait", "0.5", "--port", port)
    assert r.returncode == 1 and "still waiting" in r.stderr, r.stdout + r.stderr
