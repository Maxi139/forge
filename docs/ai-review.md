# KI-Prüfung: Hinweise besprechen

Die KI prüft jede Funktion auf Logikfehler. Jeder Hinweis ist ein Gespräch, ähnlich einem Issue bei GitHub.

## Bedienung

- Im Problem-Panel (oder im Editor über den Hover-Hinweis) auf **Besprechen** klicken. Das Gespräch klappt unter dem Hinweis auf.
- **Erklär mir das**: Die KI erklärt, warum sie das für einen Fehler hält, mit einem Beispiel mit konkreten Werten.
- **Ist so gewollt …**: Du begründest, warum es so sein soll. Die KI liest es und antwortet. Überzeugt dich die Begründung nicht aus ihrer
  Sicht, bleibt das Gespräch offen und sie sagt, bei welchen Werten es schiefgeht. Überzeugt sie, bestätigt sie das, und das Gespräch
  schließt sich.
- **Schließen**: Du entscheidest selbst. Der Hinweis erscheint nicht wieder, auch ohne dass die KI zustimmt.
- Unter **Geschlossene Hinweise** stehen alle abgeschlossenen Gespräche. **Wieder öffnen** bringt den Hinweis zurück.
- Wurde die betroffene Zeile geändert, ist der Hinweis erledigt und das Gespräch schließt sich selbst.

## Was Forge dabei spart

- Ein geschlossenes Gespräch kostet keinen Aufruf. Forge filtert den Hinweis aus dem gespeicherten Ergebnis, auch wenn die Zeile im
  Editor wandert oder die KI sie anders formuliert.
- Muss die Funktion neu geprüft werden (weil du sie geändert hast), bekommt die KI einen Abschnitt „Bereits geklärt“ mit deinen
  Begründungen und meldet das nicht noch einmal. Funktionen ohne geklärte Hinweise sehen exakt aus wie vorher.
- Antworten im Gespräch zählen zum Tageslimit. Secrets werden wie bei der Prüfung geschwärzt, bevor etwas das Gerät verlässt.

## Wie ein Hinweis wiedererkannt wird

Ein Gespräch gehört zu **Funktionsname und Text der Zeile** (ohne Einrückung und doppelte Leerzeichen), nicht zur Zeilennummer und nicht
zur Formulierung der KI. Benennst du die Funktion um oder änderst die Zeile, ist es ein neuer Fund.

## Wo es liegt

`.forge/ai-threads.json` im Projekt (höchstens 200 Gespräche, geschlossene mit Begründung bleiben am längsten). In einer
`.forgeproj`-Datei steckt es als `ai/threads.json` und reist mit. Wer das Projekt öffnet, sieht deine Erklärungen und bekommt die
geschlossenen Hinweise nicht noch einmal gezeigt.
