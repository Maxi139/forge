# Änderungen

Das Format folgt [Keep a Changelog](https://keepachangelog.com/de/1.1.0/). Die Version steht in `Cargo.toml`, siehe `scripts/sync-version.mjs`.

## [Unveröffentlicht]

### Neu
- Weboberfläche: HTML, CSS und JavaScript bearbeiten (Farben, Vervollständigung, Emmet, Fehlermeldungen), Vorschau neben dem Editor und „In .ino“: Forge setzt die Seite als Rohtext (`R"forge(…)forge"`) in den Sketch und aktualisiert den Block bei jedem Speichern. Platzhalter `{{wert}}` werden zu Funktionsparametern. KI-Prüfung, Beheben, Besprechen und Aufgaben gelten auch für Webdateien, siehe `docs/web.md`.
- Protokolldatei unter `~/Library/Logs/cc.forge.ide` mit Rotation, Panic-Hook und Fehlerhandlern in der Oberfläche; Menü „Hilfe → Protokoll im Finder zeigen“.
- Fenstergröße und -position werden gemerkt. Ein zweiter Start öffnet übergebene Dateien in der laufenden App.
- Über-Dialog mit Version, Copyright und Projektseite; Hilfe-Menü mit Dokumentation und „Problem melden“; Suchen im Menü „Bearbeiten“; „Alle in den Vordergrund“ im Fenstermenü.
- Einstellungen → Info: Version, Projektseite und die Lizenzen aller Drittanbieter (Rust, npm, Schriften).
- Selbstaktualisierung über `tauri-plugin-updater`: „Nach Updates suchen …“, Prüfung beim Start abschaltbar. Braucht Schlüssel und Endpunkt, siehe `docs/release.md`.
- Release-Workflow für Tags `v*` (signieren, notarisieren, DMG und `latest.json`).
- `LICENSE` (MIT) und die OFL-Texte der gebündelten Schriften.
- Datenschutznotiz zur KI-Prüfung: `docs/datenschutz-ki.md`.

### Geändert
- Die Syntaxprüfung versteht Rohtext-Literale (`R"x(…)x"`) über mehrere Zeilen.
- Tab in Snippet-Feldern nimmt zuerst einen offenen Vorschlag an (etwa `LED_BUILTIN` in `pinMode(`), erst der nächste Tab springt zum nächsten Feld.
- Eine Versionsquelle: `[workspace.package]` in `Cargo.toml`.
- Release-Build mit LTO, `codegen-units = 1` und `strip`.
- Strengere Content-Security-Policy (`object-src`, `base-uri`, `form-action` geschlossen).
- Das Hilfe-Menü teilte sich eine ID mit der Ansicht-Palette; sie ist entfernt.
- README: Node 20.19 oder neuer.

### Entwicklung
- `biome.json` und eine Biome-Prüfung in der CI; geprüft wird nur der Lint, bekannte Regelverstöße des Bestands sind abgeschaltet.
