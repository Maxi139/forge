import { autocompletion, type CompletionContext, currentCompletions, snippet, snippetKeymap, startCompletion } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { EditorView, runScopeHandlers } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { snippetKeys } from "./snippetKeys";

// jsdom kennt keine Layout-Messung.
Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

const source = (context: CompletionContext) => {
  const word = context.matchBefore(/\w+/);
  return word ? { from: word.from, options: [{ label: "RoteLed" }] } : null;
};

const press = (view: EditorView, key: string, shiftKey = false) => runScopeHandlers(view, new KeyboardEvent("keydown", { key, shiftKey }), "editor");
const selected = (view: EditorView) => view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to);

/** Ein Aufruf als Snippet (wie von clangd, mit `${0}` hinter dem Aufruf), im ersten Feld steht ein angefangener Name mit offenem Vorschlag. */
async function setup() {
  const view = new EditorView({
    parent: document.body,
    state: EditorState.create({ extensions: [autocompletion({ override: [source] }), snippetKeymap.of(snippetKeys)] }),
  });
  snippet("pinMode(${1:pin}, ${2:mode})${0}")(view, null, 0, 0);
  view.dispatch(view.state.replaceSelection("Rote"));
  startCompletion(view);
  for (let i = 0; i < 50 && !currentCompletions(view.state).length; i++) await new Promise((r) => setTimeout(r, 10));
  await new Promise((r) => setTimeout(r, 120)); // interactionDelay von CodeMirror
  return view;
}

describe("Tab in Snippet-Feldern", () => {
  it("nimmt erst den offenen Vorschlag an und springt erst mit dem nächsten Tab weiter", async () => {
    const view = await setup();
    expect(currentCompletions(view.state)).toHaveLength(1);

    expect(press(view, "Tab")).toBe(true);
    expect(view.state.doc.toString()).toBe("pinMode(RoteLed, mode)");
    // Der Cursor steht noch hinter dem eingefügten Namen, nicht im nächsten Feld.
    expect(view.state.selection.main.head).toBe("pinMode(RoteLed".length);

    expect(press(view, "Tab")).toBe(true);
    expect(selected(view)).toBe("mode");
    expect(press(view, "Tab")).toBe(true);
    expect(view.state.selection.main.head).toBe("pinMode(RoteLed, mode)".length);
  });

  it("springt ohne offenen Vorschlag sofort weiter und mit Shift-Tab zurück", async () => {
    const view = new EditorView({ parent: document.body, state: EditorState.create({ extensions: [snippetKeymap.of(snippetKeys)] }) });
    snippet("f(${1:a}, ${2:b})${0}")(view, null, 0, 0);
    expect(selected(view)).toBe("a");
    press(view, "Tab");
    expect(selected(view)).toBe("b");
    press(view, "Tab", true);
    expect(selected(view)).toBe("a");
  });

  it("schließt mit Escape erst den Vorschlag und danach das Snippet", async () => {
    const view = await setup();
    press(view, "Escape");
    expect(currentCompletions(view.state)).toHaveLength(0);
    // Das Snippet lebt noch: Tab springt ins nächste Feld.
    press(view, "Tab");
    expect(selected(view)).toBe("mode");
  });
});
