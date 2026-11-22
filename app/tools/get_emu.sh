#!/bin/sh
# FM1_EMU_DIR / FM1_EMU_URL / FM1_EMU_SUM override the folder, the archive and its checksum (a mirror, tests).
# Download fm1-emu (joakimeriksson/fm1-emu, MIT) for this computer (macOS arm64, Linux) into vendor/fm1-emu, checked against a pinned SHA-256.
set -eu
cd "$(dirname "$0")/../.."
V=0.5.1
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) A=fm1-emu-$V-macos-arm64.tar.gz; SUM=b5df55e1450210ae88be4ddfa903e608fa495a58bf72775d30047308e48b9895 ;;
  Linux-x86_64) A=fm1-emu-$V-linux-x86_64.tar.gz; SUM=2b4b5cc89bc42edc91dc64830b912633bde35b5fb8a66c173341ed9a851917b7 ;;
  Linux-aarch64) A=fm1-emu-$V-linux-arm64.tar.gz; SUM=3e8f50b6eb53247db7e91040d90008a7eea2c28d368821a0783d166f5ca8cbcf ;;
  *) echo "get_emu.sh: no pinned fm1-emu build for $(uname -s)-$(uname -m)" >&2; exit 2 ;;
esac
D=${FM1_EMU_DIR:-vendor/fm1-emu}
URL=${FM1_EMU_URL:-https://github.com/joakimeriksson/fm1-emu/releases/download/v$V/$A}
SUM=${FM1_EMU_SUM:-$SUM}
[ -x "$D/fm1-live" ] && { echo "$D/fm1-live"; exit 0; }
mkdir -p "$D"; T=$(mktemp -d)
curl -fsSL --retry 3 -o "$T/$A" "$URL" || { echo "get_emu.sh: download failed: $URL" >&2; rm -rf "$T"; exit 1; }
echo "$SUM  $T/$A" | shasum -a 256 -c - >/dev/null || { echo "get_emu.sh: checksum mismatch" >&2; rm -rf "$T"; exit 1; }
tar -xzf "$T/$A" -C "$T"; rm "$T/$A"
find "$T" -type f \( -name fm1-live -o -name fm1-emu \) -exec mv {} "$D/" \;
rm -rf "$T"
[ -x "$D/fm1-live" ] || { echo "get_emu.sh: fm1-live not in the archive" >&2; exit 1; }
echo "$D/fm1-live"
