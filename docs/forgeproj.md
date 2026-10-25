# Das Format `.forgeproj`

Eine `.forgeproj`-Datei enthält ein ganzes Forge-Projekt oder einen Sketch. Du kannst sie verschicken, und wer sie öffnet,
hat dasselbe Projekt mit denselben Bibliotheken, dem Board, den Secret-Namen und den gespeicherten KI-Prüfungen.

`.forgeproj` ist die Art, wie Forge speichert. Neue Sketches landen in `~/Documents/Forge/Sketches`, neue Projekte als
`<Name>.forgeproj` im Ordner, den du im Dialog wählst. **Speichern unter …** (⇧⌘S) legt das Projekt unter einem anderen Namen
oder Ort ab, und Forge arbeitet danach dort weiter. Ein Ordner mit Code ist nur der Altweg: Forge öffnet ihn weiterhin und bietet
an, ihn als Projektdatei zu speichern (der Ordner bleibt dabei unverändert). Wer einen Ordner braucht, zum Beispiel für Git,
wählt Datei → **Als Ordner exportieren …**. Ein Git-Repository gehört nicht in die Datei.

## Im Betrieb

Forge entpackt die Datei in einen Arbeitsordner (`~/Library/Application Support/cc.forge.ide/workspaces/<id>/<name>`).
Dort arbeiten Editor, Build und Sprachserver wie bei jedem Ordner. Anderthalb Sekunden nach der letzten Änderung schreibt Forge
alles zurück in die `.forgeproj`-Datei, über eine Zwischendatei, damit ein Absturz nie eine halbe Datei hinterlässt. Beim
Schließen und Beenden speichert Forge sofort.

Öffnest du dieselbe Datei erneut, bleibt der Arbeitsordner samt Build-Zwischenständen erhalten, solange die Datei seit dem
letzten Speichern unverändert ist.

## Aufbau

Die Datei ist ein ZIP-Archiv (Deflate). Mit jedem Entpacker lässt sie sich auch ohne Forge öffnen.

```text
manifest.json            Beschreibung, immer der erste Eintrag
files/…                  Inhalt des Projektordners (.ino, .cpp, data/, forge.toml …)
libraries/<Ordner>/…     Bibliotheken, die es nicht im Bibliotheksverwalter gibt
cache/ai-review.json     gespeicherte KI-Prüfungen
ai/threads.json          Gespräche über Hinweise der KI (siehe ai-review.md)
```

Nicht enthalten sind `.git`, `build/`, `.DS_Store`, `node_modules`, Zwischendateien von Forge und symbolische Links. Gleiche
Inhalte ergeben gleiche Bytes (feste Zeitstempel, sortierte Einträge). Bilder, Archive und Schriften werden nur abgelegt, nicht
noch einmal komprimiert. Obergrenze sind 256 MB unkomprimiert.

### `manifest.json`

| Feld | Inhalt |
| --- | --- |
| `format` | Formatstand, aktuell `1`. Steigt nur, wenn ältere Forge-Versionen die Datei nicht mehr lesen könnten. |
| `kind` | `project` oder `sketch` |
| `id` | Stabile Kennung. Unter ihr liegen die Secrets im Schlüsselbund. |
| `name`, `description` | Name (zugleich Name des Arbeitsordners) und Beschreibung |
| `app`, `savedAt` | Wer zuletzt gespeichert hat und wann (Sekunden seit 1970) |
| `board` | `fqbn` und Menüoptionen. Bei Sketches steht das Board nur hier. |
| `toolchain.cores` | Installierte Cores, z. B. `esp32:esp32` → `3.3.12` |
| `libraries` | Benutzte Bibliotheken: `name`, `version`, `source` |
| `secrets` | **Nur die Namen.** Die Werte bleiben im Schlüsselbund und gehören nie in die Datei. |
| `ai` | `enabled`, und ob Prüfergebnisse enthalten sind (`cached`) |

Unbekannte Felder werden ignoriert. Eine Datei mit höherem `format` lehnt Forge mit dem Hinweis ab, die App zu aktualisieren.

### Bibliotheken

Forge erkennt die benutzten Bibliotheken an den `#include`-Zeilen. Bibliotheken des Cores gehören zum Core und stehen nur dort.
Für jede Bibliothek aus dem Ordner des Nutzers gilt:

- `registry`: Der Bibliotheksverwalter bietet genau diese Version an. Es wird nur der Verweis gespeichert.
- `embedded`: Es gibt sie dort nicht (selbst geschrieben, aus Git, aus einer ZIP-Datei). Ihr Quelltext liegt unter
  `libraries/<Ordner>`, damit die Datei auch ohne Netz vollständig ist.
- `forge`: `ForgeSecrets`. Forge legt sie bei Bedarf selbst an.

Beim Öffnen vergleicht Forge mit dem Rechner und fragt, ob es Fehlendes installieren soll. Es installiert nichts ohne Rückfrage
und ersetzt nie eine bereits installierte andere Version, damit andere Projekte nicht brechen. Abweichungen zeigt es nur an.

### KI-Prüfung

`cache/ai-review.json` ist der Zwischenspeicher der KI-Prüfung für dieses Projekt. Er liegt im Arbeitsordner unter
`.forge/ai-cache.json` und hat höchstens 600 Einträge. Die Schlüssel sind Hashes aus dem Inhalt eines Blocks, dem Modell und der
Version der Vorgabe. Wer dasselbe Modell nutzt, zahlt für unveränderte Blöcke nichts. Bei einem anderen Modell prüft Forge neu.

## Vorschau im Finder

Forge liefert eine Quick-Look-Erweiterung mit (`apps/quicklook`, `Forge.app/Contents/PlugIns/ForgeQuickLook.appex`) und meldet
sie beim Start bei macOS an. Dann zeigt die Leertaste im Finder eine Übersicht (Board, Core, Bibliotheken, Secret-Namen, Stand der
KI-Prüfung) und den Dateibaum mit Vorschau von Text und Bildern. Einzelne Dateien, etwa die `.ino`, holst du heraus über

- **Datei sichern …** für die ausgewählte Datei,
- **Alles entpacken …** für den ganzen Projektordner,
- Ziehen aus der Liste auf den Schreibtisch oder in ein anderes Fenster.

Falls die Vorschau nicht erscheint: Systemeinstellungen → Allgemein → Anmeldeobjekte & Erweiterungen → Quick Look.
`pluginkit -m -i cc.forge.ide.quicklook` zeigt, ob macOS die Erweiterung kennt.

## Entwickeln

```bash
cargo test -p forge-package       # Format, Sicherheit (Pfade, Links, neuere Formate)
apps/quicklook/build.sh           # baut apps/quicklook/build/ForgeQuickLook.appex
```

`pnpm tauri build` baut die Erweiterung vor dem Bündeln (`beforeBundleCommand`) und legt sie ins App-Bundle. Signiert wird mit
`APPLE_SIGNING_IDENTITY`, sonst ad hoc.
