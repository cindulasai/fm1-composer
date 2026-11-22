"""Fit a Standard MIDI File to what COMPOSER plays: at most 8 channels, 96 PPQ, a polyphony cap per channel,
melodic notes in C1..C7, at most 32 KB. Every change is reported; nothing is changed silently."""
from __future__ import annotations

import os
from dataclasses import dataclass, field

import mido

from .piece import (ROLES, Channel, Note, Piece, PieceError, MAX_BYTES, MAX_CH, MAX_TICKS, PPQ, clean_name, encode)

MAX_INPUT = 8 * 1024 * 1024
LOW, HIGH = 24, 96                       # C1 .. C7
DRUM_SRC = 9                              # MIDI channel 10
# keep order when there are more parts than channels: the first ones stay
PRIORITY = ("drums", "bass", "lead", "chords", "pad", "counter", "arp", "percussion", "drone", "fx", "other")
KEYWORDS = (("counter", "counter"), ("drum", "drums"), ("kit", "drums"), ("perc", "percussion"), ("bass", "bass"),
            ("chord", "chords"), ("comp", "chords"), ("pad", "pad"), ("string", "pad"), ("arp", "arp"),
            ("drone", "drone"), ("fx", "fx"), ("effect", "fx"), ("lead", "lead"), ("melody", "lead"))


class FitError(ValueError):
    """The input cannot become a piece (missing, unreadable, no notes)."""


@dataclass
class Limits:
    poly: int = 6
    drum_poly: int = 8


@dataclass
class Report:
    lines: list[str] = field(default_factory=list)

    def add(self, s: str) -> None:
        self.lines.append(s)

    @property
    def reductions(self) -> int:
        return len(self.lines)


@dataclass
class _Part:
    name: str
    src: int
    notes: list[tuple[int, int, int, int]] = field(default_factory=list)   # tick, pitch, dur, vel (source ticks)
    vol: int = 100
    pan: int = 64
    role: str = "other"


def role_of(name: str, src: int) -> str:
    if src == DRUM_SRC:
        return "drums"
    n = name.lower()
    first = n.split()[0] if n.split() else ""
    if first in ROLES:                    # text2midi names its tracks "<role> – <instrument>"
        return first
    for k, r in KEYWORDS:
        if k in n:
            return r
    return "other"


def _utf8(s: str) -> str:
    """mido reads meta text as Latin-1; text2midi and most DAWs write UTF-8."""
    try:
        return s.encode("latin-1").decode("utf-8")
    except (UnicodeEncodeError, UnicodeDecodeError):
        return s


def _read(path: str) -> mido.MidiFile:
    if not os.path.exists(path):
        raise FitError(f"{path}: no such file")
    if os.path.isdir(path):
        raise FitError(f"{path}: is a folder, not a MIDI file")
    if os.path.getsize(path) > MAX_INPUT:
        raise FitError(f"{path}: {os.path.getsize(path) // 1024} KB is larger than the 8 MB this tool reads")
    try:
        return mido.MidiFile(path, clip=True)
    except (OSError, EOFError, ValueError, KeyError, IndexError) as e:
        raise FitError(f"{path}: not a readable MIDI file ({e})") from None


def _parts(mid: mido.MidiFile) -> tuple[list[_Part], int, bool]:
    """Notes per (track, channel); the first tempo; whether pitch bends were seen."""
    parts: dict[tuple[int, int], _Part] = {}
    tempo, bends = None, False
    for ti, tr in enumerate(mid.tracks):
        name, t = "", 0
        held: dict[tuple[int, int], list[tuple[int, int]]] = {}
        cc: dict[int, dict[int, int]] = {}
        for m in tr:
            t += m.time
            if m.type == "track_name" and not name:
                name = _utf8(m.name)
            elif m.type == "set_tempo" and tempo is None:
                tempo = m.tempo
            elif m.type == "pitchwheel" and m.pitch:
                bends = True
            elif m.type == "control_change" and m.control in (7, 10):
                cc.setdefault(m.channel, {}).setdefault(m.control, m.value)
            elif m.type == "note_on" and m.velocity:
                held.setdefault((m.channel, m.note), []).append((t, m.velocity))
            elif m.type in ("note_off", "note_on"):
                q = held.get((m.channel, m.note))
                if q:
                    st, v = q.pop(0)
                    p = parts.setdefault((ti, m.channel), _Part(name, m.channel))
                    p.notes.append((st, m.note, max(1, t - st), v))
        for (ch, note), q in held.items():         # never released: end at the track's end
            for st, v in q:
                p = parts.setdefault((ti, ch), _Part(name, ch))
                p.notes.append((st, note, max(1, t - st), v))
        for (tti, ch), p in parts.items():
            if tti == ti:
                p.name = p.name or name
                p.vol = cc.get(ch, {}).get(7, 100)
                p.pan = cc.get(ch, {}).get(10, 64)
    return [p for p in parts.values() if p.notes], tempo or 500000, bends


