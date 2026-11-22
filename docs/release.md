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

Forge prüft über `tauri-plugin-updater`, ob es eine neuere Version gibt (Menü „Nach Updates suchen …“, beim Start abschaltbar unter Einstellungen → Info). Der Stand im Repository ist ein **Platzhalter**, bis du einen Schlüssel erzeugt hast:

- `plugins.updater.endpoints` in `apps/desktop/src-tauri/tauri.conf.json` zeigt auf `https://github.com/maximilianschmidt/forge/releases/latest/download/latest.json`. Ersetze die Adresse durch deine echte Projektseite (dieselbe steht in `menu.rs`, `src/lib/links.ts` und `capabilities/default.json`).
- `plugins.updater.pubkey` ist leer. Solange er leer ist, meldet die Prüfung „nicht eingerichtet“. Beim Start bleibt das unsichtbar (nur ein Eintrag im Protokoll), und die App läuft normal.

### Schlüsselpaar erzeugen

```bash
pnpm --filter @forge/desktop tauri signer generate -w ~/.tauri/forge.key
```

Das Kommando fragt nach einem Passwort und schreibt `forge.key` (privat) und `forge.key.pub` (öffentlich).

1. Den Inhalt von `forge.key.pub` trägst du als `pubkey` in `tauri.conf.json` ein.
2. Den privaten Schlüssel legst du **nicht** ins Repository. Für den Release-Workflow kommt sein Inhalt in das GitHub-Secret `TAURI_SIGNING_PRIVATE_KEY`, das Passwort in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Zum Bauen mit Update-Artefakten (`Forge.app.tar.gz` und `.sig`) setzt der Workflow `TAURI_CONFIG='{"bundle":{"createUpdaterArtifacts":true}}'`. Lokale Builds erzeugen keine Update-Artefakte und brauchen den Schlüssel nicht.

Geht der private Schlüssel verloren, kann keine installierte Version mehr ein Update annehmen. Bewahre ihn also zusätzlich im Passwortmanager auf.

## Release-Workflow

`.github/workflows/release.yml` läuft bei jedem Tag `v*`: Er prüft, dass der Tag zur Version in `Cargo.toml` passt, baut die App, signiert und notarisiert sie, hängt `Forge_<Version>_aarch64.dmg`, `Forge.app.tar.gz`, `.sig` und `latest.json` an ein GitHub Release (Entwurf).

Secrets: `APPLE_CERTIFICATE` (Developer-ID-Zertifikat als base64-`.p12`), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

Eine neue Version: `node scripts/sync-version.mjs 0.2.0`, `CHANGELOG.md` ergänzen, committen, `git tag v0.2.0`, pushen.

## Installer (.pkg)

```bash
pnpm --filter @forge/desktop tauri build --bundles app
scripts/build-installer.sh
```

Ergebnis: `target/release/bundle/Forge-<Version>.pkg`. Der Installer zeigt Willkommen, Fortschritt und Abschluss und legt an:

- `Forge.app` in `/Applications`
- `arduino-cli` (universal) in `/Library/Application Support/Forge/bin`, Forge sucht dort zuerst
- ESP32-Core: das `postinstall`-Skript installiert ihn als angemeldeter Benutzer nach `~/Library/Arduino15`. Fehlt das Netz, ist das kein Fehler, Forge holt den Core beim ersten Start mit Fortschrittsanzeige nach. Protokoll: `/tmp/forge-installer.log`

Signieren und notarisieren: `INSTALLER_SIGNING_IDENTITY` („Developer ID Installer: …“) sowie `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` setzen. Ohne sie ist das Paket unsigniert (Rechtsklick → Öffnen).

`clangd` bringt der Installer nicht mit (Xcode Command Line Tools).

## Werkzeuge im Bundle

`forge-desktop --mcp-bridge` ist die stdio-Brücke für Claude Code und steckt in derselben Binärdatei. `arduino-cli`, `clangd` und `fbuild` liegen nicht im Bundle:

- `arduino-cli` kommt vom Installer, sonst wird der PATH durchsucht (`brew install arduino-cli`).
- `clangd` kommt von den Xcode Command Line Tools oder Homebrew-LLVM.
- `fbuild` installiert Forge auf Wunsch selbst in `~/Library/Application Support/cc.forge.ide/tools`.
