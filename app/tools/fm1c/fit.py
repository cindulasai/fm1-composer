"""Fit a Standard MIDI File to what COMPOSER plays: at most 8 channels, 96 PPQ, a polyphony cap per channel,
melodic notes in C1..C7, at most 32 KB. Every change is reported; nothing is changed silently."""
from __future__ import annotations

import math, os, re
from dataclasses import dataclass, field

import mido

from . import sounds
from .piece import (DEFAULT_SOUND, ROLES, Channel, Note, Piece, PieceError, Section, MAX_BYTES, MAX_CH, MAX_SECT,
                    MAX_TICKS, PPQ, clean_name, encode, section_name)

MAX_INPUT = 8 * 1024 * 1024
LOW, HIGH = 24, 96                       # C1 .. C7
DRUM_SRC = 9                              # MIDI channel 10
# keep order when there are more parts than channels: the first ones stay
PRIORITY = ("drums", "bass", "lead", "chords", "pad", "counter", "arp", "percussion", "drone", "fx", "other")
KEYWORDS = (("counter", "counter"), ("drum", "drums"), ("kit", "drums"), ("perc", "percussion"), ("bass", "bass"),
            ("chord", "chords"), ("comp", "chords"), ("pad", "pad"), ("string", "pad"), ("arp", "arp"),
            ("drone", "drone"), ("fx", "fx"), ("effect", "fx"), ("lead", "lead"), ("melody", "lead"))


# SLOOP's 16 scales (firmware/src/live.c SCALE_MASK, bit i = i semitones above the root); Studio's lib.rs SCALE_MASKS
SCALE_MASKS = (0xFFF, 0xAB5, 0x5AD, 0x6AD, 0x6B5, 0x295, 0x4A9, 0x9AD, 0x5AB, 0xAD5, 0x56B, 0xAAD, 0x4E9, 0x555, 0x6DB,
               0xB6D)
# text2midi's tonal systems, as Studio's engine has them (its copied knowledge base)
TONAL = os.path.join(os.path.dirname(__file__), "..", "..", "studio", "engine", "crates", "t2m-core", "data", "tonal")


def _systems() -> dict[str, list[int]]:
    out = {}
    for f in sorted(os.listdir(TONAL)) if os.path.isdir(TONAL) else []:
        if f.endswith(".ron"):
            with open(os.path.join(TONAL, f), encoding="utf-8") as fh:
                for m in re.finditer(r'id:\s*"([^"]+)".*?steps:\s*\[([^\]]*)\]', fh.read(), re.S):
                    out[m[1]] = [int(x) for x in m[2].replace(",", " ").split()]
    return out


def _pitch_class(s: str) -> int | None:
    """text2midi's parse_pitch_class: a letter, then at most two sharps or flats, not mixed"""
    s = s.strip()
    if not s or s[0].upper() not in "CDEFGAB":
        return None
    pc, shift = "C D EF G A B".index(s[0].upper()), 0
    for count, c in enumerate(s[1:]):
        step = 1 if c in "#\u266f" else -1 if c in "b\u266d" else None
        if step is None or count >= 2 or shift * step < 0:
            return None
        shift += step
    return (pc + shift) % 12


def brief_key(brief) -> tuple[int, int] | None:
    """A text2midi brief's key for the FM-1's live keyboard (US-24): (its tonic's pitch class, the SLOOP scale its
    tonal system is), in 12-TET only; None when it is not one of the 16 (the FM-1 then guesses it from the notes).
    Studio's engine (lib.rs brief_key) reads the same."""
    t = brief.get("tonality") if isinstance(brief, dict) else None
    if not isinstance(t, dict) or t.get("tuning") != "12tet":
        return None
    tonic = t.get("tonic")
    if isinstance(tonic, str):
        pc = _pitch_class(tonic)
    elif isinstance(tonic, dict) and isinstance(tonic.get("hz"), (int, float)) and not isinstance(tonic["hz"], bool):
        hz = float(tonic["hz"])          # text2midi's class_of_hz: the nearest semitone; not a positive number: C
        pc = int(math.floor(69 + 12 * math.log2(hz / 440) + 0.5)) % 12 if math.isfinite(hz) and hz > 0 else 0
    else:
        return None
    steps = _systems().get(t["system"]) if isinstance(t.get("system"), str) else None
    if pc is None or steps is None or any(not 0 <= x <= 11 for x in steps):
        return None
    mask = sum(1 << x for x in set(steps))
    return (pc, SCALE_MASKS.index(mask)) if mask in SCALE_MASKS else None


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
    program: int | None = None           # the first program change of its channel in its track
    role: str = "other"


