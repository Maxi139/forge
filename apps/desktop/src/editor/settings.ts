import { indentUnit } from "@codemirror/language";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { create } from "zustand";

export type Autosave = "off" | "delay" | "blur";

export interface EditorSettings {
  /** Schriftgröße des Quelltexts in Pixeln. */
  fontSize: number;
  tabWidth: number;
  wrap: boolean;
  autosave: Autosave;
  formatOnSave: boolean;
}

export const EDITOR_KEY = "forge.editor";
export const DEFAULT_EDITOR: EditorSettings = { fontSize: 13, tabWidth: 2, wrap: false, autosave: "off", formatOnSave: false };
export const FONT_RANGE = [10, 24] as const;
export const TAB_WIDTHS = [2, 4, 8] as const;
export const EDITOR_FONT_SIZES = [11, 12, 13, 14, 15, 16, 18, 20] as const;
/** Pause nach der letzten Eingabe, nach der „automatisch speichern“ greift. */
export const AUTOSAVE_DELAY = 1200;

const clamp = (n: unknown, lo: number, hi: number, fallback: number) => (typeof n === "number" && Number.isFinite(n) ? Math.min(Math.max(Math.round(n), lo), hi) : fallback);

/** Macht aus gespeicherten, womöglich veralteten oder kaputten Werten gültige Einstellungen. */
export function normalizeEditor(raw: unknown): EditorSettings {
  const r = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
  return {
    fontSize: clamp(r.fontSize, FONT_RANGE[0], FONT_RANGE[1], DEFAULT_EDITOR.fontSize),
    tabWidth: clamp(r.tabWidth, 1, 8, DEFAULT_EDITOR.tabWidth),
    wrap: typeof r.wrap === "boolean" ? r.wrap : DEFAULT_EDITOR.wrap,
    autosave: r.autosave === "delay" || r.autosave === "blur" || r.autosave === "off" ? r.autosave : DEFAULT_EDITOR.autosave,
    formatOnSave: typeof r.formatOnSave === "boolean" ? r.formatOnSave : DEFAULT_EDITOR.formatOnSave,
  };
}

function read(): EditorSettings {
  try {
    return normalizeEditor(JSON.parse(localStorage.getItem(EDITOR_KEY) ?? "{}"));
  } catch {
    return DEFAULT_EDITOR;
  }
}

function write(s: EditorSettings) {
  try {
    localStorage.setItem(EDITOR_KEY, JSON.stringify(s));
  } catch {
    /* Ohne Speicher gelten die Einstellungen nur für diese Sitzung. */
  }
}

interface EditorSettingsStore {
  settings: EditorSettings;
  patch(p: Partial<EditorSettings>): void;
}

export const useEditorSettings = create<EditorSettingsStore>((set, get) => ({
  settings: read(),
  patch(p) {
    const settings = normalizeEditor({ ...get().settings, ...p });
    write(settings);
    set({ settings });
  },
}));

/** CodeMirror-Erweiterungen zu den Einstellungen. Sie liegen in einem Compartment und tauschen sich ohne Neuaufbau. */
export function editorSettingsExtension(s: EditorSettings): Extension {
  return [
    EditorState.tabSize.of(s.tabWidth),
    indentUnit.of(" ".repeat(s.tabWidth)),
    s.wrap ? EditorView.lineWrapping : [],
    EditorView.editorAttributes.of({ style: `--fs-code:${s.fontSize}px` }),
  ];
}
