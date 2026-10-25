import { backend } from "../lib/backend";
import { useLsp } from "../lib/lsp";
import { useReferences } from "../lib/references";
import { baseName, errorText, useStore } from "../lib/store";
import { t } from "../i18n/de";
import type { SearchHit } from "../lib/backend";
import { formatFile } from "./format";

const WORD = /[A-Za-z_]\w*/g;
const IDENT = /^[A-Za-z_]\w*$/;

/** Name unter dem Cursor (Spalte ab 0). */
export function wordAt(text: string, line: number, col: number): string | null {
  const row = text.split("\n")[line - 1] ?? "";
  for (const m of row.matchAll(WORD)) {
    const from = m.index ?? 0;
    if (col >= from && col <= from + m[0].length) return m[0];
  }
  return null;
}

/** Aktive Datei und Cursor (Zeile ab 1, Spalte ab 0), oder `null` ohne offene Datei. */
function here() {
  const s = useStore.getState();
  const file = s.active;
  const buffer = file ? s.buffers[file] : undefined;
  if (!file || !buffer) return null;
  return { file, buffer, line: s.cursor.line, col: s.cursor.col - 1 };
}

/** clangd muss den Stand des Editors kennen, bevor wir fragen. */
async function ready(): Promise<boolean> {
  await useLsp.getState().sync();
  return useLsp.getState().ready;
}

export async function renameSymbol() {
  const s = useStore.getState();
  const at = here();
  if (!at) return;
  const toast = s.toast;
  if (at.buffer.readOnly || !(await ready())) {
    toast({ kind: "info", title: t.editorTools.renameNoServer, ttl: 4000 });
    return;
  }
  try {
    const target = await backend().lspPrepareRename(at.file, at.line, at.col);
    if (!target) {
      toast({ kind: "info", title: t.editorTools.renameNone, ttl: 3000 });
      return;
    }
    const name = await s.prompt({
      title: t.editorTools.renameTitle,
      label: t.editorTools.renameLabel(target.name),
      initial: target.name,
      confirm: t.editorTools.renameConfirm,
      validate: (v) => (!IDENT.test(v.trim()) ? t.editorTools.renameInvalid : v.trim() === target.name ? t.editorTools.renameSame : null),
    });
    if (!name) return;
    const files = await backend().lspRename(at.file, at.line, at.col, name.trim());
    if (!files.length) return;
    // Jede Datei wird als Editor-Änderung angewendet: sie ist danach offen und mit ⌘Z rückgängig zu machen.
    for (const f of files) {
      await useStore.getState().applyEdits(f.file, f.edits);
      await new Promise((r) => setTimeout(r, 150));
    }
    useStore.getState().setActive(at.file);
    const count = files.reduce((n, f) => n + f.edits.length, 0);
    toast({ kind: "success", title: t.editorTools.renamed(count, files.length), message: t.editorTools.renamedHint, ttl: 6000 });
  } catch (e) {
    toast({ kind: "error", title: t.editorTools.renameFailed, message: errorText(e) });
  }
}

/** Zeilentext je Datei, aus dem Editor oder von der Platte. */
async function lineSource(file: string): Promise<string[]> {
  const open = useStore.getState().buffers[file];
  if (open) return open.text.split("\n");
  try {
    return (await backend().readFile(file)).split("\n");
  } catch {
    return [];
  }
}

export async function findReferences() {
  const s = useStore.getState();
  const at = here();
  if (!at) return;
  if (!(await ready())) {
    s.toast({ kind: "info", title: t.editorTools.renameNoServer, ttl: 4000 });
    return;
  }
  const name = wordAt(at.buffer.text, at.line, at.col);
  try {
    const locs = await backend().lspReferences(at.file, at.line, at.col);
    const cache = new Map<string, string[]>();
    const hits: SearchHit[] = [];
    for (const l of locs) {
      if (!cache.has(l.file)) cache.set(l.file, await lineSource(l.file));
      const text = cache.get(l.file)![l.line - 1] ?? "";
      hits.push({ file: l.file, line: l.line, col: l.col, endCol: l.endLine === l.line ? l.endCol : text.length, text });
    }
    useReferences.getState().set({ name: name ?? baseName(at.file), hits });
    s.setSidebarViewOpen("search");
  } catch (e) {
    s.toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
  }
}

export function formatActive() {
  const active = useStore.getState().active;
  if (active) void formatFile(active).then((r) => r === "done" && useStore.getState().toast({ kind: "success", title: t.editorTools.formatDone, ttl: 1500 }));
  else useStore.getState().toast({ kind: "info", title: t.editorTools.formatNoFile, ttl: 3000 });
}
