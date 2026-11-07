#!/usr/bin/env bash
# Builds crates/web for the browser into web/generated: cargo for
# wasm32-unknown-unknown with the `web` profile, then `wasm-bindgen --target web`.
# The CLI must be the version of the `wasm-bindgen` crate in Cargo.lock; it is
# installed (once per version) if missing.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/[^0-9.]/, ""); print; exit}' Cargo.lock)
if [ "$(wasm-bindgen --version 2>/dev/null | awk '{print $2}')" != "$version" ]; then
    cargo install --locked wasm-bindgen-cli --version "$version"
fi
rustup target add wasm32-unknown-unknown >/dev/null

cargo build --locked -q --profile web --target wasm32-unknown-unknown -p excalidraw-native-web
rm -rf web/generated
mkdir -p web/generated
wasm-bindgen --target web --out-dir web/generated \
    target/wasm32-unknown-unknown/web/excalidraw_native_web.wasm
# Only the JS needs the wasm's types; the bundler treats the .wasm as an asset.
rm web/generated/excalidraw_native_web_bg.wasm.d.ts
cp apple/ExcalidrawNative/Resources/sample.excalidraw web/generated/
# The font is served as is: the web app reads the TTF itself (no woff2).
cp apple/ExcalidrawNative/Resources/Fonts/Excalifont-Regular.ttf web/generated/
