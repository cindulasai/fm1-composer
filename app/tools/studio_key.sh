#!/bin/sh
# Writes COMPOSER Studio's AI key file, studio/web/local/ai.json (git-ignored, mode 0600; $STUDIO_KEY_OUT picks
# another path), from $MINIMAX_API_KEY, else from MINIMAX_API_KEY= in text2midi_v2's .env ($T2M_ENV). The key is
# never printed. Only `fm1c studio` serves it, to this computer (127.0.0.1).
set -eu
cd "$(dirname "$0")/.."
out=${STUDIO_KEY_OUT:-studio/web/local/ai.json}
envf=${T2M_ENV:-$HOME/Claude/Projects/text2midi_v2/.env}
key=${MINIMAX_API_KEY:-}
if [ -z "$key" ] && [ -f "$envf" ]; then
    key=$(sed -n 's/^MINIMAX_API_KEY=//p' "$envf" | head -n 1 | tr -d "\"'\r ")
fi
[ -n "$key" ] || { echo "studio_key.sh: no key: set MINIMAX_API_KEY, or put MINIMAX_API_KEY=... in $envf" >&2; exit 2; }
case "$key" in *[!A-Za-z0-9._-]*) echo "studio_key.sh: the key has characters an API key does not have; not written" >&2; exit 2 ;; esac
if [ -z "${STUDIO_KEY_OUT:-}" ] && ! git check-ignore -q "$out" 2>/dev/null; then
    echo "studio_key.sh: $out is not git-ignored; not written" >&2; exit 2
fi
mkdir -p "$(dirname "$out")"
umask 077
printf '{"provider": "minimax", "key": "%s"}\n' "$key" > "$out.tmp"
chmod 600 "$out.tmp"
mv "$out.tmp" "$out"
echo "wrote $out (git-ignored, readable only by you)"
