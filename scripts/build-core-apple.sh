#!/usr/bin/env bash
# Usage: scripts/build-core-apple.sh   (normally via `make core-apple`)
# Builds the Rust bindings crate for iOS device, iOS simulator and macOS
# (arm64) and packages the static libraries into
#   apple/Generated/pkmCoreFFI.xcframework
# next to the generated Swift bindings
#   apple/Generated/pkmCore.swift
# Both are build artefacts (git-ignored) that the Xcode project links/compiles.
set -euo pipefail

cd "$(dirname "$0")/.."
out=apple/Generated
lib=libpkm_bindings.a
targets=(aarch64-apple-darwin aarch64-apple-ios aarch64-apple-ios-sim)

# Keep in sync with deploymentTarget in apple/project.yml.
export IPHONEOS_DEPLOYMENT_TARGET=26.0
export MACOSX_DEPLOYMENT_TARGET=27.0

installed=$(rustup target list --installed)
for t in "${targets[@]}"; do
  grep -qx "$t" <<<"$installed" || rustup target add "$t"
done

for t in "${targets[@]}"; do
  cargo build --release --locked -p pkm-bindings --target "$t"
done

# The generator runs on the host and reads the interface metadata embedded in
# the library, so the Swift code always matches the compiled Rust.
cargo build --release --locked -p pkm-bindings --features bindgen --bin uniffi-bindgen
rm -rf "$out"
mkdir -p "$out/headers"
target/release/uniffi-bindgen generate --language swift --metadata-no-deps \
  --out-dir "$out" "target/aarch64-apple-darwin/release/$lib"

# xcodebuild expects the module map to be called module.modulemap.
mv "$out"/*FFI.h "$out/headers/"
mv "$out"/*FFI.modulemap "$out/headers/module.modulemap"

args=()
for t in "${targets[@]}"; do
  args+=(-library "target/$t/release/$lib" -headers "$out/headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$out/pkmCoreFFI.xcframework" >/dev/null
rm -rf "$out/headers"
