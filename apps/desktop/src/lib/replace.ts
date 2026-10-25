import type { TextEdit } from "./diagnostics";
import type { SearchOptions } from "./backend";

export interface ReplaceSpec extends SearchOptions {
  query: string;
  replacement: string;
}

/** Baut den Suchausdruck wie die Projektsuche (ganzes Wort, Groß-/Kleinschreibung, Regex). */
export function buildRegex(spec: SearchOptions & { query: string }): RegExp {
  const body = spec.regex ? spec.query : spec.query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  try {
    return new RegExp(spec.wholeWord ? `\\b(?:${body})\\b` : body, spec.caseSensitive ? "g" : "gi");
  } catch {
    throw new Error("Der Ausdruck ist ungültig.");
  }
}

/** Setzt `$1`, `$&`, `$<name>` und `$$` ein. Ohne Regex gilt der Ersatz wörtlich. */
export function expandReplacement(replacement: string, m: RegExpMatchArray, regex: boolean): string {
  if (!regex) return replacement;
  return replacement.replace(/\$(\$|&|\d{1,2}|<([^>]+)>)/g, (all, token: string, name?: string) => {
    if (token === "$") return "$";
    if (token === "&") return m[0];
    if (name !== undefined) return m.groups?.[name] ?? "";
    const n = Number(token);
    // `$12` ohne zwölfte Gruppe heißt Gruppe 1 und danach eine 2, wie bei String.replace.
    if (n >= 1 && n < m.length) return m[n] ?? "";
    if (token.length === 2 && Number(token[0]) >= 1 && Number(token[0]) < m.length) return (m[Number(token[0])] ?? "") + token[1];
    return all;
  });
}

export interface HitPosition {
  line: number;
  col: number;
  endCol: number;
}

/**
 * Änderungen, die `spec` im Text vornimmt. Mit `only` nur für den einen Treffer an dieser Stelle.
 * Passt dort inzwischen nichts mehr, bleibt die Liste leer.
 */
export function replaceEdits(text: string, spec: ReplaceSpec, only?: HitPosition): TextEdit[] {
  if (!spec.query) return [];
  const re = buildRegex(spec);
  const edits: TextEdit[] = [];
  text.split("\n").forEach((raw, i) => {
    const line = i + 1;
    if (only && only.line !== line) return;
    const row = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    for (const m of row.matchAll(re)) {
      if (!m[0]) continue;
      const col = m.index ?? 0;
      if (only && (col !== only.col || col + m[0].length !== only.endCol)) continue;
      edits.push({ fromLine: line, fromCol: col, toLine: line, toCol: col + m[0].length, text: expandReplacement(spec.replacement, m, spec.regex) });
    }
  });
  return edits;
}
