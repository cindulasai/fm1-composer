"""fm1c command line. Exit codes: 0 done, 1 the FM-1 refused or failed, 2 a usage, file or tool problem."""
from __future__ import annotations

import argparse
import functools
import json
import http.server
import os
import shutil
import subprocess
import sys
import tempfile
import webbrowser

from . import emu as emumod
from . import fit as fitmod
from . import link as linkmod
from .piece import PieceError, decode, encode

STUDIO = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "studio")
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


def _convert(mid: str, out: str, name: str | None, force: bool, poly: int, key=None) -> int:
    piece, rep = fitmod.fit(mid, name, fitmod.Limits(poly=poly), key)
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
        try:
            with open(a.brief, encoding="utf-8") as f:
                key = fitmod.brief_key(json.load(f))
        except (OSError, ValueError):
            key = None                   # (text2midi composed it, so it is read; else the FM-1 guesses the key)
        return _convert(mid, out, a.name or stem, a.force, a.poly, key)


STUDIO_CABLE = '<script src="../cable.js"></script>\n'
COMPOSER_ENTRY = {"id": "composer", "name": "COMPOSER", "version": "", "author": "cindulasai", "license": "GPL-3.0-only",
                  "about": "Plays whole arrangements, up to 8 parts, made from a sentence in COMPOSER Studio (this site's home page) and sent in over MIDI IN: then play the keys in key, edit notes, sequence steps, record.",
                  "repo": "https://github.com/cindulasai/fm1-composer", "package": "../firmware/composer.fwsc"}


def _fwsc() -> str:
    """the firmware CI built (or $FM1C_FWSC)"""
    return os.environ.get("FM1C_FWSC") or os.path.join(os.path.dirname(STUDIO), "build", "composer.fwsc")


def _cabled(page: str) -> str:
    """fm1-emu's own page with Studio's MIDI cable: one script before its own"""
    at = page.index("<script")
    return page[:at] + STUDIO_CABLE + page[at:]


def _composer_first(catalog: dict) -> str:
    """fm1-emu's firmware list with COMPOSER first and the default"""
    catalog["firmwares"] = [COMPOSER_ENTRY] + [x for x in catalog["firmwares"] if x.get("id") != "composer"]
    catalog["default"] = "composer"
    return json.dumps(catalog)


def _read(path: str) -> str:
    with open(path, encoding="utf-8") as f:
        return f.read()


class _StudioHandler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm",
                      ".js": "text/javascript", ".mjs": "text/javascript"}

    def translate_path(self, path):
        if path.split("?", 1)[0] == "/firmware/composer.fwsc":
            return _fwsc()
        return super().translate_path(path)

    def do_GET(self):
        """fm1-emu's page with the cable and its firmware list with COMPOSER first; every other file as it is"""
        path = self.path.split("?", 1)[0]
        emu = os.path.join(self.directory, "emu")
        if path in ("/emu/", "/emu/index.html") and os.path.isfile(os.path.join(emu, "index.html")):
            return self._send(_cabled(_read(os.path.join(emu, "index.html"))), "text/html; charset=utf-8")
        if path == "/emu/firmwares.json" and os.path.isfile(os.path.join(emu, "firmwares.json")):
            return self._send(_composer_first(json.loads(_read(os.path.join(emu, "firmwares.json")))), "application/json")
        return super().do_GET()

    def _send(self, text, ctype):
        body = text.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def end_headers(self):
        self.send_header("Cache-Control", "no-store")
        super().end_headers()

    def log_message(self, *args):
        pass


def _built(web: str) -> bool:
    return os.path.isfile(os.path.join(web, "engine", "studio.js")) and os.path.isfile(os.path.join(web, "t2m", "index.html"))