def _cap_poly(notes: list[Note], cap: int) -> tuple[list[Note], int]:
    """Drop notes that would start while `cap` notes already sound (the quietest of the newcomers first)."""
    out, sounding, dropped = [], [], 0
    for n in sorted(notes, key=lambda n: (n.tick, -n.vel, n.pitch)):
        sounding = [e for e in sounding if e > n.tick]
        if len(sounding) >= cap:
            dropped += 1
            continue
        sounding.append(n.tick + n.dur)
        out.append(n)
    return out, dropped


def fit(path: str, name: str | None = None, limits: Limits | None = None) -> tuple[Piece, Report]:
    limits = limits or Limits()
    rep = Report()
    mid = _read(path)
    parts, tempo_us, bends = _parts(mid)
    if not parts:
        raise FitError(f"{path}: has no notes")
    if bends:
        rep.add("pitch bends ignored (notes play at the nearest key)")
    tpq = mid.ticks_per_beat or PPQ
    for p in parts:
        p.role = role_of(p.name, p.src)
    # percussion joins the drums when there are drums
    if any(p.role == "drums" for p in parts):
        drums = next(p for p in parts if p.role == "drums")
        for p in [p for p in parts if p.role == "percussion"]:
            drums.notes += p.notes
            parts.remove(p)
            rep.add(f"part '{p.name}' merged into the drums")   # (percussion is known by its name)
    parts.sort(key=lambda p: (PRIORITY.index(p.role), -len(p.notes)))
    for p in parts[MAX_CH:]:
        rep.add(f"part '{p.name or p.role}' ({p.role}, {len(p.notes)} notes) dropped: the FM-1 plays {MAX_CH} channels")
    parts = parts[:MAX_CH]

    scale = lambda t: (t * PPQ + tpq // 2) // tpq                     # noqa: E731
    chans, end = [], 1
    for p in parts:
        drums = p.role == "drums"
        notes, folded = [], 0
        for st, pitch, dur, vel in p.notes:
            if not drums and not LOW <= pitch <= HIGH:
                folded += 1
                while pitch < LOW:
                    pitch += 12
                while pitch > HIGH:
                    pitch -= 12
            s = scale(st)
            notes.append(Note(s, pitch, max(1, min(65535, scale(st + dur) - s)), vel))
        if folded:
            rep.add(f"{p.role}: {folded} notes moved by octaves into C1..C7")
        notes, dropped = _cap_poly(notes, limits.drum_poly if drums else limits.poly)
        if dropped:
            rep.add(f"{p.role}: {dropped} notes dropped (more than {limits.drum_poly if drums else limits.poly} at once)")
        end = max([end] + [n.tick + n.dur for n in notes])
        chans.append(Channel(p.role, notes, vol=min(127, p.vol), pan=min(127, p.pan), src=p.src))

    bar = PPQ * 4
    length = min(MAX_TICKS, -(-end // bar) * bar)
    for c in chans:
        late = [n for n in c.notes if n.tick >= length]
        if late:
            rep.add(f"{c.role}: {len(late)} notes past {MAX_TICKS // bar} bars dropped")
            c.notes = [n for n in c.notes if n.tick < length]
    tempo_x10 = max(300, min(3000, round(600_000_000 / tempo_us)))
    if tempo_x10 != round(600_000_000 / tempo_us):
        rep.add(f"tempo {60_000_000 / tempo_us:.1f} BPM clamped to {tempo_x10 / 10:g}")
    piece = Piece(clean_name(name if name is not None else os.path.splitext(os.path.basename(path))[0]),
                  tempo_x10, length, chans)
    # too big: cut at a bar boundary until it fits
    while True:
        try:
            encode(piece)
            break
        except PieceError as e:
            if e.code != 9 or piece.len_ticks <= bar:
                raise
            piece.len_ticks -= bar
            for c in piece.channels:
                c.notes = [n for n in c.notes if n.tick < piece.len_ticks]
    if piece.len_ticks < length:
        rep.add(f"cut to {piece.len_ticks // bar} bars to fit the FM-1's {MAX_BYTES // 1024} KB per piece")
    return piece, rep
