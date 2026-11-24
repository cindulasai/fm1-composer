"""The .f1p piece format: v1, and v2 = v1 + a sections table, written only when a piece has sections; the table also
carries the piece's key (text2midi's, US-24) when it has one (firmware/src/piece.h is the reader; the layout is
documented there). A channel may carry a loop length in 1/16 steps (polymeter) and a swing (US-25), in two bytes of
its record that were reserved before: a piece without them keeps its bytes."""
from __future__ import annotations

import struct
import unicodedata
import zlib
from dataclasses import dataclass, field

MAGIC = b"F1P1"
VERSION = 1
VERSION_SECT = 2
MAX_SECT = 24
SECT_NAME = 8
PPQ = 96
MAX_BYTES = 32768
MAX_CH = 8
MAX_TICKS = 96 * 4 * 999
NAME_LEN = 24
REST = 255
MAX_STEPS = 64
MAX_SWING = 100
ROLES = ("lead", "counter", "chords", "pad", "bass", "drums", "percussion", "arp", "drone", "fx", "other")
DEFAULT_SOUND = 255

# error codes, as piece.h
SHORT, BAD_MAGIC, BAD_VERSION, BAD_CRC, BAD_NCH, BAD_HEADER, BAD_TABLE, BAD_EVENT, TOO_BIG, BAD_SECT = range(1, 11)


class PieceError(ValueError):
    def __init__(self, code: int, msg: str):
        super().__init__(msg)
        self.code = code


@dataclass(frozen=True, order=True)
class Note:
    tick: int
    pitch: int
    dur: int
    vel: int


@dataclass
class Channel:
    role: str
    notes: list[Note] = field(default_factory=list)
    sound: int = DEFAULT_SOUND
    vol: int = 100
    pan: int = 64
    src: int = 0
    steps: int = 0                       # loop length in 1/16 steps, 0 = none (US-25 polymeter)
    swing: int = 0                       # 0..100: every 2nd 1/16 late by swing / 200 of a step (US-25)


@dataclass(frozen=True)
class Section:
    tick: int                            # its start; it ends where the next starts (the last at the piece's end)
    name: str


@dataclass
class Piece:
    name: str
    tempo_x10: int
    len_ticks: int
    channels: list[Channel]
    sections: list[Section] = field(default_factory=list)
    key: tuple[int, int] | None = None   # (root 0..11 = C..B, SLOOP's scale 0..15); written only with sections


def clean_name(s: str) -> str:
    """Printable ASCII, at most 24 characters; accents folded, other characters dropped; never empty."""
    folded = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode("ascii")
    out = " ".join("".join(c if 32 <= ord(c) <= 126 else " " for c in folded).split())
    return out[:NAME_LEN].rstrip() or "untitled"


def section_name(s: str) -> str:
    """A section's name as the FM-1 shows it: clean_name's characters, upper case, at most 8."""
    return clean_name(s).upper()[:SECT_NAME].rstrip()


def _sorted_notes(ch: Channel) -> list[Note]:
    return sorted(ch.notes)


