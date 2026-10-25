Forge ist eine Desktop-IDE für den ESP32: so leicht wie die Arduino IDE, so schnell und sorgfältig wie Zed oder Linear. Das System hat drei Eigenschaften — **präzise, ruhig, eigenwillig** — und eine Regel über allen: **Der Code steht im Mittelpunkt, die Oberfläche tritt zurück.** Das Eigenwillige zeigt sich an ruhigen Stellen und nie dort, wo gearbeitet wird.

## Grundsätze

1. **Ruhe zuerst.** Neutrale Flächen, eine Schattenstufe, Haarlinien statt Kästen. Wenn etwas auffällt, hat es einen Grund.
2. **Glut ist knapp.** Der Akzent `glut` markiert genau eine Primäraktion pro Ansicht (Flash), den Fokus, aktive Indikatoren und laufenden Fortschritt. Nie als Dekor, nie für Überschriften.
3. **Status hat eine Form.** Jeder Zustand trägt Icon oder Wort zusätzlich zur Farbe. `success` (Teal) und `error` (Rosé) unterscheiden sich im Farbton, nie als Grün gegen Rot.
4. **Hardware als Motiv.** Pin-Raster, Leiterbahnen, Kupferpads und Signalpulse — aus der Welt des Boards, nicht aus der Welt der Dev-Tools.
5. **Fakten statt Floskeln.** Adressen, Größen, Baudraten und Zeilennummern stehen da, wo man sie braucht, in Mono.

## Inhalt und Sprache

- Deutsch, **du**, sachlich und kurz. Kein Marketington in der Oberfläche.
- Satzanfang groß, sonst normale Groß-/Kleinschreibung — kein Title Case. Panel-Überschriften im Stil `overline` sind die einzige Versalien-Stelle.
- **Buttons sind Verben**: „Flashen“, „Ports suchen“, „Installieren“, „Löschen“ — nie „OK“ oder „Ja“.
- **Laufende Zustände in Verlaufsform mit Ellipse** und geschütztem Leerzeichen: „Flashe …“, „Suche Port …“, „Kompiliere …“.
- **Fehler sagen, was passiert ist und was jetzt hilft**: „Port verloren — Kabel prüfen oder BOOT gedrückt halten und erneut verbinden.“ Nicht: „Ein Fehler ist aufgetreten.“
- Compiler-Meldungen werden übersetzt und gekürzt („';' erwartet vor '}'“); die Rohausgabe bleibt im Ausgabe-Panel.
- Zahlen mit Einheit und schmalem Abstand: „742 KB“, „115200 baud“, „8.4 s“. Hex in Versalien mit `0x`: `0x10000`.
- Fachbegriffe bleiben, wie Maker sie sagen: Flash, Sketch, Board, Port, Pin, Baudrate, OTA. Keine Zwangsübersetzung („Blitzen“).
- Keine Emoji. Keine Ausrufezeichen außer in echten Warnungen.

## Farbe

Dark ist das Primärthema, Light gleichwertig. Jede Farbe kommt aus `tokens.json`; im Code nur über die CSS-Variablen.

- **Flächen, von tief nach hoch:** `bg` (Titelleiste, Activity Bar, Statusleiste) → `surface` (Editor) → `surface-panel` (Sidebar, Panels) → `surface-raised` (Palette, Dialog, Toast). Hover `surface-hover`, Auswahl `surface-active`.
- **Linien:** `line` für Struktur, `line-strong` für betonte Trenner, `line-control` für Ränder von Eingabefeldern (≥3:1).
- **Text:** `ink` für alles Wichtige, `ink-muted` für Sekundäres, `ink-faint` nur für Zeilennummern, Platzhalter und Shortcut-Hinweise — nicht auf `surface-active` und im Light-Theme nicht auf `bg`.
- **Akzent:** `glut` (glühendes Metall). Auf Glutflächen steht `on-glut`. Getönter Grund: `glut-soft`. Hover: `glut-strong`.
- **Status:** `success`, `warning`, `error`, `info`, jeweils mit `-soft`-Grund für Badges, Toasts und Inline-Fehler.
- **Syntax:** `syn-keyword` (Kupfer), `syn-type`, `syn-function`, `syn-string`, `syn-number`, `syn-constant`, `syn-macro`, `syn-comment` (kursiv), `syn-punct`, `syn-variable`. Alle ≥4.5:1 auf `surface` und ≥3:1 auf `selection`.
- **Daten:** `plot-1` bis `plot-4`, in dieser Reihenfolge. Serie 1 ist immer Glut.
- **Hardware-Motiv:** `trace` für Leiterbahnen und Pin-Raster, `copper` für Pads.

Jedes Textpaar hält in beiden Themes mindestens 4.5:1 auf den Flächen, die seine Token-Notiz nennt.

## Typografie

Drei Familien, jede mit einer Aufgabe:

- **Instrument Sans** (`sans`) für die Oberfläche — präzise, leicht schmal, mit Charakter in den Rundungen. Standard ist `body` mit 13/20 px.
- **IBM Plex Mono** (`mono`) für Code, Monitor und alle Zahlen. Editor in `code` (13.5/22 px — Code soll atmen), Monitor und Ausgabe in `code-sm`, Statusleiste und Pinnummern in `numeric` mit tabellarischen Ziffern.
- **Martian Mono** (`display`) für die Wortmarke, Leerzustände, Splash und große Messwerte (`display-xl`, `display`, `metric`). Nie für Fließtext und nie kleiner als 12 px.

