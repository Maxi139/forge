import { createElement, type ReactNode } from "react";

export function cx(...parts: Array<string | false | null | undefined>): string {
  return parts.filter(Boolean).join(" ");
}

/** Unscharfer Abgleich: alle Zeichen der Eingabe müssen der Reihe nach vorkommen. */
export function fuzzy(query: string, text: string): { score: number; idx: number[] } | null {
  if (!query) return { score: 0, idx: [] };
  const q = query.toLowerCase();
  const s = text.toLowerCase();
  const idx: number[] = [];
  let j = 0;
  let score = 0;
  let prev = -2;
  for (let i = 0; i < s.length && j < q.length; i++) {
    if (s[i] === q[j]) {
      idx.push(i);
      score += (i === prev + 1 ? 3 : 1) + (i === 0 ? 3 : " /_-.".includes(s[i - 1]) ? 2 : 0);
      prev = i;
      j++;
    }
  }
  return j === q.length ? { score, idx } : null;
}

/** Hebt die Treffer-Zeichen mit <mark> hervor. */
export function marks(text: string, idx: number[] | undefined): ReactNode {
  if (!idx || idx.length === 0) return text;
  const out: ReactNode[] = [];
  let last = 0;
  idx.forEach((i, k) => {
    if (i > last) out.push(text.slice(last, i));
    out.push(createElement("mark", { key: k }, text[i]));
    last = i + 1;
  });
  out.push(text.slice(last));
  return out;
}

export function formatBytes(b: number): string {
  if (b >= 1048576) return `${(b / 1048576).toFixed(2)} MB`;
  if (b >= 1024) return `${(b / 1024).toFixed(1)} KB`;
  return `${b} B`;
}

export interface LineFilter {
  query: string;
  matchCase: boolean;
  errorsOnly: boolean;
}

/** Zeilen des Monitors nach Text und Art. Kurze Zeilen mit leerer Suche bleiben alle erhalten. */
export function filterLines<T extends { text: string; kind?: string }>(lines: T[], f: LineFilter): T[] {
  const q = f.matchCase ? f.query : f.query.toLowerCase();
  return lines.filter((l) => {
    if (f.errorsOnly && l.kind !== "err") return false;
    if (!q) return true;
    return (f.matchCase ? l.text : l.text.toLowerCase()).includes(q);
  });
}