def role_of(name: str, src: int, program: int | None = None) -> str:
    if src == DRUM_SRC:
        return "drums"
    n = name.lower()
    first = n.split()[0] if n.split() else ""
    if first in ROLES:                    # text2midi names its tracks "<role> – <instrument>"
        return first
    for k, r in KEYWORDS:
        if k in n:
            return r
    return sounds.role_hint(name, program) or "other"     # (an instrument or a program has a role)


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
    except (OSError, EOFError, ValueError, KeyError, IndexError, mido.midifiles.meta.KeySignatureError) as e:
        raise FitError(f"{path}: not a readable MIDI file ({e})") from None


def _parts(mid: mido.MidiFile) -> tuple[list[_Part], int, bool, list[tuple[int, str]]]:
    """Notes per (track, channel); the first tempo; whether pitch bends were seen; the markers (tick, text)."""
    parts: dict[tuple[int, int], _Part] = {}
    tempo, bends, markers = None, False, []
    for ti, tr in enumerate(mid.tracks):
        name, t = "", 0
        held: dict[tuple[int, int], list[tuple[int, int]]] = {}
        cc: dict[int, dict[int, int]] = {}
        prog: dict[int, int] = {}
        for m in tr:
            t += m.time
            if m.type == "track_name" and not name:
                name = _utf8(m.name)
            elif m.type == "marker":
                markers.append((t, _utf8(m.text)))
            elif m.type == "set_tempo" and tempo is None:
                tempo = m.tempo
            elif m.type == "pitchwheel" and m.pitch:
                bends = True
            elif m.type == "control_change" and m.control in (7, 10):
                cc.setdefault(m.channel, {}).setdefault(m.control, m.value)
            elif m.type == "program_change":
                prog.setdefault(m.channel, m.program)
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
                p.program = prog.get(ch)
    return [p for p in parts.values() if p.notes], tempo or 500000, bends, markers


def _sections(markers: list[tuple[int, str]], scale, length: int, rep: Report) -> list[Section]:
    """text2midi's form (a marker at each section's start) as the sections table: in time order, the first of a
    tick kept, inside the piece, at most MAX_SECT."""
    out: list[Section] = []
    same = late = 0
    for t, text in sorted(markers, key=lambda m: m[0]):
        s = scale(t)
        if s >= length:
            late += 1
        elif out and out[-1].tick == s:
            same += 1
        else:
            out.append(Section(s, section_name(text)))
    if same:
        rep.add(f"{same} section markers dropped (at the tick of the one before)")
    if late:
        rep.add(f"{late} section markers past the end dropped")
    if len(out) > MAX_SECT:
        rep.add(f"{len(out) - MAX_SECT} sections after the {MAX_SECT}th dropped (the FM-1 keeps {MAX_SECT})")
    return out[:MAX_SECT]


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


def fit(path: str, name: str | None = None, limits: Limits | None = None,
        key: tuple[int, int] | None = None) -> tuple[Piece, Report]:
    """key: text2midi's (brief_key), carried when the piece has sections"""
    limits = limits or Limits()
    rep = Report()
    mid = _read(path)
    parts, tempo_us, bends, markers = _parts(mid)
    if not parts:
        raise FitError(f"{path}: has no notes")
    if bends:
        rep.add("pitch bends ignored (notes play at the nearest key)")
    tpq = mid.ticks_per_beat or PPQ
    for p in parts:
        p.role = role_of(p.name, p.src, p.program)
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
    chans, end, unknown, fm6 = [], 1, [], 0
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
        snd = sounds.sound(p.name, p.program, p.role)
        if snd is None:
            unknown.append(f"'{p.name or p.role}'")
            snd = DEFAULT_SOUND
        elif snd >= sounds.FM6_FIRST:
            fm6 += 1
            if fm6 > sounds.FM6_POOL:
                alt = sounds.FM6_ALT[snd - sounds.FM6_FIRST]
                rep.add(f"{p.role}: {sounds.NAMES[snd]} plays as {sounds.NAMES[alt]} "
                        f"(the FM-1 plays at most {sounds.FM6_POOL} FM6 sounds at once)")
                snd = alt
        chans.append(Channel(p.role, notes, sound=snd, vol=min(127, p.vol), pan=min(127, p.pan), src=p.src))
    if unknown:
        rep.add(f"role default sound for {', '.join(unknown)} (no known instrument or GM program)")

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
                  tempo_x10, length, chans, _sections(markers, scale, length, rep), key)
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
            piece.sections = [s for s in piece.sections if s.tick < piece.len_ticks]
    if piece.len_ticks < length:
        rep.add(f"cut to {piece.len_ticks // bar} bars to fit the FM-1's {MAX_BYTES // 1024} KB per piece")
    return piece, rep
