# Datenschutz: KI-Prüfung

Die KI-Prüfung ist **aus, bis du sie einschaltest**. Dafür trägst du in Einstellungen → KI einen OpenRouter-Schlüssel ein und aktivierst sie für ein Projekt.

## Was gesendet wird

Bei einer Prüfung oder KI-Aufgabe schickt Forge an `openrouter.ai`:

- den Quelltext der zu prüfenden Funktion und einen Kontext aus dem Projekt, den die Analyse zusammenstellt (zum Beispiel Deklarationen, die die Funktion braucht),
- deine Frage oder Aufgabe und den bisherigen Verlauf des Hinweises.

OpenRouter leitet die Anfrage an den gewählten Modellanbieter weiter. Dessen Datenschutzbestimmungen und die von OpenRouter gelten für diese Daten. Welche Anbieter Anfragen speichern oder zum Training nutzen, stellst du in deinem OpenRouter-Konto ein.

## Was nicht gesendet wird

- Werte aus dem Schlüsselbund (`Secret("NAME")`) und Zugangsdaten. Der Redaktor ersetzt Geheimnisse im Text vor dem Senden.
- Ganze Dateien, die für die Funktion nicht gebraucht werden.
- Dein OpenRouter-Schlüssel liegt im macOS-Schlüsselbund und geht nur als Autorisierung an OpenRouter.

## Lokal gespeichert

- Antworten liegen in einem Zwischenspeicher im Projekt (er reist in einer `.forgeproj`-Datei mit), damit gleicher Code nicht erneut geprüft wird. Ebenso die Gespräche zu Hinweisen, siehe [ai-review.md](ai-review.md).
- Ein Tageslimit für Tokens begrenzt die Nutzung (Einstellungen → KI).

## Abschalten

Einstellungen → KI → Schlüssel entfernen, oder die Prüfung für das Projekt ausschalten. Ohne Schlüssel sendet Forge nichts an OpenRouter.

Forge selbst sendet keine Nutzungsdaten. Die einzigen weiteren Netzwerkzugriffe sind die Paketverwaltung von `arduino-cli` und die Update-Prüfung (abschaltbar).
