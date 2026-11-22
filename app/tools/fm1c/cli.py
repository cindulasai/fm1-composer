"""fm1c command line. Exit codes: 0 done, 1 the FM-1 refused or failed, 2 a usage, file or tool problem."""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile

from . import fit as fitmod
from .piece import PieceError, encode

DEFAULT_T2M = os.path.expanduser("~/Claude/Projects/text2midi_v2/target/release/t2m-cli")


class UsageError(Exception):
    pass


class _Parser(argparse.ArgumentParser):
    def error(self, message):                       # one line, exit 2, no usage dump
        raise UsageError(message)


def _write(out: str, data: bytes, force: bool) -> None:
    if os.path.isdir(out):
        raise UsageError(f"{out}: is a folder")
    if os.path.exists(out) and not force:
        raise UsageError(f"{out}: already exists (use --force to replace it)")
    d = os.path.dirname(os.path.abspath(out))
    if not os.path.isdir(d):
        raise UsageError(f"{d}: no such folder")
    fd, tmp = tempfile.mkstemp(dir=d, prefix=".fm1c-")
    try:
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.replace(tmp, out)                          # whole or not at all
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


def _convert(mid: str, out: str, name: str | None, force: bool, poly: int) -> int:
    piece, rep = fitmod.fit(mid, name, fitmod.Limits(poly=poly))
    data = encode(piece)
    _write(out, data, force)
    notes = sum(len(c.notes) for c in piece.channels)
    print(f"{out}: '{piece.name}', {len(piece.channels)} channels, {notes} notes, "
          f"{piece.len_ticks // 384} bars, {piece.tempo_x10 / 10:g} BPM, {len(data)} bytes, "
          f"{rep.reductions} reductions")
    for line in rep.lines:
        print(f"  - {line}")
    return 0


def cmd_convert(a) -> int:
    out = a.out or os.path.splitext(os.path.basename(a.midi))[0] + ".f1p"
    return _convert(a.midi, out, a.name, a.force, a.poly)


def cmd_compose(a) -> int:
    t2m = a.t2m or os.environ.get("FM1C_T2M") or DEFAULT_T2M
    if not (os.path.isfile(t2m) and os.access(t2m, os.X_OK)):
        raise UsageError(f"text2midi's t2m-cli not found at {t2m} (build it: cargo build --release -p t2m-cli, "
                         "or pass --t2m PATH)")
    if not os.path.isfile(a.brief):
        raise UsageError(f"{a.brief}: no such brief")
    stem = os.path.splitext(os.path.basename(a.brief))[0]
    out = a.out or stem + ".f1p"
    with tempfile.TemporaryDirectory(prefix="fm1c-") as tmp:
        r = subprocess.run([t2m, "export", a.brief, "--out", tmp, "--seed", str(a.seed), "--profile", "approx",
                            "--name", "piece"], capture_output=True, text=True)
        if r.returncode != 0:
            msg = (r.stderr.strip().splitlines() or ["(no message)"])[-1]
            raise UsageError(f"text2midi could not compose {a.brief}: {msg}")
        mid = os.path.join(tmp, "piece.mid")
        if not os.path.isfile(mid):
            raise UsageError(f"text2midi wrote no piece.mid for {a.brief}")
        return _convert(mid, out, a.name or stem, a.force, a.poly)


def build_parser() -> argparse.ArgumentParser:
    p = _Parser(prog="fm1c", description="Compose, convert and push pieces for COMPOSER on the M-VAVE FM-1.")
    sub = p.add_subparsers(dest="cmd", parser_class=_Parser)
    c = sub.add_parser("convert", help="a MIDI file -> a .f1p piece fitted to the FM-1")
    c.add_argument("midi")
    c.add_argument("-o", "--out")
    c.add_argument("--name")
    c.add_argument("--force", action="store_true")
    c.add_argument("--poly", type=int, default=6, choices=range(1, 17), metavar="1..16")
    c.set_defaults(fn=cmd_convert)
    k = sub.add_parser("compose", help="a text2midi brief -> a .f1p piece (runs text2midi's t2m-cli)")
    k.add_argument("brief")
    k.add_argument("-o", "--out")
    k.add_argument("--seed", type=int, default=1)
    k.add_argument("--name")
    k.add_argument("--t2m")
    k.add_argument("--force", action="store_true")
    k.add_argument("--poly", type=int, default=6, choices=range(1, 17), metavar="1..16")
    k.set_defaults(fn=cmd_compose)
    return p


def main(argv: list[str] | None = None) -> int:
    try:
        a = build_parser().parse_args(argv)
        if not getattr(a, "fn", None):
            raise UsageError("a command is needed: convert, compose (fm1c --help)")
        return a.fn(a)
    except UsageError as e:
        print(f"fm1c: {e}", file=sys.stderr)
        return 2
    except (fitmod.FitError, PieceError) as e:
        print(f"fm1c: {e}", file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("fm1c: interrupted", file=sys.stderr)
        return 2
    except OSError as e:
        print(f"fm1c: {e.filename or ''}: {e.strerror}", file=sys.stderr)
        return 2