def _validate(p: Piece) -> None:
    if not 1 <= len(p.channels) <= MAX_CH:
        raise PieceError(BAD_NCH, f"a piece has 1 to {MAX_CH} channels, not {len(p.channels)}")
    if not 300 <= p.tempo_x10 <= 3000:
        raise PieceError(BAD_HEADER, f"tempo must be 30.0 to 300.0 BPM, not {p.tempo_x10 / 10}")
    if not 1 <= p.len_ticks <= MAX_TICKS:
        raise PieceError(BAD_HEADER, f"length must be 1 to {MAX_TICKS} ticks, not {p.len_ticks}")
    if p.name != clean_name(p.name):
        raise PieceError(BAD_HEADER, f"name must be printable ASCII, 1 to {NAME_LEN} characters: {p.name!r}")
    for i, c in enumerate(p.channels):
        if c.role not in ROLES:
            raise PieceError(BAD_TABLE, f"channel {i + 1}: unknown role {c.role!r}")
        if not (0 <= c.sound <= 255 and 0 <= c.vol <= 127 and 0 <= c.pan <= 127 and 0 <= c.src <= 15):
            raise PieceError(BAD_TABLE, f"channel {i + 1}: sound/volume/pan/source out of range")
        if not (0 <= c.steps <= MAX_STEPS and 0 <= c.swing <= MAX_SWING):
            raise PieceError(BAD_TABLE, f"channel {i + 1}: loop length 0 to {MAX_STEPS} steps, swing 0 to {MAX_SWING}")
        for n in c.notes:
            if not (0 <= n.tick < p.len_ticks and 0 <= n.pitch <= 127 and 1 <= n.vel <= 127 and 1 <= n.dur <= 65535):
                raise PieceError(BAD_EVENT, f"channel {i + 1}: note out of range: {n}")
    if len(p.sections) > MAX_SECT:
        raise PieceError(BAD_SECT, f"a piece has at most {MAX_SECT} sections, not {len(p.sections)}")
    prev = -1
    for s in p.sections:
        if not (prev < s.tick < p.len_ticks) or not 1 <= len(s.name) <= SECT_NAME or any(not 32 <= ord(c) <= 126 for c in s.name):
            raise PieceError(BAD_SECT, f"section {s.name!r} at {s.tick}: starts must rise inside the piece, "
                                       f"names 1 to {SECT_NAME} printable characters")
        prev = s.tick
    if p.key is not None and not (len(p.key) == 2 and 0 <= p.key[0] <= 11 and 0 <= p.key[1] <= 15):
        raise PieceError(BAD_SECT, f"key must be (root 0..11, scale 0..15), not {p.key!r}")


def _events(ch: Channel) -> bytes:
    out = bytearray()
    t = 0
    for n in _sorted_notes(ch):
        d = n.tick - t
        while d > 65535:
            out += struct.pack("<HHBB", 65535, 1, REST, 1)
            d -= 65535
        out += struct.pack("<HHBB", d, n.dur, n.pitch, n.vel)
        t = n.tick
    return bytes(out)


