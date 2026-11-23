#!/bin/sh
# Print the sanitizer flags this machine can run: ASan + UBSan where an ASan binary works (Linux CI), else UBSan
# alone (some macOS sandboxes hang ASan's runtime at start). Cached in build/sanflags. SANFLAGS=... overrides.
# Left shifts of negative values and signed wrap-around are not stopped: SLOOP's fixed point (engines/fm6_core.c, kept
# unchanged) shifts signed samples and wraps its phases, as SLOOP's own sanitizer run allows (tests/run_tests.sh there).
set -eu
cd "$(dirname "$0")/.."
[ -n "${SANFLAGS:-}" ] && { echo "$SANFLAGS"; exit 0; }
[ -f build/sanflags ] && { cat build/sanflags; exit 0; }
mkdir -p build/san
printf 'int main(void){return 0;}\n' > build/san/t.c
F="-fsanitize=undefined,bounds -fno-sanitize=shift-base,signed-integer-overflow -fno-sanitize-recover=all"
if cc -fsanitize=address,undefined -o build/san/t build/san/t.c 2>/dev/null &&
   ( perl -e 'alarm 5; exec @ARGV' build/san/t ) >/dev/null 2>&1; then
    F="-fsanitize=address,undefined,bounds -fno-sanitize=shift-base,signed-integer-overflow -fno-sanitize-recover=all"
fi
echo "$F" | tee build/sanflags
