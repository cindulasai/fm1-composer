#!/bin/sh
# The gate: C unit tests (host), then pytest (unit, e2e through host mode and the fm1c CLI, hostile, property),
# then the CI-built firmware in fm1-emu (tests/emu_e2e.sh).
set -eu
cd "$(dirname "$0")"
PY=${PY:-}
if [ -z "$PY" ]; then
    if [ -x ../.venv/bin/python ]; then PY=../.venv/bin/python; else PY=python3; fi
fi
for t in tests/*_test.c; do
    [ -f "$t" ] || continue
    n=$(basename "$t" .c)
    mkdir -p build/unit
    cc -O1 -g -Wall -Werror -Wno-unused-function $(sh tests/sanflags.sh) -Ifirmware/src -Ifirmware/hal -Iengines -o "build/unit/$n" "$t" -lm
    "build/unit/$n" > "build/unit/$n.log" 2>&1 || { tail -n 20 "build/unit/$n.log"; exit 1; }
done
mkdir -p build
if "$PY" -m pytest -q -x -p no:cacheprovider tools/tests > build/pytest.log 2>&1; then
    tail -n 2 build/pytest.log
else
    tail -n 40 build/pytest.log; exit 1
fi
sh tests/emu_e2e.sh                             # the real firmware in fm1-emu (or one SKIP line saying what is missing)
