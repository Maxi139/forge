import { acceptCompletion, clearSnippet, closeCompletion, completionStatus, nextSnippetField, prevSnippetField } from "@codemirror/autocomplete";
import type { KeyBinding } from "@codemirror/view";

/**
 * Tastenbelegung innerhalb eines Snippets, etwa `pinMode(pin, mode)`. Standardmäßig springt Tab zum nächsten Feld,
 * auch wenn gerade ein Vorschlag offen ist. Dann wäre `RoteLed` nie eingefügt worden. Hier gilt:
 * Ist ein Vorschlag offen, nimmt Tab ihn an und bleibt im Feld. Erst der nächste Tab springt weiter.
 * Escape schließt zuerst den Vorschlag und beendet erst dann das Snippet.
 */
export const snippetKeys: readonly KeyBinding[] = [
  {
    key: "Tab",
    run: (view) => (completionStatus(view.state) === "active" && acceptCompletion(view)) || nextSnippetField(view),
    shift: prevSnippetField,
  },
  { key: "Escape", run: (view) => (completionStatus(view.state) ? closeCompletion(view) : clearSnippet(view)) },
];
