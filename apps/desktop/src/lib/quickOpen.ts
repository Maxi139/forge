import type { Command } from "@forge/ui";
import { create } from "zustand";
import type { FileNode } from "./backend";
import { useStore } from "./store";

const KEY = "forge.recent-files";
const MAX_RECENT = 8;

/** Alle Dateien (ohne Ordner) in der Reihenfolge des Baums. */
export function flattenFiles(tree: FileNode[]): string[] {
  const out: string[] = [];
  const walk = (nodes: FileNode[]) => {
    for (const n of nodes) {
      if (n.isDir) walk(n.children ?? []);
      else out.push(n.path);
    }
  };
  walk(tree);
  return out;
}

function relative(root: string, path: string): string {
  return path.startsWith(`${root}/`) ? path.slice(root.length + 1) : path;
}

const base = (p: string) => p.slice(p.lastIndexOf("/") + 1);

/**
 * Einträge der Dateisuche. Ohne Eingabe stehen die zuletzt geöffneten Dateien oben (Gruppe „Zuletzt“),
 * danach alle Dateien. Mit Eingabe zählt nur die Liste aller Dateien, gesucht wird im ganzen Pfad.
 */
export function fileCommands(tree: FileNode[], root: string, recent: string[], labels: { recent: string; files: string }, active: string | null): Command[] {
  const all = flattenFiles(tree);
  const known = new Set(all);
  const recents = recent.filter((p) => known.has(p) && p !== active).slice(0, MAX_RECENT);
  const entry = (p: string, group: string, prefix: string, extra: Partial<Command> = {}): Command => {
    const rel = relative(root, p);
    const dir = rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "";
    return { id: `${prefix}:${p}`, label: base(p), hint: dir || undefined, search: rel, group, icon: "file-code", ...extra };
  };
  return [...recents.map((p) => entry(p, labels.recent, "recent", { emptyOnly: true })), ...all.map((p) => entry(p, labels.files, "file"))];
}

function read(): Record<string, string[]> {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

interface RecentStore {
  files: string[];
  load(project: string): void;
  touch(project: string, path: string): void;
}

/** Zuletzt geöffnete Dateien des aktuellen Projekts, neueste zuerst. Bleibt über Neustarts erhalten. */
export const useRecentFiles = create<RecentStore>((set, get) => ({
  files: [],
  load: (project) => set({ files: read()[project] ?? [] }),
  touch(project, path) {
    const files = [path, ...get().files.filter((p) => p !== path)].slice(0, 20);
    set({ files });
    try {
      localStorage.setItem(KEY, JSON.stringify({ ...read(), [project]: files }));
    } catch {
      /* Ohne Speicher gilt die Liste nur für diese Sitzung. */
    }
  },
}));

// Aktive Datei merken, Liste beim Projektwechsel laden.
useStore.subscribe((s, prev) => {
  const project = s.project?.path;
  if (!project) return;
  if (project !== prev.project?.path) useRecentFiles.getState().load(project);
  if (s.active && s.active !== prev.active && !s.buffers[s.active]?.readOnly) useRecentFiles.getState().touch(project, s.active);
});
