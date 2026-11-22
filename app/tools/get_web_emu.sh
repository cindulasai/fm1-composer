#!/bin/sh
# Builds fm1-emu's browser version (Joakim Eriksson, MIT), unmodified, at the pinned commit, into studio/web/emu/:
# its whole page (index.html, main.js, the wasm, ...), its LICENSE, and BUILD.txt (the commit and every file's sha256).
# The source is kept in ../vendor/fm1-emu-src ($FM1_EMU_SRC picks another clone).
set -eu
cd "$(dirname "$0")/.."
REPO=https://github.com/joakimeriksson/fm1-emu
COMMIT=588db9e988ea818951dcf8fda8fdd11d5fcc77eb
SRC=${FM1_EMU_SRC:-../vendor/fm1-emu-src}
OUT=studio/web/emu
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
miss=""
for t in git bash cargo rustup; do command -v "$t" >/dev/null 2>&1 || miss="$miss $t"; done
if command -v rustup >/dev/null 2>&1 && ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$'; then
    miss="$miss wasm32-target(rustup target add wasm32-unknown-unknown)"
fi
[ -z "$miss" ] || { echo "get_web_emu.sh: missing:$miss" >&2; exit 2; }
if [ ! -d "$SRC/.git" ]; then
    mkdir -p "$(dirname "$SRC")"
    git clone -q "$REPO" "$SRC"
fi
git -C "$SRC" cat-file -e "$COMMIT^{commit}" 2>/dev/null || git -C "$SRC" fetch -q origin
git -C "$SRC" checkout -q --detach "$COMMIT"
[ -z "$(git -C "$SRC" status --porcelain --untracked-files=no)" ] || { echo "get_web_emu.sh: $SRC has local changes; not building it" >&2; exit 2; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
bash "$SRC/web/build.sh" "$tmp/dist" > "$tmp/build.log" 2>&1 || { tail -n 20 "$tmp/build.log" >&2; exit 1; }
mkdir -p "$OUT"
rm -rf "$OUT"
mkdir -p "$OUT"
for f in index.html style.css main.js persistence.js tryout.js worklet.js jit.js fwsc.js firmwares.json og.png fm1_web.wasm; do
    cp "$tmp/dist/$f" "$OUT/"
done
cp -R "$tmp/dist/fonts" "$OUT/fonts"
cp "$SRC/LICENSE" "$OUT/LICENSE"
if command -v sha256sum >/dev/null 2>&1; then sum=$(sha256sum "$OUT/fm1_web.wasm" | cut -d' ' -f1); else sum=$(shasum -a 256 "$OUT/fm1_web.wasm" | cut -d' ' -f1); fi
printf 'fm1-emu %s\ncommit %s\nfm1_web.wasm sha256 %s\n' "$REPO" "$COMMIT" "$sum" > "$OUT/BUILD.txt"
for f in index.html style.css main.js persistence.js tryout.js worklet.js jit.js fwsc.js firmwares.json; do   # its page, unchanged
    printf '%s sha256 %s\n' "$f" "$(if command -v sha256sum >/dev/null 2>&1; then sha256sum "$OUT/$f"; else shasum -a 256 "$OUT/$f"; fi | cut -d' ' -f1)" >> "$OUT/BUILD.txt"
done
echo "fm1-emu's browser build (commit $COMMIT) in $OUT"
