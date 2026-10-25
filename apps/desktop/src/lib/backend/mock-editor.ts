import type { Backend } from "./backend";
import type { LspFileEdits, LspLocation, LspSymbol } from "./types";
import type { TextEdit } from "../diagnostics";

type EditorApi = Pick<Backend, "formatAvailable" | "formatDocument" | "lspSymbols" | "lspReferences" | "lspPrepareRename" | "lspRename">;

const WORD = /[A-Za-z_]\w*/g;
const CODE = /\.(ino|cpp|cc|c|h|hpp)$/i;

/** Wort an der Stelle (Zeile ab 1, Spalte ab 0). */
function wordAt(text: string, line: number, col: number): { name: string; from: number; to: number } | null {
  const row = text.split("\n")[line - 1] ?? "";
  for (const m of row.matchAll(WORD)) {
    const from = m.index ?? 0;
    if (col >= from && col <= from + m[0].length) return { name: m[0], from, to: from + m[0].length };
  }
  return null;
}

function occurrences(text: string, name: string): Array<{ line: number; col: number }> {
  const out: Array<{ line: number; col: number }> = [];
  text.split("\n").forEach((row, i) => {
    for (const m of row.matchAll(WORD)) if (m[0] === name) out.push({ line: i + 1, col: m.index ?? 0 });
  });
  return out;
}

/** Sehr einfache Einrückung nach Klammern, genug für die Vorschau im Browser. */
export function mockFormat(text: string): string {
  let depth = 0;
  const lines = text.split("\n").map((raw) => {
    const row = raw.trim();
    if (!row) return "";
    const lead = row.startsWith("}") ? 1 : 0;
    const out = `${"  ".repeat(Math.max(depth - lead, 0))}${row}`;
    depth = Math.max(depth + (row.match(/\{/g)?.length ?? 0) - (row.match(/\}/g)?.length ?? 0), 0);
    return out;
  });
  return `${lines.join("\n").replace(/\n+$/, "")}\n`;
}

/** Beispieldaten für Gliederung, Referenzen, Umbenennen und Formatieren, damit alles im Browser sichtbar ist. */
export function createMockEditor(projectFiles: () => Record<string, string>): EditorApi {
  const codeFiles = () => Object.entries(projectFiles()).filter(([p]) => CODE.test(p));
  return {
    formatAvailable: async () => true,
    formatDocument: async (_path, text) => mockFormat(text),

    lspSymbols: async (file) => {
      const text = projectFiles()[file];
      if (text == null) return [];
      const out: LspSymbol[] = [];
      text.split("\n").forEach((row, i) => {
        const fn = /^(?:[A-Za-z_][\w:<>*&\s]*?\s+)([A-Za-z_]\w*)\s*\([^;]*\)\s*(?:\{|$)/.exec(row);
        if (fn && !/^\s*(if|for|while|switch|return)\b/.test(row)) {
          const col = row.indexOf(fn[1]);
          let end = i + 1;
          if (row.includes("{")) {
            let depth = 0;
            for (let j = i; j < text.split("\n").length; j++) {
              const r = text.split("\n")[j];
              depth += (r.match(/\{/g)?.length ?? 0) - (r.match(/\}/g)?.length ?? 0);
              end = j + 1;
              if (depth <= 0) break;
            }
          }
          out.push({ name: fn[1], detail: row.trim().replace(/\s*\{$/, ""), kind: 12, line: i + 1, col, endLine: end, endCol: 1, depth: 0, container: null });
          return;
        }
        const def = /^#define\s+([A-Za-z_]\w*)/.exec(row);
        if (def) out.push({ name: def[1], detail: "#define", kind: 14, line: i + 1, col: row.indexOf(def[1]), endLine: i + 1, endCol: row.length, depth: 0, container: null });
        const obj = /^(Adafruit_\w+|WebServer|[A-Z]\w+)\s+([a-z_]\w*)\s*[(;=]/.exec(row);
        if (obj) out.push({ name: obj[2], detail: obj[1], kind: 13, line: i + 1, col: row.indexOf(obj[2]), endLine: i + 1, endCol: row.length, depth: 0, container: null });
      });
      return out;
    },

    lspReferences: async (file, line, col) => {
      const word = wordAt(projectFiles()[file] ?? "", line, col);
      if (!word) return [];
      const out: LspLocation[] = [];
      for (const [path, text] of codeFiles())
        for (const o of occurrences(text, word.name)) out.push({ file: path, line: o.line, col: o.col, endLine: o.line, endCol: o.col + word.name.length, external: false });
      return out;
    },

    lspPrepareRename: async (file, line, col) => {
      const word = wordAt(projectFiles()[file] ?? "", line, col);
      // Schlüsselwörter und Dinge aus dem Core lassen sich nicht umbenennen.
      if (!word || /^(void|int|float|if|else|for|while|return|const|char|bool|Serial|delay|analogRead|map|setup|loop)$/.test(word.name)) return null;
      return { line, col: word.from, endLine: line, endCol: word.to, name: word.name };
    },

    lspRename: async (file, line, col, newName) => {
      const word = wordAt(projectFiles()[file] ?? "", line, col);
      if (!word) return [];
      const out: LspFileEdits[] = [];
      for (const [path, text] of codeFiles()) {
        const edits: TextEdit[] = occurrences(text, word.name).map((o) => ({ fromLine: o.line, fromCol: o.col, toLine: o.line, toCol: o.col + word.name.length, text: newName }));
        if (edits.length) out.push({ file: path, edits });
      }
      return out;
    },
  };
}