def encode(p: Piece) -> bytes:
    _validate(p)
    evs = [_events(c) for c in p.channels]
    sect = b""
    if p.sections:
        root, scale = (p.key[0] + 1, p.key[1]) if p.key is not None else (0, 0)
        sect = struct.pack("<BBBx", len(p.sections), root, scale) + b"".join(
            struct.pack("<I", s.tick) + s.name.encode("ascii").ljust(SECT_NAME, b"\0") for s in p.sections)
    off = 48 + 16 * len(p.channels) + len(sect)
    table = bytearray()
    for c, e in zip(p.channels, evs):
        table += struct.pack("<BBBBBBBxII", ROLES.index(c.role), c.sound, c.vol, c.pan, c.src, c.steps, c.swing, len(e) // 6, off)
        off += len(e)
    total = off + 4
    if total > MAX_BYTES:
        raise PieceError(TOO_BIG, f"piece is {total} bytes; the FM-1 takes at most {MAX_BYTES}")
    hdr = MAGIC + struct.pack("<BBHHHI", VERSION_SECT if p.sections else VERSION, len(p.channels), 0, p.tempo_x10, PPQ, p.len_ticks)
    hdr += p.name.encode("ascii").ljust(NAME_LEN, b"\0") + struct.pack("<II", total, 0)
    body = hdr + bytes(table) + sect + b"".join(evs)
    return body + struct.pack("<I", zlib.crc32(body) & 0xFFFFFFFF)


def decode(b: bytes) -> Piece:
    """The inverse of encode, with the same checks as piece_check() in piece.h."""
    n = len(b)
    if n > MAX_BYTES:
        raise PieceError(TOO_BIG, f"{n} bytes is more than {MAX_BYTES}")
    if n < 48 + 16 + 4:
        raise PieceError(SHORT, "too short to be a piece")
    if b[:4] != MAGIC:
        raise PieceError(BAD_MAGIC, "not a .f1p piece")
    if b[4] not in (VERSION, VERSION_SECT):
        raise PieceError(BAD_VERSION, f"piece version {b[4]}; this fm1c reads versions {VERSION} and {VERSION_SECT}")
    _, _, nch, flags, tempo, ppq, length = struct.unpack_from("<4sBBHHHI", b, 0)
    total, reserved = struct.unpack_from("<II", b, 40)
    if total != n:
        raise PieceError(SHORT, "length field does not match the file (truncated?)")
    if zlib.crc32(b[:-4]) & 0xFFFFFFFF != struct.unpack_from("<I", b, n - 4)[0]:
        raise PieceError(BAD_CRC, "checksum mismatch (corrupt piece)")
    if not 1 <= nch <= MAX_CH:
        raise PieceError(BAD_NCH, f"{nch} channels")
    raw = b[16:40]
    name = raw.split(b"\0", 1)[0]
    if (flags or reserved or not 300 <= tempo <= 3000 or ppq != PPQ or not 1 <= length <= MAX_TICKS or not name
            or any(not 32 <= c <= 126 for c in name) or any(raw[len(name):])):
        raise PieceError(BAD_HEADER, "bad header")
    end = 48 + 16 * nch
    if end + 4 > n:
        raise PieceError(SHORT, "channel table truncated")
    sections, key = [], None
    if b[4] == VERSION_SECT:
        ns = b[end]
        if end + 8 > n or not 1 <= ns <= MAX_SECT or b[end + 3] or end + 8 + 12 * ns > n:
            raise PieceError(BAD_SECT, "bad sections table")
        if b[end + 1] > 12 or b[end + 2] > 15 or (not b[end + 1] and b[end + 2]):
            raise PieceError(BAD_SECT, "bad key in the sections table")
        key = (b[end + 1] - 1, b[end + 2]) if b[end + 1] else None
        prev = -1
        for i in range(ns):
            t, = struct.unpack_from("<I", b, end + 4 + 12 * i)
            raw_s = b[end + 8 + 12 * i:end + 16 + 12 * i]
            nm = raw_s.split(b"\0", 1)[0]
            if not prev < t < length or not nm or any(not 32 <= c <= 126 for c in nm) or any(raw_s[len(nm):]):
                raise PieceError(BAD_SECT, f"section {i + 1}: bad start or name")
            sections.append(Section(t, nm.decode("ascii")))
            prev = t
        end += 4 + 12 * ns
    chans = []
    for i in range(nch):
        role, sound, vol, pan, src, steps, swing, r2, nev, off = struct.unpack_from("<BBBBBBBBII", b, 48 + 16 * i)
        if role >= len(ROLES) or vol > 127 or pan > 127 or src > 15 or steps > MAX_STEPS or swing > MAX_SWING or r2:
            raise PieceError(BAD_TABLE, f"channel {i + 1}: bad record")
        if off != end or nev > (n - 4 - end) // 6:
            raise PieceError(BAD_TABLE, f"channel {i + 1}: bad events offset/count")
        t, notes = 0, []
        for j in range(nev):
            d, dur, pitch, vel = struct.unpack_from("<HHBB", b, off + 6 * j)
            t += d
            if t >= length:
                raise PieceError(BAD_EVENT, f"channel {i + 1}: event past the end")
            if pitch == REST:
                continue
            if pitch > 127 or not 1 <= vel <= 127 or dur < 1:
                raise PieceError(BAD_EVENT, f"channel {i + 1}: bad note")
            notes.append(Note(t, pitch, dur, vel))
        chans.append(Channel(ROLES[role], notes, sound, vol, pan, src, steps, swing))
        end = off + 6 * nev
    if end + 4 != n:
        raise PieceError(BAD_TABLE, "trailing bytes after the events")
    return Piece(name.decode("ascii"), tempo, length, chans, sections, key)


def dump(p: Piece) -> str:
    """Canonical text, identical to tests/piece_dump.c for the same piece."""
    lines = [f"piece {p.name}|{p.tempo_x10}|{p.len_ticks}|{len(p.channels)}"]
    ends = [s.tick for s in p.sections[1:]] + [p.len_ticks]
    lines += [f"s {s.tick} {e} {s.name}" for s, e in zip(p.sections, ends)]
    if p.sections and p.key is not None:
        lines.append(f"k {p.key[0]} {p.key[1]}")
    for i, c in enumerate(p.channels):
        lines.append(f"ch {i} {ROLES.index(c.role)} {c.sound} {c.vol} {c.pan} {c.src}")
        if c.steps or c.swing:
            lines.append(f"t {c.steps} {c.swing}")
        lines += [f"n {n.tick} {n.dur} {n.pitch} {n.vel}" for n in _sorted_notes(c)]
    return "\n".join(lines) + "\n"
