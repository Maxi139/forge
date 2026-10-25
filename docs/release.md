# Release

Forge wird als `Forge.app` und `.dmg` gebaut. Für die Weitergabe muss die App signiert und notarisiert sein.

## Bauen

```bash
pnpm install
pnpm --filter @forge/desktop tauri build --bundles app,dmg
```

Das Ergebnis liegt unter `target/release/bundle/`.

`tauri build` ruft vorher `apps/quicklook/build.sh` auf und legt die Vorschau-Erweiterung für `.forgeproj` nach `Contents/PlugIns/`.

## Signieren und notarisieren

Du brauchst ein Apple-Developer-ID-Zertifikat. Tauri liest diese Variablen:

| Variable | Inhalt |
| --- | --- |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` | Apple-ID für die Notarisierung |
| `APPLE_PASSWORD` | App-spezifisches Passwort dieser Apple-ID |
| `APPLE_TEAM_ID` | Team-ID |

`apps/quicklook/build.sh` liest `APPLE_SIGNING_IDENTITY` ebenfalls und signiert die Erweiterung mit Sandbox-Rechten (`ForgeQuickLook.entitlements`). Sie muss vor der App signiert sein, und das ist sie, weil sie vor dem Bündeln gebaut wird.

Mit gesetzten Variablen signiert `tauri build` mit Hardened Runtime (`Entitlements.plist`) und lädt die App zur Notarisierung hoch.

## Updates

Die App aktualisiert sich noch nicht selbst. Dafür braucht es `tauri-plugin-updater`, ein Schlüsselpaar (`pnpm tauri signer generate`) und einen Ort für `latest.json`. Der private Schlüssel gehört in den Schlüsselbund des Release-Rechners, nicht ins Repository.

## Werkzeuge im Bundle

`forge-desktop --mcp-bridge` ist die stdio-Brücke für Claude Code und steckt in derselben Binärdatei. `arduino-cli`, `clangd` und `fbuild` liegen nicht im Bundle:

- `arduino-cli` wird im PATH gesucht (`brew install arduino-cli`).
- `clangd` kommt von den Xcode Command Line Tools oder Homebrew-LLVM.
- `fbuild` installiert Forge auf Wunsch selbst in `~/Library/Application Support/cc.forge.ide/tools`.
