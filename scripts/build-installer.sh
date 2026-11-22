#!/bin/bash
# Baut Forge-<Version>.pkg: Forge.app + arduino-cli (universal) + ESP32-Core-Einrichtung per postinstall.
#
#   scripts/build-installer.sh            # nutzt die vorhandene target/release/bundle/macos/Forge.app
#
# Signieren (optional): INSTALLER_SIGNING_IDENTITY="Developer ID Installer: Name (TEAMID)"
# Notarisieren (optional): APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
app="${FORGE_APP:-$root/target/release/bundle/macos/Forge.app}"
work="$root/target/installer"
out="$root/target/release/bundle/Forge-$version.pkg"

[ -d "$app" ] || { echo "Forge.app fehlt: $app (erst 'pnpm --filter @forge/desktop tauri build --bundles app' ausführen)"; exit 1; }

rm -rf "$work"; mkdir -p "$work/app-root/Applications" "$work/tools-root/Library/Application Support/Forge/bin" "$work/dl"

# arduino-cli als universelle Binärdatei
for a in ARM64 64bit; do
  curl -fsSL "https://downloads.arduino.cc/arduino-cli/arduino-cli_latest_macOS_$a.tar.gz" -o "$work/dl/$a.tgz"
  mkdir -p "$work/dl/$a"; tar -xzf "$work/dl/$a.tgz" -C "$work/dl/$a" arduino-cli
done
lipo -create "$work/dl/ARM64/arduino-cli" "$work/dl/64bit/arduino-cli" \
  -output "$work/tools-root/Library/Application Support/Forge/bin/arduino-cli"
chmod 755 "$work/tools-root/Library/Application Support/Forge/bin/arduino-cli"
cp "$root/installer/LICENSE-arduino-cli.txt" "$work/tools-root/Library/Application Support/Forge/bin/" 2>/dev/null || true

# Komponenten-Pakete
ditto "$app" "$work/app-root/Applications/Forge.app"
sign=()
[ -n "${INSTALLER_SIGNING_IDENTITY:-}" ] && sign=(--sign "$INSTALLER_SIGNING_IDENTITY")
pkgbuild --root "$work/app-root" --identifier cc.forge.ide.app --version "$version" \
  --install-location / "$work/app.pkg"
pkgbuild --root "$work/tools-root" --identifier cc.forge.ide.tools --version "$version" \
  --install-location / --scripts "$root/installer/scripts" "$work/tools.pkg"

# Ein Paket, zwei Komponenten
mkdir -p "$work/pkgs"
cp "$work/app.pkg" "$work/tools.pkg" "$work/pkgs/"
python3 - "$root/installer/distribution.xml" "$work/distribution.xml" "$version" <<'PY'
import sys
s=open(sys.argv[1]).read().replace("__VERSION__",sys.argv[3])
s=s.replace('<pkg-ref id="cc.forge.ide.pkg"/>','<pkg-ref id="cc.forge.ide.app"/><pkg-ref id="cc.forge.ide.tools"/>')
s=s.replace('<pkg-ref id="cc.forge.ide.pkg" version="%s" onConclusion="none">Forge.pkg</pkg-ref>'%sys.argv[3],
 '<pkg-ref id="cc.forge.ide.app" version="%s">app.pkg</pkg-ref><pkg-ref id="cc.forge.ide.tools" version="%s">tools.pkg</pkg-ref>'%(sys.argv[3],sys.argv[3]))
open(sys.argv[2],'w').write(s)
PY
cp "$root/installer/resources/"* "$work/pkgs/" 2>/dev/null || true
[ -f "$work/pkgs/background.png" ] || cp "$root/apps/desktop/src-tauri/icons/128x128.png" "$work/pkgs/background.png"

mkdir -p "$(dirname "$out")"
productbuild --distribution "$work/distribution.xml" --resources "$work/pkgs" \
  --package-path "$work/pkgs" ${sign[@]+"${sign[@]}"} "$out"
echo "Fertig: $out"

if [ -n "${APPLE_ID:-}" ] && [ -n "${INSTALLER_SIGNING_IDENTITY:-}" ]; then
  xcrun notarytool submit "$out" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$out"
fi
