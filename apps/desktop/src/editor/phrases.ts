import { EditorState } from "@codemirror/state";

/** Deutsche Texte für Suchleiste, „Gehe zu Zeile“, Falten und Meldungen von CodeMirror. */
export const GERMAN_PHRASES: Record<string, string> = {
  Find: "Suchen",
  Replace: "Ersetzen",
  next: "Weiter",
  previous: "Zurück",
  all: "Alle",
  "match case": "Groß-/Kleinschreibung",
  regexp: "Regex",
  "by word": "Ganzes Wort",
  replace: "Ersetzen",
  "replace all": "Alle ersetzen",
  close: "Schließen",
  "current match": "Aktueller Treffer",
  "on line": "in Zeile",
  "replaced $ matches": "$ Treffer ersetzt",
  "replaced match on line $": "Treffer in Zeile $ ersetzt",
  "Go to line": "Gehe zu Zeile",
  go: "Los",
  "folded code": "Eingeklappter Code",
  unfold: "Ausklappen",
  "Fold line": "Zeile einklappen",
  "Unfold line": "Zeile ausklappen",
  to: "bis",
  "Selection deleted": "Auswahl gelöscht",
  "Control character": "Steuerzeichen",
  Completions: "Vorschläge",
  Diagnostics: "Meldungen",
  "No diagnostics": "Keine Meldungen",
};

export const germanPhrases = EditorState.phrases.of(GERMAN_PHRASES);
