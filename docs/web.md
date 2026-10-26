# Weboberfläche für den ESP32

Forge bearbeitet HTML, CSS und JavaScript wie den Sketch selbst und macht aus der Seite fertigen C++-Text für die `.ino`.
Statt `response += "<br>"` zu tippen, schreibst du die Seite als normale Datei.

## Ablauf

1. **Befehlspalette → „Weboberfläche anlegen“** erzeugt `web/index.html`, `web/style.css` und `web/app.js`.
2. In einer Webdatei **„In .ino“** wählen. Forge erzeugt den Code, zeigt ihn an und setzt ihn vor `setup()` ein.
3. Die Seite im Sketch ausliefern:

   ```cpp
   server.on("/", []() { server.send_P(200, "text/html", INDEX_HTML); });
   ```

4. Ab jetzt aktualisiert **jedes Speichern** einer Webdatei (auch von CSS und JS) den Block in der `.ino`.
   Auch bei jedem Kompilieren steht der aktuelle Stand im Sketch.

Der erzeugte Block ist von Markierungen umschlossen. Was dazwischen steht, wird überschrieben, ändere es nicht von Hand:

```cpp
// >>> forge:web web/index.html name=INDEX_HTML minify=1
const char INDEX_HTML[] PROGMEM = R"forge(<!doctype html>…)forge";
// <<< forge:web
```

Die Optionen stehen in der ersten Zeile und lassen sich dort ändern: `name=` (Name im Code) und `minify=1` (verkleinern).

## Was beim Erzeugen passiert

- `<link rel="stylesheet" href="style.css">` und `<script src="app.js">` mit lokalen Dateien werden in die Seite eingesetzt.
  Das Gerät liefert so eine einzige Datei aus. Dateien im Netz (CDN) bleiben Verweise, Forge warnt davor, weil der ESP32 im
  eigenen WLAN oft kein Internet hat.
- Der Text kommt als Rohtext (`R"forge(…)forge"`) in den Code. Anführungszeichen und Zeilenumbrüche brauchen kein Maskieren.
  Kommt `)forge"` im Text vor, wählt Forge einen anderen Begrenzer.
- **Verkleinern** entfernt Kommentare und Leerraum und spart Flash und Übertragung. Die Webdatei bleibt unverändert.

## Platzhalter

`{{wert}}` in der Seite wird zu einem Parameter einer Funktion, die die Seite zusammensetzt:

```cpp
// web/index.html:  <p>Temperatur: {{temp}} Grad</p>
server.send(200, "text/html", renderIndex(String(temperatur)));
```

Die Werte werden unverändert eingesetzt. Kommen sie von außen, musst du sie vorher selbst maskieren.
In der Vorschau trägst du Beispielwerte ein.

## Editor

- Farben, Tag-Vervollständigung und automatisches Schließen von Tags für HTML, CSS und JavaScript.
- Emmet: `ul>li*3` und Tab erzeugt die Liste.
- Strukturfehler (Tags, Klammern, Semikolons in CSS) erscheinen beim Tippen im Editor und im Problem-Panel.
- Warnungen für fehlende Dateien und für Dateien aus dem Netz.

## Vorschau

„Vorschau“ zeigt die Seite neben dem Editor, so wie das Gerät sie ausliefert (Dateien eingesetzt). Die Vorschau läuft in einem
abgeschotteten Fenster: Skripte laufen, senden aber nichts ins Netz und erreichen weder Forge noch das Gerät. Anfragen an
das Gerät (`fetch("/led")`) laufen deshalb ins Leere. Fehler und `console.log` der Seite erscheinen unter „Konsole“.

## KI

Alles, was die KI für den Sketch kann, gilt auch für Webdateien:

- **Prüfung** auf Logikfehler: Anfragen an Adressen, die der Sketch nicht beantwortet (Forge liest die `server.on(…)` aus dem
  Sketch), IDs, die es nicht gibt, falsche Methode, zu schnelles Abfragen. Die Prüfung läuft pro Datei und erst nach dem
  Speichern, nicht bei jedem Tastendruck, und nutzt den Zwischenspeicher wie beim Sketch.
- **Beheben**, **Besprechen** und **KI-Aufgabe** (⇧⌘I) arbeiten über Sketch und Webdateien hinweg, etwa „Füge einen Knopf hinzu,
  der die LED schaltet“. Die KI sieht den erzeugten Block nur gekürzt und ändert die Webdatei, nicht den Block.
