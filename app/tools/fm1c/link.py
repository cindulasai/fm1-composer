"""Talk to COMPOSER over USB-MIDI SysEx (protocol: app/firmware/src/xfer.c).

Link(port).info() / list() / put(data) / wait_saved() / delete(id) / get(id, length); errors raise LinkError.
A port is a MIDI port (python-rtmidi, matched by part of its name) or "exec:COMMAND" (a program speaking hex frames
on stdin/stdout, as app/tests/xfer_host does)."""
from __future__ import annotations

import os
import queue
import shlex
import struct
import subprocess
import threading
import time
import zlib

HEAD = bytes([0x7D, 0x46, 0x31])
CHUNK = 256
NAK_TEXT = {1: "damaged frame", 2: "chunk out of order", 3: "size not allowed", 4: "no transfer in progress",
            5: "incomplete piece", 6: "not a valid piece", 7: "library full", 8: "flash error",
            9: "busy: a pushed piece is waiting for playback to stop", 10: "no such piece", 11: "unknown command",
            12: "transfer timed out"}


class LinkError(Exception):
    def __init__(self, code: int, msg: str):
        super().__init__(msg)
        self.code = code


class PortError(Exception):
    """No such port, or it cannot be opened (a usage problem, exit 2)."""


def pack7(b: bytes) -> bytes:
    out = bytearray()
    for i in range(0, len(b), 7):
        g = b[i:i + 7]
        out.append(sum(((x >> 7) & 1) << k for k, x in enumerate(g)))
        out.extend(x & 0x7F for x in g)
    return bytes(out)


def unpack7(b: bytes) -> bytes:
    out, i = bytearray(), 0
    while i < len(b):
        hi, g = b[i], b[i + 1:i + 8]
        if not g or hi & 0x80 or any(x & 0x80 for x in g):
            raise ValueError("not 7-bit packed")
        out.extend(x | (((hi >> k) & 1) << 7) for k, x in enumerate(g))
        i += 1 + len(g)
    return bytes(out)


def frame(cmd: int, payload: bytes = b"") -> bytes:
    return b"\xF0" + HEAD + bytes([cmd]) + pack7(payload + struct.pack("<I", zlib.crc32(payload))) + b"\xF7"


def parse(f: bytes):
    """(cmd, payload) of one of our frames; None for anything else (or damaged)."""
    if len(f) < 7 or f[0] != 0xF0 or f[-1] != 0xF7 or f[1:4] != HEAD:
        return None
    try:
        d = unpack7(f[5:-1])
    except ValueError:
        return None
    if len(d) < 4 or zlib.crc32(d[:-4]) != struct.unpack("<I", d[-4:])[0]:
        return None
    return f[4], d[:-4]


class MidiPort:
    def __init__(self, want: str):
        try:
            import rtmidi
        except ImportError as e:                                  # pragma: no cover (pinned dependency)
            raise PortError(f"python-rtmidi is missing ({e})") from e
        saved = os.dup(2)                                         # (the MIDI system's own complaints: not ours to print)
        try:
            with open(os.devnull, "w") as null:
                os.dup2(null.fileno(), 2)
                self.i, self.o = rtmidi.MidiIn(), rtmidi.MidiOut()
                names, in_names = self.o.get_ports(), self.i.get_ports()
        except Exception as e:                                    # no MIDI system here (e.g. no ALSA sequencer)
            raise PortError(f"no MIDI port matching '{want}' (MIDI is not available here: {e or type(e).__name__})") from e
        finally:
            os.dup2(saved, 2)
            os.close(saved)
        hits = [k for k, n in enumerate(names) if want.lower() in n.lower()]
        ins = [k for k, n in enumerate(in_names) if want.lower() in n.lower()]
        if not hits or not ins:
            raise PortError(f"no MIDI port matching '{want}' (ports: {', '.join(names) or 'none'}); "
                            "connect the FM-1 (the emulator: fm1c emu)")
        self.q: queue.Queue = queue.Queue()
        self.i.ignore_types(sysex=False, timing=True, active_sense=True)
        self.i.set_callback(lambda m, _: m[0][:1] == [0xF0] and self.q.put(bytes(m[0])))
        self.i.open_port(ins[0])
        self.o.open_port(hits[0])

    def send(self, f: bytes) -> None:
        self.o.send_message(list(f))

    def recv(self, timeout: float):
        try:
            return self.q.get(timeout=timeout)
        except queue.Empty:
            return None

    def close(self) -> None:
        self.i.close_port()
        self.o.close_port()


