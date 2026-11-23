#!/bin/sh
# Run COMPOSER in host mode with a script: build/host/<name>.log (stdout: SCREEN lines, saves, MIDI out),
# build/host/<name>.wav, build/host/<name>-<ms>.ppm.   usage: host_e2e.sh <script-name> [extra cc flags]
set -eu
cd "$(dirname "$0")/.."
name=${1:-}
case "$name" in
    /*.txt) script=$name; name=$(basename "$name" .txt) ;;     # (a script written by a test)
    *) script=tests/scripts/$name.txt ;;
esac
[ -n "$name" ] && [ -f "$script" ] || { echo "unknown script: ${name:-<none>} (tests/scripts/*.txt)" >&2; exit 2; }
shift
mkdir -p build/host
bin=build/host/composer
tmp=$(mktemp build/host/.composer.XXXXXX)
san=${SANFLAGS:-$(sh tests/sanflags.sh)}                     # (out-of-bounds stops the run, never passes)
if ! cc -O1 -g -Wall -Wno-unused-function -DDINGHY_HOST -DAPP_VER='"dev"' $san ${HOST_CFLAGS:-} "$@" \
    -Ifirmware/hal -Ifirmware/src -Ifirmware/gen -Iengines -o "$tmp" firmware/src/composer.c; then
    rm -rf "$tmp" "$tmp.dSYM"; exit 1
fi
mv "$tmp" "$bin"
rm -rf "$bin.dSYM"                                           # (macOS cc -g writes <out>.dSYM: keep one, beside the binary)
if [ -d "$tmp.dSYM" ]; then mv "$tmp.dSYM" "$bin.dSYM"; fi
"$bin" "build/host/$name" "$script" > "build/host/$name.log"
