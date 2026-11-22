#!/bin/sh
# The gate: C unit tests (host), then pytest (unit, e2e through host mode and the fm1c CLI, hostile, property),
# then the CI-built firmware in fm1-emu (tests/emu_e2e.sh).
set -eu
cd "$(dirname "$0")"
PY=${PY:-}
if [ -z "$PY" ]; then
    if [ -x ../.venv/bin/python ]; then PY=../.venv/bin/python; else PY=python3; fi
fi
"$PY" -c "import mido, pytest, hypothesis, rtmidi, PIL" 2>/dev/null || {
    echo "check.sh: $PY lacks the Python packages; from app/: python3 -m venv ../.venv && ../.venv/bin/pip install -r tools/requirements.txt" >&2
    exit 2
}
"$PY" -c "import playwright" 2>/dev/null || {
    echo "check.sh: $PY lacks playwright (Studio's browser tests); from app/: ../.venv/bin/pip install -r tools/requirements.txt && ../.venv/bin/python -m playwright install chromium-headless-shell" >&2
    exit 2
}
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH" && export PATH
command -v cargo >/dev/null 2>&1 || {
    echo "check.sh: cargo not found; COMPOSER Studio's engine is Rust: https://rustup.rs, then rustup target add wasm32-unknown-unknown && cargo install wasm-bindgen-cli --version 0.2.130" >&2
    exit 2
}
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
if [ ! -f build/composer.fwsc ] || [ ! -f studio/web/emu/fm1_web.wasm ]; then    # (Studio's FM-1 tests skipped)
    echo "studio emu e2e: SKIP (needs build/composer.fwsc from CI and studio/web/emu: sh tools/get_web_emu.sh)"
fi
sh tests/emu_e2e.sh                             # the real firmware in fm1-emu (or one SKIP line saying what is missing)
