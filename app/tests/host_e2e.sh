#!/bin/sh
# Run COMPOSER in host mode with a script: build/host/<name>.log (stdout: SCREEN lines, saves, MIDI out),
# build/host/<name>.wav, build/host/<name>-<ms>.ppm.   usage: host_e2e.sh <script-name> [extra cc flags]
set -eu
cd "$(dirname "$0")/.."
name=${1:-}
[ -n "$name" ] && [ -f "tests/scripts/$name.txt" ] || { echo "unknown script: ${name:-<none>} (tests/scripts/*.txt)" >&2; exit 2; }
shift
mkdir -p build/host
bin=build/host/composer
tmp=$(mktemp build/host/.composer.XXXXXX)
if ! cc -O1 -g -Wall -Wno-unused-function -DDINGHY_HOST -DAPP_VER='"dev"' ${HOST_CFLAGS:-} "$@" \
    -Ifirmware/hal -Ifirmware/src -Ifirmware/gen -Iengines -o "$tmp" firmware/src/composer.c; then
    rm -f "$tmp"; exit 1
fi
mv "$tmp" "$bin"
"$bin" "build/host/$name" "tests/scripts/$name.txt" > "build/host/$name.log"
