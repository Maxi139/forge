import type { BacktraceFrame } from "./backend/types";

/** Zeilen, in denen Code-Adressen stehen, die sich auflösen lassen (Backtrace und Register-Dump). */
export function isDecodable(text: string): boolean {
  return /^\s*Backtrace:\s*0x[0-9a-f]+/i.test(text) || /(?:^|\s)(?:PC|MEPC|RA)\s*:\s*0x[0-9a-f]{4,8}/i.test(text);
}

const baseName = (p: string) => p.slice(p.lastIndexOf("/") + 1);

/**
 * Die Datei im Projekt, auf die eine aufgelöste Stelle zeigt, oder null für Kern- und Bibliothekscode.
 * Der Compiler meldet Sketches oft als Kopie im Build-Ordner (`Name.ino.cpp`), das gilt als die `.ino`.
 */
export function projectFileFor(file: string, root: string, mainFile: string): string | null {
  if (!file || file.startsWith("??")) return null;
  if (file.startsWith(`${root}/`)) return file;
  const name = baseName(file);
  const ino = name.endsWith(".ino.cpp") ? name.slice(0, -4) : name;
  if (ino.endsWith(".ino")) {
    const main = baseName(mainFile);
    if (ino === main) return mainFile;
    const sibling = `${mainFile.slice(0, mainFile.lastIndexOf("/"))}/${ino}`;
    return sibling;
  }
  return null;
}

export function frameLabel(f: BacktraceFrame): string {
  return `${f.function} · ${baseName(f.file)}:${f.line}`;
}
