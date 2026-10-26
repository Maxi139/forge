import { cssLanguage } from "@codemirror/lang-css";
import { htmlLanguage } from "@codemirror/lang-html";
import { javascriptLanguage } from "@codemirror/lang-javascript";
import type { Diagnostic } from "../diagnostics";
import { references, webKind } from "./embed";

const MAX_PER_FILE = 12;

const MESSAGE = {
  html: "Syntaxfehler im HTML: Hier fehlt etwas, etwa ein schließendes Tag, ein Anführungszeichen oder ein „>“.",
  js: "Syntaxfehler in JavaScript. Prüfe Klammern, Anführungszeichen und Kommas.",
  css: "Syntaxfehler in CSS. Prüfe geschweifte Klammern und Semikolons.",
} as const;

/** `{{platzhalter}}` gleich lang durch einen Namen ersetzen, damit die Parser sie als Wert lesen und die Spalten stimmen. */
function neutralize(text: string): string {
  return text.replace(/\{\{\s*[A-Za-z_]\w*\s*\}\}/g, (m) => "_".repeat(m.length));
}

type Tree = ReturnType<typeof htmlLanguage.parser.parse>;

/** In welcher Sprache der Fehler liegt: In einer HTML-Datei können Skripte und Stile stecken. */
function languageAt(tree: Tree, pos: number, fallback: keyof typeof MESSAGE): keyof typeof MESSAGE {
  for (let n: ReturnType<Tree["resolveInner"]> | null = tree.resolveInner(Math.max(pos, 0), -1); n; n = n.parent) {
    if (n.name === "Script") return "js";
    if (n.name === "StyleSheet") return "css";
  }
  return fallback;
}

export interface CheckOptions {
  /** Ob die Datei im Projekt liegt. Ohne Angabe wird nicht auf fehlende Dateien geprüft. */
  exists?(path: string): boolean;
}

/** Strukturfehler, fehlende und externe Dateien in einer Webdatei. Läuft ohne Netz und ohne Gerät. */
export function checkWeb(file: string, text: string, opts: CheckOptions = {}): Diagnostic[] {
  const kind = webKind(file);
  if (!kind) return [];
  const lineStarts = [0];
  for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) lineStarts.push(i + 1);
  const place = (pos: number) => {
    let lo = 0;
    let hi = lineStarts.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (lineStarts[mid] <= pos) lo = mid;
      else hi = mid - 1;
    }
    return { line: lo + 1, col: pos - lineStarts[lo] };
  };
  const lineEnd = (line: number) => (lineStarts[line] ?? text.length + 1) - 1;

  const out: Diagnostic[] = [];
  const add = (from: number, to: number, severity: Diagnostic["severity"], message: string) => {
    const a = place(from);
    // Eine Meldung ohne Länge bekommt das nächste Zeichen, am Zeilenende das letzte.
    let end = Math.max(to, from + 1);
    let col = a.col;
    if (from >= lineEnd(a.line) && col > 0) {
      col -= 1;
      end = from;
    }
    const b = place(Math.min(end, lineEnd(a.line)));
    out.push({
      id: `web:${file}:${a.line}:${col}:${message.length}`,
      file,
      line: a.line,
      from: col,
      to: Math.max(b.line === a.line ? b.col : a.col + 1, col + 1),
      severity,
      message,
      source: "web",
    });
  };

  const parsed = neutralize(text);
  const tree = (kind === "html" ? htmlLanguage : kind === "css" ? cssLanguage : javascriptLanguage).parser.parse(parsed);
  // Meldungen zu einem Tag sind genauer als ein bloßer Fehlerknoten und gehen vor.
  const tags: Array<{ from: number; to: number; message: string }> = [];
  const generic: number[] = [];
  tree.iterate({
    enter(node) {
      if (node.name === "MismatchedCloseTag" || node.name === "MissingCloseTag") {
        const tag = text.slice(node.from, node.to).trim();
        tags.push({
          from: node.from,
          to: node.to,
          message: node.name === "MismatchedCloseTag" ? `Das schließende Tag ${tag} passt zu keinem offenen Tag.` : `${tag} wird nicht geschlossen.`,
        });
      } else if (node.type.isError) generic.push(node.from);
    },
  });
  const seenLines = new Set<number>();
  for (const t of tags) {
    const line = place(t.from).line;
    if (seenLines.has(line) || out.length >= MAX_PER_FILE) continue;
    seenLines.add(line);
    add(t.from, t.to, "error", t.message);
  }
  for (const found of generic) {
    // Der Parser merkt den Fehler erst, wo er nicht weiterkommt, oft am Anfang der nächsten Zeile. Das Ende der Zeile davor hilft mehr.
    const here = place(found);
    const from = here.col === 0 && here.line > 1 ? Math.max(lineStarts[here.line - 1] - 1, 0) : found;
    const line = place(from).line;
    if (seenLines.has(line) || out.length >= MAX_PER_FILE) continue;
    seenLines.add(line);
    add(from, from, "error", MESSAGE[languageAt(tree, found, kind)]);
  }

  if (kind === "html") {
    for (const r of references(text, file)) {
      if (r.external) {
        add(r.at, r.at + 1, "warning", `Externe Datei (${r.ref}): Im eigenen WLAN hat das Gerät oft kein Internet, dann lädt sie nicht. Besser liegt sie im Projekt.`);
      } else if (r.path && opts.exists && !opts.exists(r.path)) {
        add(r.at, r.at + 1, "warning", `Die Datei ${r.ref} gibt es im Projekt nicht, sie wird nicht eingesetzt.`);
      }
    }
  }
  return out;
}
