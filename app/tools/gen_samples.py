#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Leo Kuroshita (@kurogedelic), Hügelton Instruments
"""Build the SAMPLE engine's sample sets from WAV files into a C header.

Samples are stored as IMA ADPCM (4 bit). Roots come from the file names, loops
from the WAV 'smpl' chunk; the ADPCM state at the loop start is stored so loops
restart exactly.

COMPOSER: SLOOP 2.8.5's tools/gen_samples.py (bac9c05, blob a2affba762a497b7929400475ff134ce40fd6b34), cut down
(firmware/SLOOP-PROVENANCE.md). Its only inputs are the CC0 sets in assets/samples-cc0 (ATTRIBUTION.txt there):
PIANO (VCSL, a grand), STRGS and HORNS (VSCO-2 CE). Not here: gen_waves.py (the Hügelton Sample Pack, all rights
reserved: never an input), SLOOP's other sets and its GM kit, SLICE's BREAK, the build cache. STRGS and HORNS loop
their steady body (kind "body", COMPOSER's) so a held note sustains; SLOOP plays them as one-shot stabs.
One SAMPLE preset is written per set (SET_PRESETS: its name and sound). A file named ..._m<n>.wav has its root
given (MIDI note n). Pure Python (sampleio.py), no numpy: the same bytes on Python >= 3.12 (its sum()).

    python3 tools/gen_samples.py engines/felucca_samples.h
"""
import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from sampleio import ima_encode, key_split, onset, peak, read_any_wav, resample  # noqa: E402

SRC = Path(__file__).resolve().parents[1]
CC0 = SRC / "assets" / "samples-cc0"
TR = 22050                                   # stored sample rate

# set -> kind ("piano" its attack then a steady loop that the envelope fades; "body", COMPOSER's: a stab's steady
# body looped, the rest cut)
CC0_SETS = [("PIANO", "piano"), ("STRGS", "body"), ("HORNS", "body")]

# the preset of each set: name, {SET (filled in), TUNE, BITS, LOOP (filled in), CUT, -, DRV, -}, env, mono,
# sends (DIST, CHORUS, DELAY, REVERB; COMPOSER plays the piece's effects, not these)
SET_PRESETS = {      # COMPOSER: the names PIANO, STRINGS, HORNS (SLOOP: GRAND PNO, STRING STB, HORN STAB)
    "PIANO": ("PIANO", [0, 0, 0, 0, 127, 0, 0, 0], (0, 110, 0, 58), 0, (0, 6, 8, 24)),
    "STRGS": ("STRINGS", [0, 0, 0, 0, 122, 0, 10, 0], (0, 127, 127, 26), 0, (0, 10, 12, 24)),
    "HORNS": ("HORNS", [0, 0, 0, 0, 120, 0, 20, 0], (0, 127, 127, 22), 0, (8, 0, 14, 18)),
}

_wavs = {}


def wav(path):
    """read_any_wav, once per file and run"""
    if path not in _wavs:
        _wavs[path] = read_any_wav(path)
    return _wavs[path]


