# Forge

ESP32-IDE für macOS. Tauri 2 (Rust) mit React-Oberfläche, gebaut nach dem Forge-Designsystem.

## Entwickeln

Voraussetzungen: Rust, Node 20.19+, pnpm, `arduino-cli` (`brew install arduino-cli`).

```bash
pnpm install
pnpm --filter @forge/desktop tauri dev
```

Nur die Oberfläche mit Mock-Backend: `pnpm --filter @forge/desktop dev`, dann `http://localhost:1420`.

## Prüfen

```bash
cargo test --workspace
pnpm --filter @forge/desktop exec vitest run
pnpm --filter @forge/desktop exec tsc --noEmit
```

## Aufbau

| Pfad | Inhalt |
| --- | --- |
| `apps/desktop` | Tauri-App: `src/` React, `src-tauri/` Befehle und MCP-Anbindung |
| `packages/ui` | Designsystem als React-Komponenten (`design-source/` ist die unveränderte Vorlage) |
| `crates/forge-package` | Dateiformat `.forgeproj`: packen, entpacken, Bibliotheken erkennen ([docs/forgeproj.md](docs/forgeproj.md)) |
| `apps/quicklook` | Quick-Look-Erweiterung für `.forgeproj` (Swift), wird ins App-Bundle gelegt |
| `crates/forge-toolchain` | arduino-cli, optional fbuild, Compilerprüfung, Build mit Hash-Überspringen, Flash über esptool |
| `crates/forge-devices` | USB-Erkennung, Monitor |
| `crates/forge-analysis` | Syntaxprüfung, Regeln, Funktionsblöcke |
| `crates/forge-ai` | OpenRouter-Prüfung mit Zwischenspeicher und Tageslimit |
| `crates/forge-lsp` | clangd als Sprachserver: Vorschläge, Hover, Definition, Signaturen, semantische Farben |
| `crates/forge-secrets` | Schlüsselbund, NVS-Image, Redaktor und die Arduino-Bibliothek `ForgeSecrets` (`Secret("NAME")`) |
| `crates/forge-mcp` | MCP-Server (HTTP auf 127.0.0.1 mit Token) und stdio-Brücke |

## Werkzeuge

- `arduino-cli` ist Pflicht (`brew install arduino-cli`), der ESP32-Core wird über die Einrichtung geladen.
- `clangd` kommt von den Xcode Command Line Tools oder Homebrew-LLVM. Ohne ihn gilt die eingebaute Vorschlagsliste.
- `fbuild` ist optional (Einstellungen → Build). Es baut ca. doppelt so schnell und ist bisher nur für den ESP32 Dev Module freigegeben. Alles andere baut arduino-cli.

Tests, die echte Werkzeuge brauchen, laufen mit `cargo test -p forge-lsp --test clangd -- --ignored`.

## Claude Code

Einstellungen → Claude Code → „Mit Claude Code verbinden“ schreibt eine `.mcp.json` ins Projekt. Sie enthält nur den Aufruf der Brücke (`forge-desktop --mcp-bridge`), kein Token.

Release, Signierung und Notarisierung: [docs/release.md](docs/release.md).