class ExecPort:
    def __init__(self, cmd: str):
        try:
            self.p = subprocess.Popen(shlex.split(cmd), stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        except OSError as e:
            raise PortError(f"cannot run {cmd}: {e.strerror}") from e
        self.q: queue.Queue = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.p.stdout:
            line = line.strip()
            if line.startswith("F0"):
                self.q.put(bytes.fromhex(line))

    def send(self, f: bytes) -> None:
        self.line(f.hex().upper())

    def line(self, s: str) -> None:
        self.p.stdin.write(s + "\n")
        self.p.stdin.flush()

    def recv(self, timeout: float):
        try:
            return self.q.get(timeout=timeout)
        except queue.Empty:
            return None

    def close(self) -> None:
        try:
            self.p.stdin.close()
            self.p.wait(timeout=2)
        except (OSError, subprocess.TimeoutExpired):
            self.p.kill()
            self.p.wait()


def open_port(spec: str | None):
    spec = spec or os.environ.get("FM1C_PORT") or "FM-1"
    return ExecPort(spec[5:]) if spec.startswith("exec:") else MidiPort(spec)


class Link:
    def __init__(self, port, timeout: float = 3.0, retries: int = 3):
        self.port, self.timeout, self.retries = port, timeout, retries

    def _ask(self, cmd: int, payload: bytes = b"", answers=None, retry: bool = True):
        answers = answers or {0x40 | cmd}
        for _ in range(self.retries if retry else 1):
            self.port.send(frame(cmd, payload))
            end = time.monotonic() + self.timeout
            while (left := end - time.monotonic()) > 0:
                r = self.port.recv(left)
                p = r and parse(r)
                if not p:
                    continue
                rc, d = p
                if rc == 0x7F and len(d) == 2 and d[0] == cmd:
                    raise LinkError(d[1], NAK_TEXT.get(d[1], f"refused (code {d[1]})"))
                if rc in answers:
                    return rc, d
        raise LinkError(0, "no answer from the FM-1")

    def info(self) -> dict:
        _, d = self._ask(0x01)
        count, free, room, flags = d[1], *struct.unpack("<II", d[2:10]), d[10]
        return {"version": d[0], "count": count, "free": free, "room": room, "playing": bool(flags & 1), "staged": bool(flags & 2)}

    def list(self) -> list[dict]:
        out = []
        for i in range(self.info()["count"]):
            _, d = self._ask(0x02, bytes([i]))
            pid, ln = struct.unpack("<II", d[1:9])
            out.append({"id": pid, "len": ln, "name": d[9:33].split(b"\0")[0].decode("ascii", "replace")})
        return out

    def put(self, data: bytes):
        """('saved', id) or ('staged', None)."""
        self._ask(0x03, struct.pack("<I", len(data)), retry=False)
        try:
            for off in range(0, len(data), CHUNK):
                self._ask(0x04, struct.pack("<I", off) + data[off:off + CHUNK], retry=False)
            rc, d = self._ask(0x05, answers={0x45, 0x4F}, retry=False)
        except LinkError:
            try:
                self._ask(0x08, retry=False)
            except LinkError:
                pass
            raise
        return ("saved", struct.unpack("<I", d)[0]) if rc == 0x45 else ("staged", None)

    def wait_saved(self, timeout: float) -> int:
        end = time.monotonic() + timeout
        while (left := end - time.monotonic()) > 0:
            p = (r := self.port.recv(min(left, 1.0))) and parse(r)
            if p and p[0] == 0x45:
                return struct.unpack("<I", p[1])[0]
            if p and p[0] == 0x7F and p[1][:1] == b"\x05":
                raise LinkError(p[1][1], NAK_TEXT.get(p[1][1], "refused"))
        raise LinkError(0, "still waiting for playback to stop")

    def delete(self, pid: int) -> None:
        self._ask(0x06, struct.pack("<I", pid), retry=False)

    def get(self, pid: int, length: int) -> bytes:
        out = bytearray()
        while len(out) < length:
            _, d = self._ask(0x07, struct.pack("<II", pid, len(out)))
            if struct.unpack("<I", d[:4])[0] != len(out) or len(d) == 4:
                raise LinkError(0, "a chunk came back out of place")
            out += d[4:]
        return bytes(out)