def cc0_entries(setname, kind):
    """-> [(file name, int16 samples at TR, loop or None, root)]"""
    files = sorted((CC0 / setname).glob("*.wav"))
    out = []
    for p in files:
        given = re.search(r"_m(\d+)\.wav$", p.name)
        if not given:
            raise SystemExit(f"{p}: the root note must be in the name (..._m<note>.wav)")
        sr, x = wav(p)
        x = x[max(0, onset(x) - 16):]
        root = int(given.group(1))                      # (the file says it)
        x = resample(x, sr, TR)
        keep = {"body": 1.0, "piano": 1.0}[kind]
        x = x[:int(keep * TR)]
        n = len(x)
        if kind == "piano":                          # the attack, then a steady tone the envelope (DEC) fades:
            ls, le, xf, a0 = int(0.55 * TR), n - 1, int(0.08 * TR), int(0.30 * TR)   # long notes in 1 s
            w = int(0.02 * TR)
            env = [max(1e-6, (sum(v * v for v in x[max(0, i - w):i + w]) / (2 * w)) ** 0.5) for i in range(0, n, w)]
            ref = env[a0 // w]
            x = [v * (ref / env[min(len(env) - 1, i // w)] if i >= a0 else 1.0) for i, v in enumerate(x)]
            ls = best_loop_start(x, le, xf, int(0.45 * TR), int(0.65 * TR))   # (the tone in phase: no dip)
            for i in range(xf):
                a = i / xf
                x[le - xf + i] = x[le - xf + i] * (1 - a) + x[ls - xf + i] * a
            loop = (ls, le)
        else:                                        # COMPOSER "body": the loop in the stab's steady part (its
            le, xf = int(0.20 * TR), int(0.03 * TR)  # first 0.25 s), crossfaded as "piano" is; cut after it
            ls = best_loop_start(x, le, xf, int(0.08 * TR), int(0.12 * TR))
            for i in range(xf):
                a = i / xf
                x[le - xf + i] = x[le - xf + i] * (1 - a) + x[ls - xf + i] * a
            x = x[:le + 1]
            loop = (ls, le)
        pk = peak(x)
        out.append((p.name, [int(v * 30000 / pk) for v in x], loop, root))
    return out


def best_loop_start(x, le, xf, lo, hi):
    """the loop start in [lo, hi) whose last xf samples look most like the loop end's (correlation, on
    every 4th sample, every 2nd start: plain Python, no numpy): the crossfade then joins two copies of the
    same wave, not two phases that cancel"""
    tail = x[le - xf:le:4]
    tn = sum(v * v for v in tail) ** 0.5 + 1e-9
    best, bl = -2.0, (lo + hi) // 2
    for ls in range(lo, hi, 2):
        seg = x[ls - xf:ls:4]
        c = sum(a * b for a, b in zip(seg, tail)) / ((sum(v * v for v in seg) ** 0.5 + 1e-9) * tn)
        if c > best:
            best, bl = c, ls
    return bl


class Builder:
    def __init__(self):
        self.zones, self.sets, self.blob, self.kinds = [], [], bytearray(), {}

    def add(self, s, loop_start):
        """ADPCM-encode s into the blob (each sample starts on an even offset) -> (offset, state at loop_start)"""
        enc, st = ima_encode(s, loop_start)
        off = len(self.blob)
        self.blob += enc
        if len(self.blob) & 1:
            self.blob.append(0)
        return off, st

    def add_set(self, name, kind, entries):
        """entries: zone dicts with root16; assigns the key ranges"""
        if not entries:
            return
        entries.sort(key=lambda e: e["root16"])
        for e, (lo, hi) in zip(entries, key_split([e["root16"] // 16 for e in entries])):
            e["lo"], e["hi"] = lo, hi
        self.sets.append((name, len(self.zones), len(entries)))
        self.zones += entries
        self.kinds[name] = kind

    def cc0_set(self, name, kind):
        entries = []
        for _, s, loop, root in cc0_entries(name, kind):
            ls, le = loop if loop else (len(s), len(s))
            off, st = self.add(s, ls)
            entries.append(dict(off=off, n=len(s), ls=ls, le=le, looped=bool(loop), sr=TR,
                                root16=int(round(root * 16)), pred=st[0], idx=st[1]))
        self.add_set(name, kind, entries)

    def header(self):
        zones, sets, blob = self.zones, self.sets, self.blob
        L = ["/* generated by tools/gen_samples.py: IMA ADPCM sample sets (CC0: assets/samples-cc0/ATTRIBUTION.txt) */",
             "#pragma once", "#include <stdint.h>", ""]
        L.append(f"static const uint8_t SMP_DATA[{max(1, len(blob))}] = {{")
        for i in range(0, len(blob), 32):
            L.append("    " + ",".join(map(str, blob[i:i + 32])) + ",")
        L.append("};")
        L.append("static const smp_zone_t SMP_ZONES[] = {")
        for e in zones:
            rate = int(round(e["sr"] / 44100 * 65536))
            L.append(f"    {{{e['off']}, {e['n']}, {e['ls']}, {e['le']}, {rate}, {e['root16']}, {e['pred']}, "
                     f"{e['idx']}, {e['lo']}, {e['hi']}, {1 if e['looped'] else 0}}},")
        L.append("};")
        L.append("static const smp_set_t SMP_SETS[] = {")
        for name, z0, nz in sets:
            L.append(f'    {{"{name}", {z0}, {nz}}},')
        L.append("};")
        L.append(f"#define SMP_NSETS {len(sets)}")
        L.append("static const preset_t SMP_PRESET_TABLE[] = {")
        for i, (name, _, _) in enumerate(sets):
            pname, e, env, mono, fx = SET_PRESETS[name]
            e = list(e)
            e[0], e[3] = i, 1
            L.append(f'    {{"{pname}", {{{", ".join(map(str, e))}}}, {{{", ".join(map(str, env))}}}, 0, {mono}, '
                     f'FX({", ".join(map(str, fx))})}},')
        L.append("};")
        names = ", ".join(f'"{n}"' for n, _, _ in sets)
        L.append("#define SMP_SET_NAMES_INIT " + names)
        L.append("static const char *const SMP_SET_NAMES[] = {" + names + "};")
        return "\n".join(L) + "\n"

    def summary(self):
        return f"samples: {len(self.sets)} sets, {len(self.zones)} zones, {len(self.blob)} B ADPCM"


def build():
    """-> (header text, summary)"""
    b = Builder()
    for name, kind in CC0_SETS:
        b.cc0_set(name, kind)
    return b.header(), b.summary()


def main(out):
    text, summary = build()
    Path(out).write_text(text)
    print(summary)


if __name__ == "__main__":
    main(sys.argv[1])
