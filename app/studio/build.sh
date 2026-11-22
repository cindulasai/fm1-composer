#!/bin/sh
# Builds COMPOSER Studio for the browser: the engine, web/engine/studio.js + studio_bg.wasm (Rust -> wasm32, wasm-bindgen
# 0.2.130), and text2midi's window, web/t2m/ (its Svelte code, unchanged, with Studio's stand-ins; node + npm).
set -eu
cd "$(dirname "$0")"
[ -d "$HOME/.cargo/bin" ] && PATH="$HOME/.cargo/bin:$PATH"
miss=""
command -v cargo >/dev/null 2>&1 || miss="$miss cargo (https://rustup.rs)"
command -v wasm-bindgen >/dev/null 2>&1 || miss="$miss wasm-bindgen (cargo install wasm-bindgen-cli --version 0.2.130)"
rustup target list --installed 2>/dev/null | grep -q '^wasm32-unknown-unknown$' || miss="$miss the wasm32 target (rustup target add wasm32-unknown-unknown)"
command -v npm >/dev/null 2>&1 || miss="$miss node and npm (https://nodejs.org)"
[ -z "$miss" ] || { echo "build.sh: missing:$miss" >&2; exit 2; }
cargo build -q --release --target wasm32-unknown-unknown -p studio --manifest-path engine/Cargo.toml
wasm-bindgen --target web --no-typescript --out-dir web/engine --out-name studio engine/target/wasm32-unknown-unknown/release/studio.wasm
[ -d t2m-ui/node_modules ] || (cd t2m-ui && npm ci --silent --no-audit --no-fund)
(cd t2m-ui && npx vite build --config vite.studio.config.js)
