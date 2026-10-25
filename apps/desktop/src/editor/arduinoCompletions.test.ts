import { acceptCompletion, autocompletion, currentCompletions, nextSnippetField, startCompletion } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import { arduinoCompletions } from "./arduinoCompletions";

async function setup(doc: string) {
  const view = new EditorView({
    parent: document.body,
    state: EditorState.create({ doc, selection: { anchor: doc.length }, extensions: [autocompletion({ override: [arduinoCompletions] })] }),
  });
  startCompletion(view);
  for (let i = 0; i < 50 && !currentCompletions(view.state).length; i++) await new Promise((r) => setTimeout(r, 10));
  await new Promise((r) => setTimeout(r, 120)); // interactionDelay von CodeMirror
  return view;
}

// jsdom kennt keine Layout-Messung.
Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

describe("Funktionsvorschläge", () => {
  it("fügt Parameter ein und springt mit Tab bis hinter den Aufruf", async () => {
    const view = await setup("pinMod");
    expect(acceptCompletion(view)).toBe(true);
    expect(view.state.doc.toString()).toBe("pinMode(pin, mode)");
    expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("pin");
    nextSnippetField(view);
    expect(view.state.sliceDoc(view.state.selection.main.from, view.state.selection.main.to)).toBe("mode");
    nextSnippetField(view);
    expect(view.state.selection.main.head).toBe("pinMode(pin, mode)".length);
  });

  it("setzt bei Funktionen ohne Parameter den Cursor in die Klammern-Folge", async () => {
    const view = await setup("yiel");
    acceptCompletion(view);
    expect(view.state.doc.toString()).toBe("yield()");
  });
});
