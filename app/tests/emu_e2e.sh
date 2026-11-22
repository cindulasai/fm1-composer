#!/bin/sh
# The real firmware (the .fwsc CI builds) in fm1-emu: tools/tests/test_emu.py. Missing either: one SKIP line, exit 0,
# never a silent pass.   FM1_LIVE: the emulator (default ../vendor/fm1-emu/fm1-live), FM1_FWSC: the firmware
# (default build/composer.fwsc; CI builds it, locally: gh run download -n composer-fwsc -D app/build)
set -eu
cd "$(dirname "$0")/.."
live=${FM1_LIVE:-../vendor/fm1-emu/fm1-live}
fwsc=${FM1_FWSC:-build/composer.fwsc}
if [ ! -x "$live" ]; then echo "emu e2e: SKIP (no fm1-live at $live: sh tools/get_emu.sh)"; exit 0; fi
if [ ! -f "$fwsc" ]; then echo "emu e2e: SKIP (no .fwsc at $fwsc: gh run download -n composer-fwsc -D app/build)"; exit 0; fi
PY=${PY:-}
if [ -z "$PY" ]; then
    if [ -x ../.venv/bin/python ]; then PY=../.venv/bin/python; else PY=python3; fi
fi
mkdir -p build
case "$live" in /*) ;; *) live=$PWD/$live ;; esac
case "$fwsc" in /*) ;; *) fwsc=$PWD/$fwsc ;; esac
if FM1_EMU_RUN=1 FM1_LIVE=$live FM1_FWSC=$fwsc "$PY" -m pytest -q -x -p no:cacheprovider tools/tests/test_emu.py \
        > build/emu.log 2>&1; then
    echo "emu e2e: $(tail -n 1 build/emu.log)"
else
    tail -n 40 build/emu.log; exit 1
fi
