import { EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { describe, expect, it } from "vitest";
import type { Diagnostic } from "../lib/diagnostics";
import { diagnosticsField, forgeDiagnostics, setDiagnostics } from "./diagnostics";

Range.prototype.getClientRects ??= () => [] as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

function diag(line: number, from: number, to: number, source: Diagnostic["source"] = "ai"): Diagnostic {
  return { id: `${source}:${line}:${from}`, file: "a.ino", line, from, to, severity: "warning", message: "Test", source };
}

describe("Meldungen im Editor", () => {
  it("verträgt Spalten hinter dem Zeilenende und leere Spannen in beliebiger Reihenfolge", () => {
    const view = new EditorView({ parent: document.body, state: EditorState.create({ doc: "ab\ncd\nef\n", extensions: [forgeDiagnostics()] }) });
    // Die Spalte 10 liegt hinter Zeile 1. Geklemmt landet sie vor dem Anfang von Zeile 2.
    const list = [diag(2, 0, 1), diag(1, 10, 12), diag(3, 2, 2, "syntax"), diag(1, 2, 2, "syntax"), diag(3, 0, 5)];
    expect(() => view.dispatch({ effects: setDiagnostics.of(list) })).not.toThrow();
    expect(view.state.field(diagnosticsField).list).toHaveLength(5);
  });
});
