import type { TextEdit } from "./diagnostics";

function position(text: string, offset: number): { line: number; col: number } {
  let line = 1;
  let start = 0;
  for (let i = text.indexOf("\n"); i !== -1 && i < offset; i = text.indexOf("\n", start)) {
    line++;
    start = i + 1;
  }
  return { line, col: offset - start };
}

/** Eine einzelne Änderung, die `before` in `after` verwandelt: nur der Bereich zwischen gleichem Anfang und Ende. */
export function diffEdit(before: string, after: string): TextEdit | null {
  if (before === after) return null;
  let head = 0;
  const max = Math.min(before.length, after.length);
  while (head < max && before[head] === after[head]) head++;
  let tail = 0;
  while (tail < max - head && before[before.length - 1 - tail] === after[after.length - 1 - tail]) tail++;
  const from = position(before, head);
  const to = position(before, before.length - tail);
  return { fromLine: from.line, fromCol: from.col, toLine: to.line, toCol: to.col, text: after.slice(head, after.length - tail) };
}

/** Wendet Änderungen auf einen Text an (für Dateien, die nicht im Editor offen sind, und für Tests). */
export function applyTextEdits(text: string, edits: TextEdit[]): string {
  const starts = [0];
  for (let i = text.indexOf("\n"); i !== -1; i = text.indexOf("\n", i + 1)) starts.push(i + 1);
  const offset = (line: number, col: number) => {
    const l = Math.min(Math.max(line, 1), starts.length) - 1;
    const end = l + 1 < starts.length ? starts[l + 1] - 1 : text.length;
    return Math.min(starts[l] + col, end);
  };
  const ranges = edits.map((e) => ({ from: offset(e.fromLine, e.fromCol), to: offset(e.toLine, e.toCol), text: e.text })).sort((a, b) => b.from - a.from);
  let out = text;
  for (const r of ranges) out = out.slice(0, r.from) + r.text + out.slice(r.to);
  return out;
}
