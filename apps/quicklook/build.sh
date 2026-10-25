#!/bin/bash
# Baut die Vorschau-Erweiterung (Quick Look) für .forgeproj als ForgeQuickLook.appex.
#
#   apps/quicklook/build.sh [Ausgabeordner]
#
# Ohne Xcode-Projekt: swiftc baut die Erweiterung direkt, für Apple Silicon und Intel.
# Signiert wird mit $APPLE_SIGNING_IDENTITY (Developer ID), sonst ad hoc. Erweiterungen brauchen die Sandbox,
# deshalb gehört ForgeQuickLook.entitlements immer dazu.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
out="${1:-$here/build}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/../desktop/src-tauri/Cargo.toml" | head -1)"
appex="$out/ForgeQuickLook.appex"
min="12.0"

rm -rf "$appex"
mkdir -p "$appex/Contents/MacOS" "$out/obj"

for arch in arm64 x86_64; do
  swiftc -target "$arch-apple-macos$min" -swift-version 5 -O \
    -application-extension -parse-as-library -module-name ForgeQuickLook \
    -framework Cocoa -framework Quartz -framework UniformTypeIdentifiers -lcompression \
    -Xlinker -e -Xlinker _NSExtensionMain \
    -o "$out/obj/ForgeQuickLook-$arch" "$here"/Sources/*.swift
done
lipo -create "$out"/obj/ForgeQuickLook-arm64 "$out"/obj/ForgeQuickLook-x86_64 \
  -output "$appex/Contents/MacOS/ForgeQuickLook"

sed "s/__VERSION__/${version:-0.1.0}/g" "$here/Info.plist" > "$appex/Contents/Info.plist"

codesign --force --sign "${APPLE_SIGNING_IDENTITY:--}" --timestamp=none \
  --entitlements "$here/ForgeQuickLook.entitlements" \
  ${APPLE_SIGNING_IDENTITY:+--options runtime --timestamp} \
  "$appex"
echo "$appex"
