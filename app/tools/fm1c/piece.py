"""The .f1p piece format v1 (firmware/src/piece.h is the reader; the layout is documented there)."""
from __future__ import annotations

import struct
import unicodedata
import zlib
from dataclasses import dataclass, field

MAGIC = b"F1P1"
VERSION = 1
PPQ = 96
MAX_BYTES = 32768
MAX_CH = 8
MAX_TICKS = 96 * 4 * 999
NAME_LEN = 24
REST = 255
ROLES = ("lead", "counter", "chords", "pad", "bass", "drums", "percussion", "arp", "drone", "fx", "other")
DEFAULT_SOUND = 255

# error codes, as piece.h
SHORT, BAD_MAGIC, BAD_VERSION, BAD_CRC, BAD_NCH, BAD_HEADER, BAD_TABLE, BAD_EVENT, TOO_BIG = range(1, 10)


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


@dataclass
class Piece:
    name: str
    tempo_x10: int
    len_ticks: int
    channels: list[Channel]


def clean_name(s: str) -> str:
    """Printable ASCII, at most 24 characters; accents folded, other characters dropped; never empty."""
    folded = unicodedata.normalize("NFKD", s).encode("ascii", "ignore").decode("ascii")
    out = " ".join("".join(c if 32 <= ord(c) <= 126 else " " for c in folded).split())
    return out[:NAME_LEN].rstrip() or "untitled"


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
        for n in c.notes:
            if not (0 <= n.tick < p.len_ticks and 0 <= n.pitch <= 127 and 1 <= n.vel <= 127 and 1 <= n.dur <= 65535):
                raise PieceError(BAD_EVENT, f"channel {i + 1}: note out of range: {n}")


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
    off = 48 + 16 * len(p.channels)
    table = bytearray()
    for c, e in zip(p.channels, evs):
        table += struct.pack("<BBBBB3xII", ROLES.index(c.role), c.sound, c.vol, c.pan, c.src, len(e) // 6, off)
        off += len(e)
    total = off + 4
    if total > MAX_BYTES:
        raise PieceError(TOO_BIG, f"piece is {total} bytes; the FM-1 takes at most {MAX_BYTES}")
    hdr = MAGIC + struct.pack("<BBHHHI", VERSION, len(p.channels), 0, p.tempo_x10, PPQ, p.len_ticks)
    hdr += p.name.encode("ascii").ljust(NAME_LEN, b"\0") + struct.pack("<II", total, 0)
    body = hdr + bytes(table) + b"".join(evs)
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
    if b[4] != VERSION:
        raise PieceError(BAD_VERSION, f"piece version {b[4]}; this fm1c reads version {VERSION}")
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
    chans = []
    for i in range(nch):
        role, sound, vol, pan, src, r0, r1, r2, nev, off = struct.unpack_from("<BBBBBBBBII", b, 48 + 16 * i)
        if role >= len(ROLES) or vol > 127 or pan > 127 or src > 15 or r0 or r1 or r2:
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
        chans.append(Channel(ROLES[role], notes, sound, vol, pan, src))
        end = off + 6 * nev
    if end + 4 != n:
        raise PieceError(BAD_TABLE, "trailing bytes after the events")
    return Piece(name.decode("ascii"), tempo, length, chans)


def dump(p: Piece) -> str:
    """Canonical text, identical to tests/piece_dump.c for the same piece."""
    lines = [f"piece {p.name}|{p.tempo_x10}|{p.len_ticks}|{len(p.channels)}"]
    for i, c in enumerate(p.channels):
        lines.append(f"ch {i} {ROLES.index(c.role)} {c.sound} {c.vol} {c.pan} {c.src}")
        lines += [f"n {n.tick} {n.dur} {n.pitch} {n.vel}" for n in _sorted_notes(c)]
    return "\n".join(lines) + "\n"
