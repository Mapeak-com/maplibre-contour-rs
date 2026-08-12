#!/usr/bin/env bash
# Build artifacts/MaplibreContourFFI.xcframework and the committed Swift wrapper
# (Sources/MaplibreContour/maplibre_contour_rs.swift) from the Rust crate.
# Requires rustup, cargo, Xcode.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$ROOT/build/apple"
ARTIFACTS="$ROOT/artifacts"
LIB="libmaplibre_contour_rs.a"
FRAMEWORK="MaplibreContourFFI"
FFI_MODULE="maplibre_contour_rsFFI"

# Apple-silicon only, device + simulator. The macOS slices and the Intel
# (x86_64) simulator slice were dropped deliberately: nothing consumes them and
# each one roughly doubled the download size of the published xcframework.
TARGETS=(
  aarch64-apple-ios
  aarch64-apple-ios-sim
)

rustup target add "${TARGETS[@]}"

for t in "${TARGETS[@]}"; do
  ( cd "$ROOT" && cargo build --release --lib --target "$t" )
done

HEADERS="$BUILD/headers"
rm -rf "$HEADERS"; mkdir -p "$HEADERS/$FFI_MODULE"
( cd "$ROOT" && cargo run --quiet --features cli --bin uniffi-bindgen -- generate \
    --library "target/aarch64-apple-ios/release/$LIB" \
    --language swift --out-dir "$BUILD/swift" --no-format )
cp "$BUILD/swift/maplibre_contour_rs.swift" "$ROOT/Sources/MaplibreContour/maplibre_contour_rs.swift"
cp "$BUILD/swift/maplibre_contour_rsFFI.h" "$HEADERS/$FFI_MODULE/"
cp "$BUILD/swift/maplibre_contour_rsFFI.modulemap" "$HEADERS/$FFI_MODULE/module.modulemap"

# Each slice is a single architecture now, so there is nothing left to `lipo`.
rm -rf "$ARTIFACTS/$FRAMEWORK.xcframework"; mkdir -p "$ARTIFACTS"
xcodebuild -create-xcframework \
  -library "$ROOT/target/aarch64-apple-ios/release/$LIB"     -headers "$HEADERS" \
  -library "$ROOT/target/aarch64-apple-ios-sim/release/$LIB" -headers "$HEADERS" \
  -output "$ARTIFACTS/$FRAMEWORK.xcframework"

echo "Wrote $ARTIFACTS/$FRAMEWORK.xcframework"