def _export(out: str, force: bool) -> int:
    """Studio as static files: what `fm1c studio` serves, its rewrites baked in; never local/ (the AI key) or dotfiles"""
    web = os.path.realpath(os.environ.get("FM1C_STUDIO_WEB") or os.path.join(STUDIO, "web"))   # (tests: a copy)
    emu = os.path.join(web, "emu")
    if not _built(web):
        raise UsageError(f"COMPOSER Studio is not built in {web}: run sh app/studio/build.sh (needs Rust and npm)")
    if not (os.path.isfile(os.path.join(emu, "index.html")) and os.path.isfile(os.path.join(emu, "firmwares.json"))):
        raise UsageError(f"fm1-emu's page is not in {emu}: run sh app/tools/get_web_emu.sh")
    fwsc = _fwsc()
    if not os.path.isfile(fwsc):
        raise UsageError(f"no COMPOSER firmware at {fwsc}: put CI's composer-fwsc artifact there (or set FM1C_FWSC)")
    if os.path.exists(out) and not os.path.isdir(out):
        raise UsageError(f"{out}: is a file, not a folder")
    real = os.path.realpath(out)
    if real == web or web.startswith(real + os.sep) or real.startswith(web + os.sep):
        raise UsageError(f"{out}: is Studio's own folder (or holds it); export somewhere else")
    if os.path.isdir(out):
        if not os.access(out, os.W_OK | os.X_OK):
            raise UsageError(f"{out}: cannot write there")
        if os.listdir(out):
            if not force:
                raise UsageError(f"{out} is not empty (--force replaces it)")
            for name in os.listdir(out):
                p = os.path.join(out, name)
                if os.path.isdir(p) and not os.path.islink(p):
                    shutil.rmtree(p)
                else:
                    os.unlink(p)
    os.makedirs(out, exist_ok=True)
    secret, n = os.path.join(web, "local"), 0
    for d, dirs, files in os.walk(web):
        dirs[:] = sorted(x for x in dirs if not x.startswith(".") and os.path.join(d, x) != secret)
        rel = os.path.relpath(d, web)
        os.makedirs(os.path.join(out, rel), exist_ok=True)
        for name in sorted(files):
            src, dst = os.path.join(d, name), os.path.normpath(os.path.join(out, rel, name))
            if name.startswith(".") or os.path.realpath(src).startswith(secret + os.sep):   # (no link to the key)
                continue
            if src == os.path.join(emu, "index.html"):
                _write(dst, _cabled(_read(src)).encode("utf-8"), True)
            elif src == os.path.join(emu, "firmwares.json"):
                _write(dst, _composer_first(json.loads(_read(src))).encode("utf-8"), True)
            else:
                shutil.copyfile(src, dst)
            n += 1
    os.makedirs(os.path.join(out, "firmware"), exist_ok=True)
    shutil.copyfile(fwsc, os.path.join(out, "firmware", "composer.fwsc"))
    print(f"COMPOSER Studio exported to {out}: {n + 1} files; any static web server can serve it")
    return 0


def cmd_studio(a) -> int:
    if a.export is not None:
        return _export(a.export, a.force)
    if a.force:
        raise UsageError("--force goes with --export DIR")
    if not 0 <= a.port <= 65535:
        raise UsageError(f"--port must be 0 to 65535, not {a.port}")
    web = os.path.join(STUDIO, "web")
    if not _built(web):
        print("building COMPOSER Studio (once; needs Rust and npm) ...", flush=True)
        if subprocess.run(["sh", os.path.join(STUDIO, "build.sh")]).returncode != 0:
            raise UsageError("COMPOSER Studio's engine could not be built (see above)")
    try:
        srv = http.server.ThreadingHTTPServer(("127.0.0.1", a.port), functools.partial(_StudioHandler, directory=web))
    except OSError as e:
        raise UsageError(f"cannot serve COMPOSER Studio on 127.0.0.1:{a.port}: {e.strerror}") from None
    url = f"http://127.0.0.1:{srv.server_address[1]}/"
    print(f"COMPOSER Studio: {url}  (only this computer can open it; Ctrl-C stops it)", flush=True)
    if not a.no_open:
        webbrowser.open(url)
    try:
        srv.serve_forever()
    except KeyboardInterrupt:
        pass
    finally:
        srv.server_close()
    return 0


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
    waiting, replaced = None, 0                 # (--no-wait: the piece staged before, not saved yet)
    try:
        for path, data, name in pieces:
            try:
                how, pid = lk.put(data)
                if how == "staged":
                    # decision 55: the newer of two pieces sent while playing wins, and a live piece is saved only
                    # when playback stops, so the one staged before this is never saved
                    if waiting is not None:
                        replaced += 1
                        print(f"{waiting}: replaced by {name} before it was saved, not in LIBRARY", file=sys.stderr)
                    print(f"{name}: staged — takes over at the next bar (NEXT: {name}, then LIVE: {name}), "
                          "saved when playback stops")
                    if a.no_wait:
                        pushed += 1
                        waiting = name
                        continue
                    pid = lk.wait_saved(a.wait)
                    print(f"{name}: saved")
                waiting = None
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
    print(f"pushed {pushed}, verified {verified}" + (f", refused {refused}" if refused else "")
          + (f", replaced {replaced}" if replaced else ""))
    return 1 if refused or replaced or verified < checked else 0


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
    u.add_argument("--no-wait", action="store_true", help="do not wait while a piece is staged during playback (a later one replaces it)")
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
    s = sub.add_parser("studio", help="COMPOSER Studio in your browser: an idea in plain English -> a piece")
    s.add_argument("--port", type=int, default=8714, help="the port on 127.0.0.1 (default 8714; 0 = any free one)")
    s.add_argument("--no-open", action="store_true", help="print the address; do not open a browser")
    s.add_argument("--export", metavar="DIR", help="instead, write Studio as static files into DIR (with the firmware, "
                   "app/build/composer.fwsc or $FM1C_FWSC) for any web server; never your AI key")
    s.add_argument("--force", action="store_true", help="with --export: replace what DIR holds")
    s.set_defaults(fn=cmd_studio)
    return p


def main(argv: list[str] | None = None) -> int:
    try:
        a = build_parser().parse_args(argv)
        if not getattr(a, "fn", None):
            raise UsageError("a command is needed: convert, compose, push, list, delete, pull, emu, studio (fm1c --help)")
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
