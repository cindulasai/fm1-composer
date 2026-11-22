"""fm1c command line. Exit codes: 0 done, 1 the FM-1 refused or failed, 2 a usage, file or tool problem."""
from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile

from . import emu as emumod
from . import fit as fitmod
from . import link as linkmod
from .piece import PieceError, decode, encode

DEFAULT_T2M = os.path.expanduser("~/Claude/Projects/text2midi_v2/target/release/t2m-cli")


EPILOG = ("COMPOSER plays up to 8 channels on 16 voices (measured in fm1-emu, not yet on a real FM-1). "
          "Inspired by SLOOP (isod89/sloop-fm1); built on Dinghy, Felucca.")


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
        mask = os.umask(0)
        os.umask(mask)
        os.fchmod(fd, 0o666 & ~mask)                  # (mkstemp makes it 0600: a normal file instead)
        with os.fdopen(fd, "wb") as f:
            f.write(data)
        os.replace(tmp, out)                          # whole or not at all
    except BaseException:
        if os.path.exists(tmp):
            os.unlink(tmp)
        raise


def _convert(mid: str, out: str, name: str | None, force: bool, poly: int) -> int:
    piece, rep = fitmod.fit(mid, name, fitmod.Limits(poly=poly))
    if not any(c.notes for c in piece.channels):
        raise UsageError(f"{mid}: no notes left to play after fitting it to the FM-1 ({'; '.join(rep.lines)})")
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


def _link(a):
    return linkmod.Link(linkmod.open_port(a.port), timeout=a.timeout)


def cmd_push(a) -> int:
    pieces = []
    for path in a.files:
        with open(path, "rb") as f:
            data = f.read(linkmod.CHUNK * 129)
        try:
            pieces.append((path, data, decode(data).name))
        except PieceError as e:
            raise UsageError(f"{path}: {e}") from e
    port = linkmod.open_port(a.port)
    lk, pushed, verified, refused, checked = linkmod.Link(port, timeout=a.timeout), 0, 0, 0, 0
    try:
        for path, data, name in pieces:
            try:
                how, pid = lk.put(data)
                if how == "staged":
                    print(f"{name}: staged — saves when playback stops")
                    if a.no_wait:
                        pushed += 1
                        continue
                    pid = lk.wait_saved(a.wait)
                    print(f"{name}: saved")
                pushed += 1
                ok = lk.get(pid, len(data)) == data
                checked += 1
                verified += ok
                print(f"{name}: #{pid}, {len(data)} bytes, {'verified' if ok else 'READ BACK DIFFERS'}")
            except linkmod.LinkError as e:
                refused += 1
                print(f"{name}: refused — {e}", file=sys.stderr)
    finally:
        port.close()
    print(f"pushed {pushed}, verified {verified}" + (f", refused {refused}" if refused else ""))
    return 1 if refused or verified < checked else 0


def cmd_list(a) -> int:
    lk = _link(a)
    try:
        info, items = lk.info(), lk.list()
    finally:
        lk.port.close()
    for it in items:
        print(f"#{it['id']:<5} {it['name']:<24} {it['len'] / 1024:5.1f} KB")
    print(f"{len(items)} piece{'s' if len(items) != 1 else ''}, {info['free'] // 1024} KB free"
          + (", playing" if info["playing"] else "") + (", one piece waiting to be saved" if info["staged"] else ""))
    return 0


def _piece_id(v: str) -> int:
    try:
        n = int(v)
    except ValueError:
        n = -1
    if not 0 <= n <= 0xFFFFFFFF:
        raise argparse.ArgumentTypeError(f"'{v}' is not a piece id (the # that fm1c list shows)")
    return n


def cmd_delete(a) -> int:
    lk = _link(a)
    try:
        lk.delete(a.id)
    finally:
        lk.port.close()
    print(f"deleted #{a.id}")
    return 0


def cmd_pull(a) -> int:
    lk = _link(a)
    try:
        hit = [it for it in lk.list() if it["id"] == a.id]
        if not hit:
            raise linkmod.LinkError(10, f"no piece #{a.id}")
        data = lk.get(a.id, hit[0]["len"])
    finally:
        lk.port.close()
    decode(data)                                                  # what came back is a whole piece
    out = a.out or f"{hit[0]['name'] or 'piece'}.f1p"
    _write(out, data, a.force)
    print(f"#{a.id} -> {out} ({len(data)} bytes)")
    return 0


def cmd_emu(a) -> int:
    return emumod.run(a.fwsc, a.flash)


def _port_args(x):
    x.add_argument("--port", help="MIDI port (part of its name; default Felucca or FM-1, or $FM1C_PORT)")
    x.add_argument("--timeout", type=float, default=3.0, help=argparse.SUPPRESS)


def build_parser() -> argparse.ArgumentParser:
    p = _Parser(prog="fm1c", description="Compose, convert and push pieces for COMPOSER on the M-VAVE FM-1.",
                epilog=EPILOG)
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
    u = sub.add_parser("push", help="send .f1p pieces to the FM-1's library (each read back to verify)")
    u.add_argument("files", nargs="+")
    u.add_argument("--no-wait", action="store_true", help="do not wait while a piece is staged during playback")
    u.add_argument("--wait", type=float, default=600.0, help=argparse.SUPPRESS)
    _port_args(u)
    u.set_defaults(fn=cmd_push)
    li = sub.add_parser("list", help="the pieces in the FM-1's library")
    _port_args(li)
    li.set_defaults(fn=cmd_list)
    d = sub.add_parser("delete", help="remove a piece from the FM-1's library by its #id")
    d.add_argument("id", type=_piece_id)
    _port_args(d)
    d.set_defaults(fn=cmd_delete)
    g = sub.add_parser("pull", help="copy a piece from the FM-1 back to this computer")
    g.add_argument("id", type=_piece_id)
    g.add_argument("-o", "--out")
    g.add_argument("--force", action="store_true")
    _port_args(g)
    g.set_defaults(fn=cmd_pull)
    e = sub.add_parser("emu", help="run COMPOSER in fm1-emu with USB, so push / list / ... reach it")
    e.add_argument("--fwsc", help="the firmware (default app/build/composer.fwsc, or $FM1C_FWSC)")
    e.add_argument("--flash", help=f"the emulated flash, the library in it (default {emumod.FLASH})")
    e.set_defaults(fn=cmd_emu)
    return p


def main(argv: list[str] | None = None) -> int:
    try:
        a = build_parser().parse_args(argv)
        if not getattr(a, "fn", None):
            raise UsageError("a command is needed: convert, compose, push, list, delete, pull, emu (fm1c --help)")
        return a.fn(a)
    except UsageError as e:
        print(f"fm1c: {e}", file=sys.stderr)
        return 2
    except (fitmod.FitError, PieceError, linkmod.PortError, emumod.EmuError) as e:
        print(f"fm1c: {e}", file=sys.stderr)
        return 2
    except linkmod.LinkError as e:
        print(f"fm1c: the FM-1 refused: {e}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("fm1c: interrupted", file=sys.stderr)
        return 2
    except OSError as e:
        print(f"fm1c: {e.filename or ''}: {e.strerror}", file=sys.stderr)
        return 2