Alle drei kommen von Google Fonts; `components/bundle.css` lädt sie. Hierarchie entsteht durch Gewicht und Farbe, kaum durch Größe: in der IDE gibt es außer im Leerzustand nichts über 17 px.

## Raster und Abstände

- **4-px-Raster:** `space-1` 4 · `space-2` 8 · `space-3` 12 · `space-4` 16 · `space-6` 24 · `space-8` 32 · `space-12` 48.
- Zeilen in Bäumen und Listen `size-row` (26 px), Buttons und Felder `size-control` (28 px; sm 24, lg 34).
- **Feste Schale:** Titelleiste `size-titlebar` 40 · Activity Bar `size-activitybar` 44 · Sidebar `size-sidebar` 248 · Tabs `size-tab` 34 · Panel-Kopf `size-panel-header` 32 · Statusleiste `size-statusbar` 24.
- Das **Pin-Raster** hat die Teilung von `space-6` (24 px). Es liegt hinter dem Plotter und in leeren Zuständen.

## Formen, Ebenen, Fokus

- Radien klein und präzise: `radius-xs` (Badges, Tasten) · `radius-sm` (Buttons, Felder, Tabs) · `radius-md` (Toasts, Karten) · `radius-lg` (Palette, Dialog) · `radius-pill` nur für Statuspunkte und Fortschritt.
- **Eine Schattenstufe:** `shadow-pop` für alles Schwebende. Panels trennen Haarlinien, nie Schatten. `shadow-glow` nur während Flash läuft.
- **Fokus:** 2 px solid `focus` mit 2 px Abstand an Buttons, Tabs, Baumzeilen; Felder zeigen den Fokus als Rand plus 1 px Ring. Der Ring hält ≥3:1 auf jeder Fläche.

## Bewegung

Bewegung meldet Statuswechsel — sie schmückt nicht. Dauern stehen in `bundle.css`: `--dur-1` 120 ms (Hover, Zeilen einblenden), `--dur-2` 200 ms (Palette, Toast, Tabs), `--dur-3` 360 ms (Fortschritt). Kurve `--ease` für Ankommen, `--ease-io` für Schleifen.

- **Verbunden:** der Statuspunkt pingt genau zweimal und ruht.
- **Kompilieren:** atmender Punkt in `info`.
- **Flashen:** schneller Glutpuls, `shadow-glow` am Flash-Button, Lichtlauf durch die aktive Phase der `BuildProgress`, glühende Linie über der Statusleiste.
- **Fehler:** keine Bewegung. Ein Fehler steht still, damit man ihn lesen kann.
- Bei `prefers-reduced-motion` laufen alle Animationen auf 1 ms, Signale in `Traces` verschwinden.

## Ikonografie

Ein eigener Satz aus 43 Icons im 16-px-Raster: 1.5 px Strich, runde Enden und Ecken, keine Füllungen. Im Code über `Forge.Icon` mit `currentColor`; als Dateien in der Gruppe **Icons**. Motive kommen aus der Werkbank (Chip, USB, Board, Signal, Pinleiste), nicht aus Büro-Metaphern. Kompilieren ist ein Haken, Flashen ein Blitz — wie Maker es von der Arduino IDE kennen. Keine Emoji, keine fremden Icon-Sätze daneben.

## Das Eigenwillige

Forge hat einen Charakter, aber er bleibt an ruhigen Stellen:

- **Leiterbahnen** (`Traces`): generativ, seed-basiert, waagrecht, senkrecht und im 45°-Winkel, mit Pads an den Enden. In Leerzuständen, im Splash, im Cover und während des Flashens. Nie hinter Code oder Text.
- **Pin-Raster** als Messraster im Plotter und auf der Platine der Geräteansicht.
- **Signalpuls:** ein kurzes Glutsegment, das durch Bahnen und Fortschrittsbalken wandert — das Bild für „Daten fließen zum Board“.
- **Wortmarke:** „forge“ klein in Martian Mono mit einem glühenden Quadrat. Ein Logo-Zeichen gibt es noch nicht; bis dahin steht der Name in Schrift.

## Aufbau der IDE

Von oben nach unten: **Titelleiste** (Wortmarke, Projekt, Suche ⌘K, Board-und-Port-Ziel, Kompilieren, Flash) · **Hauptbereich** aus Activity Bar, Sidebar, Editor mit Tabs und dem unteren Panel (Monitor, Plotter, Probleme, Ausgabe; teilbar) · optionale **Geräteansicht** rechts (Board, Speicher, Pinbelegung) · **Statusleiste**. Die Showcase-Karte *IdeLayout* zeigt alles zusammen und reagiert auf Flash und ⌘K.

## Barrierefreiheit

- Alles per Tastatur erreichbar; Tabs mit Pfeiltasten, Palette mit ↑ ↓ ↵ esc.
- Statusmeldungen laufen über `role="status"` bzw. `alert`; der Monitor ist ein `log`.
- Farben nie allein tragend — siehe Grundsatz 3.
