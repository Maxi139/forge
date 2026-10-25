import { create } from "zustand";
import { t } from "../i18n/de";
import { backend } from "../lib/backend";
import { errorText, useStore } from "../lib/store";
import { diffEdit } from "../lib/textEdit";

/** Dateien, die clang-format versteht. */
export const FORMATTABLE = /\.(ino|cpp|cc|c|h|hpp|hh)$/i;

interface FormatStore {
  /** `null`, solange nicht geprüft. */
  available: boolean | null;
  check(): Promise<void>;
}

export const useFormat = create<FormatStore>((set) => ({
  available: null,
  async check() {
    try {
      set({ available: await backend().formatAvailable() });
    } catch {
      set({ available: false });
    }
  },
}));

export type FormatResult = "done" | "clean" | "failed" | "skipped";

/**
 * Formatiert eine geöffnete Datei mit clang-format. Die Änderung läuft als Editor-Änderung,
 * ist also mit ⌘Z rückgängig zu machen und behält die Cursorposition so gut es geht.
 */
export async function formatFile(path: string, opts: { quiet?: boolean } = {}): Promise<FormatResult> {
  const s = useStore.getState();
  const toast = s.toast;
  const b = s.buffers[path];
  if (!b || !FORMATTABLE.test(path)) {
    if (!opts.quiet) toast({ kind: "info", title: t.editorTools.formatNoFile, ttl: 3000 });
    return "skipped";
  }
  if (b.readOnly) {
    if (!opts.quiet) toast({ kind: "info", title: t.editorTools.formatReadOnly, ttl: 3000 });
    return "skipped";
  }
  if (useFormat.getState().available === null) await useFormat.getState().check();
  if (!useFormat.getState().available) {
    if (!opts.quiet) toast({ kind: "warning", title: t.editorTools.formatMissing, message: t.editorTools.formatMissingText });
    return "skipped";
  }
  const before = b.text;
  try {
    const formatted = await backend().formatDocument(path, before);
    // Wurde inzwischen weitergetippt, passt das Ergebnis nicht mehr.
    if (useStore.getState().buffers[path]?.text !== before) return "skipped";
    const edit = diffEdit(before, formatted);
    if (!edit) {
      if (!opts.quiet) toast({ kind: "info", title: t.editorTools.formatClean, ttl: 2000 });
      return "clean";
    }
    await useStore.getState().applyEdits(path, [edit]);
    // Der Editor übernimmt die Änderung kurz nach dem Aufruf.
    await new Promise((r) => setTimeout(r, 120));
    return "done";
  } catch (e) {
    toast({ kind: "error", title: t.editorTools.formatFailed, message: errorText(e) });
    return "failed";
  }
}
