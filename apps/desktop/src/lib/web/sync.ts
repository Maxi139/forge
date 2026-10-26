import { t } from "../../i18n/de";
import { backend, type FileNode } from "../backend";
import { baseName, errorText, useStore } from "../store";
import { diffEdit } from "../textEdit";
import { buildPage, findBlocks, generate, references, replaceBlocks, webKind, type EmbedSpec, type Generated } from "./embed";

/** Quelltext des Sketches, in den die erzeugten Blöcke kommen. */
export const CODE_FILE = /\.(ino|cpp|c|cc|h|hpp)$/i;

export function projectRoot(): string {
  return useStore.getState().project?.path ?? "";
}

/** Pfad relativ zum Projekt, mit `/`. */
export function toRel(abs: string): string {
  const root = projectRoot();
  return root && abs.startsWith(`${root}/`) ? abs.slice(root.length + 1) : abs;
}

export const toAbs = (rel: string) => `${projectRoot()}/${rel}`;

export function flattenTree(nodes: FileNode[]): string[] {
  return nodes.flatMap((n) => (n.isDir ? flattenTree(n.children ?? []) : [n.path]));
}

/** Text einer Datei im Projekt: der Editor-Stand, sonst die Platte. `null`, wenn es sie nicht gibt. */
export async function readProjectText(abs: string): Promise<string | null> {
  const open = useStore.getState().buffers[abs];
  if (open) return open.text;
  try {
    return await backend().readFile(abs);
  } catch {
    return null;
  }
}

export interface RenderedPage {
  /** Die fertige Seite: Dateien eingesetzt, auf Wunsch verkleinert. */
  page: string;
  /** Lokale Dateien, die die Seite einbindet und die es nicht gibt. */
  missing: string[];
}

/** Liest die Seite samt ihrer Stylesheets und Skripte und baut daraus, was das Gerät ausliefert. */
export async function renderPage(sourceRel: string, minify: boolean): Promise<RenderedPage | null> {
  const html = await readProjectText(toAbs(sourceRel));
  if (html == null) return null;
  const files = new Map<string, string>();
  for (const ref of references(html, sourceRel)) {
    if (!ref.path || files.has(ref.path)) continue;
    const text = await readProjectText(toAbs(ref.path));
    if (text != null) files.set(ref.path, text);
  }
  const built = buildPage(html, sourceRel, files, minify);
  return { page: built.html, missing: built.missing };
}

/** Erzeugt den Block für eine Quelle. */
export async function renderBlock(spec: EmbedSpec): Promise<(Generated & { missing: string[] }) | null> {
  const rendered = await renderPage(spec.source, spec.minify);
  if (!rendered) return null;
  return { ...generate(rendered.page, spec), missing: rendered.missing };
}

/** Übernimmt neuen Text für eine Datei des Sketches. Die offene Datei ändert sich im Editor, ⌘Z nimmt es zurück. */
async function putText(abs: string, text: string): Promise<void> {
  const s = useStore.getState();
  const b = s.buffers[abs];
  if (!b) {
    await backend().writeFile(abs, text);
    return;
  }
  if (b.text === text) return;
  if (s.active === abs) {
    const edit = diffEdit(b.text, text);
    if (edit) await s.applyEdits(abs, [edit]);
    return;
  }
  const clean = b.text === b.saved;
  useStore.setState((st) => ({
    buffers: { ...st.buffers, [abs]: { ...b, text, saved: clean ? text : b.saved, revision: b.revision + 1, version: b.version + 1 } },
  }));
  if (clean) await backend().writeFile(abs, text);
}

/** Schreibt einen Block (neu oder aktualisiert) in eine Datei des Sketches. */
export async function writeCode(abs: string, transform: (text: string) => string): Promise<boolean> {
  const before = await readProjectText(abs);
  if (before == null) throw new Error(`${baseName(abs)} gibt es nicht.`);
  const after = transform(before);
  if (after === before) return false;
  await putText(abs, after);
  return true;
}

let running: Promise<void> = Promise.resolve();

/**
 * Wartet, bis der Abgleich der Webseiten fertig ist. Vor dem Bauen nötig: Speichern einer Webseite stößt ihn an,
 * und er ändert die .ino, die danach noch einmal gespeichert werden muss.
 */
export async function settleWebSync(): Promise<boolean> {
  await running;
  return useStore.getState().saveAll();
}

/**
 * Gleicht alle erzeugten Blöcke im Sketch mit ihren Webdateien ab. Läuft nach jedem Speichern einer Webdatei.
 * Gibt die Dateien zurück, die sich geändert haben.
 */
export function syncWebBlocks(opts: { quiet?: boolean } = {}): Promise<string[]> {
  const job = running.then(() => syncNow(opts));
  running = job.then(
    () => undefined,
    () => undefined,
  );
  return job;
}

async function syncNow(opts: { quiet?: boolean }): Promise<string[]> {
  const s = useStore.getState();
  if (!s.project) return [];
  const changed: string[] = [];
  const rendered = new Map<string, Generated | null>();
  try {
    for (const abs of flattenTree(s.tree).filter((p) => CODE_FILE.test(p))) {
      const text = await readProjectText(abs);
      if (text == null || !text.includes("forge:web")) continue;
      const blocks = findBlocks(text);
      if (!blocks.length) continue;
      for (const b of blocks) {
        const key = `${b.source}\0${b.name}\0${b.minify}`;
        if (!rendered.has(key)) rendered.set(key, await renderBlock({ source: b.source, name: b.name, minify: b.minify }));
      }
      // Fehlt die Webdatei, bleibt der Block, wie er ist.
      const next = replaceBlocks(text, (b) => rendered.get(`${b.source}\0${b.name}\0${b.minify}`)?.code ?? null);
      if (next !== text) {
        await putText(abs, next);
        changed.push(abs);
      }
    }
  } catch (e) {
    useStore.getState().toast({ kind: "error", title: t.web.syncFailed, message: errorText(e) });
    return changed;
  }
  if (changed.length && !opts.quiet) {
    useStore.getState().toast({ kind: "info", title: t.web.synced(changed.map(baseName).join(", ")), ttl: 2500 });
  }
  return changed;
}

/** Wird nach jedem Speichern aufgerufen: nur Webdateien lösen den Abgleich aus. */
export function onFileSaved(abs: string) {
  if (webKind(abs)) void syncWebBlocks();
}

