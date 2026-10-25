import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_EDITOR, EDITOR_KEY, normalizeEditor, useEditorSettings } from "./settings";
import { GERMAN_PHRASES } from "./phrases";

describe("Editor-Einstellungen", () => {
  beforeEach(() => {
    localStorage.clear();
    useEditorSettings.setState({ settings: DEFAULT_EDITOR });
  });

  it("macht aus kaputten Werten gültige", () => {
    expect(normalizeEditor(null)).toEqual(DEFAULT_EDITOR);
    expect(normalizeEditor({ fontSize: 999, tabWidth: "x", wrap: "ja", autosave: "oft" })).toEqual({ ...DEFAULT_EDITOR, fontSize: 24 });
    expect(normalizeEditor({ fontSize: 2 }).fontSize).toBe(10);
  });

  it("speichert Änderungen unter forge.editor", () => {
    useEditorSettings.getState().patch({ fontSize: 16, autosave: "delay", wrap: true });
    expect(JSON.parse(localStorage.getItem(EDITOR_KEY)!)).toMatchObject({ fontSize: 16, autosave: "delay", wrap: true, tabWidth: 2 });
    expect(useEditorSettings.getState().settings.fontSize).toBe(16);
  });
});

describe("Deutsche Editor-Texte", () => {
  it("deckt die Suchleiste ab", () => {
    for (const k of ["Find", "Replace", "next", "previous", "all", "match case", "regexp", "by word", "replace", "replace all", "close", "Go to line", "go"]) expect(GERMAN_PHRASES[k]).toBeTruthy();
  });
});
