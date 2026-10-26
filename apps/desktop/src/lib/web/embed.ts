/**
 * Webdateien (HTML, CSS, JavaScript) für den Sketch aufbereiten: Dateien zusammenführen, verkleinern und als
 * C++-Text erzeugen, den man in die .ino setzt. Reine Funktionen, ohne Zugriff auf Dateien oder Oberfläche.
 */

export type WebKind = "html" | "css" | "js";

const KIND: Array<[RegExp, WebKind]> = [
  [/\.html?$/i, "html"],
  [/\.css$/i, "css"],
  [/\.m?js$/i, "js"],
];

export function webKind(path: string | null | undefined): WebKind | null {
  if (!path) return null;
  return KIND.find(([re]) => re.test(path))?.[1] ?? null;
}

export const isHtml = (path: string | null | undefined) => webKind(path) === "html";

/* ---------- Pfade ---------- */

export function dirOf(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

/** Setzt `.` und `..` um. `null`, wenn der Pfad über die Wurzel hinausführt. */
export function normalizePath(path: string): string | null {
  const out: string[] = [];
  for (const part of path.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (!out.length) return null;
      out.pop();
    } else out.push(part);
  }
  return (path.startsWith("/") ? "/" : "") + out.join("/");
}

/** Der Pfad einer lokalen Datei, auf die `ref` von `from` aus zeigt. `null` bei Adressen im Netz, Ankern und `data:`. */
export function resolveLocal(from: string, ref: string): string | null {
  const clean = ref.trim().split(/[?#]/)[0];
  if (!clean || /^[a-z][a-z0-9+.-]*:/i.test(clean) || clean.startsWith("//")) return null;
  // Ein führender Schrägstrich meint die Wurzel des Geräts, hier der Ordner der Seite.
  const base = dirOf(from);
  const rel = clean.replace(/^\/+/, "");
  return normalizePath(base ? `${base}/${rel}` : rel);
}

const isExternal = (ref: string) => /^(?:[a-z][a-z0-9+.-]*:)?\/\//i.test(ref.trim());

/* ---------- Tags ---------- */

/** Wert eines Attributs im Tag. `null`, wenn es fehlt, `""` bei Attributen ohne Wert. */
function attr(tag: string, name: string): string | null {
  const m = new RegExp(`(?<![\\w-])${name}(?:\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s"'>]+)))?(?=[\\s>/]|$)`, "i").exec(tag);
  if (!m) return null;
  return m[1] ?? m[2] ?? m[3] ?? "";
}

function dropAttr(attrs: string, name: string): string {
  return attrs.replace(new RegExp(`\\s+(?<![\\w-])${name}(?:\\s*=\\s*(?:"[^"]*"|'[^']*'|[^\\s"'>]+))?(?=[\\s>/]|$)`, "gi"), "");
}

const LINK = /<link\b[^>]*>/gi;
const SCRIPT = /<script\b([^>]*)>\s*<\/script\s*>/gi;

export interface Reference {
  /** Wie es im Tag steht. */
  ref: string;
  kind: "css" | "js";
  /** Der Pfad im Projekt, `null` bei Adressen im Netz. */
  path: string | null;
  external: boolean;
  /** Zeichenposition des Tags im HTML. */
  at: number;
}

/** Alle Stylesheets und Skripte, die die Seite einbindet. */
export function references(html: string, htmlPath: string): Reference[] {
  const out: Reference[] = [];
  for (const m of html.matchAll(LINK)) {
    const rel = attr(m[0], "rel");
    const href = attr(m[0], "href");
    if (!href || !rel || !rel.toLowerCase().split(/\s+/).includes("stylesheet")) continue;
    out.push({ ref: href, kind: "css", path: resolveLocal(htmlPath, href), external: isExternal(href), at: m.index ?? 0 });
  }
  for (const m of html.matchAll(SCRIPT)) {
    const src = attr(m[1], "src");
    if (!src) continue;
    out.push({ ref: src, kind: "js", path: resolveLocal(htmlPath, src), external: isExternal(src), at: m.index ?? 0 });
  }
  return out.sort((a, b) => a.at - b.at);
}

/** Setzt lokale Stylesheets und Skripte in die Seite ein. Was fehlt, bleibt als Verweis stehen und wird gemeldet. */
export function inlineAssets(html: string, htmlPath: string, files: ReadonlyMap<string, string>): { html: string; missing: string[] } {
  const missing: string[] = [];
  const find = (ref: string): string | null => {
    const path = resolveLocal(htmlPath, ref);
    if (path == null) return null;
    const text = files.get(path);
    if (text == null) {
      if (!missing.includes(path)) missing.push(path);
      return null;
    }
    return text;
  };

  let out = html.replace(LINK, (tag) => {
    const rel = attr(tag, "rel");
    const href = attr(tag, "href");
    if (!href || !rel || !rel.toLowerCase().split(/\s+/).includes("stylesheet")) return tag;
    const css = find(href);
    if (css == null) return tag;
    const media = attr(tag, "media");
    return `<style${media ? ` media="${media}"` : ""}>\n${css.trim()}\n</style>`;
  });

  out = out.replace(SCRIPT, (tag, attrs: string) => {
    const src = attr(attrs, "src");
    if (!src) return tag;
    const js = find(src);
    if (js == null) return tag;
    const module = (attr(attrs, "type") ?? "").toLowerCase() === "module";
    const deferred = attr(attrs, "defer") !== null && !module;
    let rest = dropAttr(dropAttr(dropAttr(attrs, "src"), "defer"), "async");
    rest = rest.trimEnd();
    // Ein eingesetztes Skript wartet nicht mehr auf die Seite. `defer` ersetzen wir durch das, was es bewirkt.
    const body = deferred ? `document.addEventListener("DOMContentLoaded",function(){\n${js.trim()}\n});` : js.trim();
    return `<script${rest}>\n${body.replace(/<\/(script)/gi, "<\\/$1")}\n</script>`;
  });
  return { html: out, missing };
}

/* ---------- Verkleinern ---------- */

/** Wendet `fn` nur auf Text außerhalb von Anführungszeichen an. */
function outsideQuotes(css: string, fn: (s: string) => string): string {
  return css
    .split(/("(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*')/g)
    .map((part, i) => (i % 2 ? part : fn(part)))
    .join("");
}

export function minifyCss(css: string): string {
  return outsideQuotes(css.replace(/\/\*[\s\S]*?\*\//g, ""), (s) => s.replace(/\s+/g, " ").replace(/\s*([{};,])\s*/g, "$1"))
    .replace(/;}/g, "}")
    .trim();
}

export function minifyJs(js: string): string {
  // In Schablonentexten (`) zählt jeder Zeilenumbruch und jede Einrückung. Dann bleibt der Code, wie er ist.
  if (js.includes("`")) return js.trim();
  return js
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l !== "" && !l.startsWith("//"))
    .join("\n");
}

export function minifyHtml(html: string): string {
  // Skripte, Stile und vorformatierter Text bleiben unberührt (Skripte und Stile werden getrennt verkleinert).
  const parts = html.split(/(<script\b[\s\S]*?<\/script\s*>|<style\b[\s\S]*?<\/style\s*>|<pre\b[\s\S]*?<\/pre\s*>|<textarea\b[\s\S]*?<\/textarea\s*>)/gi);
  const out = parts.map((part, i) => {
    if (i % 2 === 0) {
      let text = part.replace(/<!--(?!\[)[\s\S]*?-->/g, "").replace(/>\s*\n\s*</g, "><");
      // Zeilenumbrüche vor und hinter einem Skript, Stil oder vorformatierten Text sind nur Formatierung.
      if (i > 0) text = text.replace(/^\s*\n\s*/, "");
      if (i < parts.length - 1) text = text.replace(/\s*\n\s*$/, "");
      return text.replace(/\s+/g, " ");
    }
    const m = /^(<(script|style)\b[^>]*>)([\s\S]*?)(<\/\2\s*>)$/i.exec(part);
    if (!m) return part;
    const body = m[2].toLowerCase() === "style" ? minifyCss(m[3]) : /type\s*=\s*["'](?!module|text\/javascript)/i.test(m[1]) ? m[3].trim() : minifyJs(m[3]);
    return `${m[1]}${m[2].toLowerCase() === "style" ? body : `\n${body}\n`}${m[4]}`;
  });
  return out.join("").trim();
}

/** Erst Dateien zusammenführen, dann (auf Wunsch) verkleinern. Daraus wird der Text, den das Gerät ausliefert. */
export function buildPage(html: string, htmlPath: string, files: ReadonlyMap<string, string>, minify: boolean): { html: string; missing: string[] } {
  const inlined = inlineAssets(html, htmlPath, files);
  return { html: minify ? minifyHtml(inlined.html) : inlined.html.trim(), missing: inlined.missing };
}

/* ---------- Platzhalter ---------- */

const PLACEHOLDER = /\{\{\s*([A-Za-z_]\w*)\s*\}\}/g;

/** Namen der `{{platzhalter}}` in der Reihenfolge ihres ersten Auftretens. */
export function placeholders(html: string): string[] {
  const seen: string[] = [];
  for (const m of html.matchAll(PLACEHOLDER)) if (!seen.includes(m[1])) seen.push(m[1]);
  return seen;
}

/** Setzt Beispielwerte ein, damit die Vorschau wie die fertige Seite aussieht. */
export function fillPlaceholders(html: string, values: Record<string, string>): string {
  return html.replace(PLACEHOLDER, (_, name: string) => values[name] ?? `‹${name}›`);
}

/* ---------- C++ erzeugen ---------- */

const CPP_WORDS = new Set(
  "alignas alignof and auto bool break case catch char class const constexpr continue default delete do double else enum explicit export extern false float for friend goto if inline int long namespace new not nullptr operator or private protected public register return short signed sizeof static struct switch template this throw true try typedef typename union unsigned using virtual void volatile while String".split(" "),
);

export function cppIdentifier(raw: string, fallback = "WEB"): string {
  let id = raw.replace(/[^A-Za-z0-9_]/g, "_").replace(/^_+|_+$/g, "");
  if (!id) id = fallback;
  if (/^\d/.test(id) || CPP_WORDS.has(id)) id = `_${id}`;
  return id;
}

/** Vorschlag für den Namen: `web/index.html` wird `INDEX_HTML`, mit Platzhaltern `renderIndex`. */
export function suggestName(source: string, dynamic: boolean): string {
  const file = source.split("/").pop() ?? source;
  const stem = file.replace(/\.[^.]+$/, "");
  if (dynamic) {
    const words = stem.split(/[^A-Za-z0-9]+/).filter(Boolean);
    return cppIdentifier(`render${words.map((w) => w[0].toUpperCase() + w.slice(1)).join("") || "Page"}`);
  }
  return cppIdentifier(file.replace(/\.[^.]+$/, "").toUpperCase() + "_HTML");
}

/** Länge in Bytes, wie das Gerät den Text im Flash ablegt. */
export function utf8Length(text: string): number {
  let n = 0;
  for (const ch of text) {
    const c = ch.codePointAt(0) ?? 0;
    n += c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
  }
  return n;
}

/** Ein Begrenzer für Rohtext (`R"begrenzer(…)begrenzer"`), der im Text nicht vorkommt. */
function rawDelimiter(content: string): string {
  let d = "forge";
  for (let i = 1; content.includes(`)${d}"`); i++) d = `forge${i}`;
  return d;
}

const raw = (content: string) => {
  const d = rawDelimiter(content);
  return `R"${d}(${content})${d}"`;
};

export const BEGIN = "// >>> forge:web";
export const END = "// <<< forge:web";

export interface EmbedSpec {
  /** Pfad der Quelle, relativ zum Projekt. */
  source: string;
  name: string;
  minify: boolean;
}

export interface Generated {
  /** Der fertige Block samt Markierungen. */
  code: string;
  /** Größe der Seite im Flash. */
  bytes: number;
  /** Parameter der erzeugten Funktion, leer bei einer Konstante. */
  params: string[];
}

/**
 * Erzeugt den Block für die .ino. Ohne Platzhalter eine Konstante für `server.send_P`,
 * mit `{{platzhaltern}}` eine Funktion, die die Seite zusammensetzt und als String zurückgibt.
 */
export function generate(page: string, spec: EmbedSpec): Generated {
  const names = placeholders(page);
  const params = names.map((n) => cppIdentifier(n, "wert"));
  const head = `${BEGIN} ${spec.source} name=${spec.name}${spec.minify ? " minify=1" : ""}`;
  const note = `// Erzeugt aus ${spec.source}. Änderungen gehören in die Webdatei, sie überschreibt diesen Block beim Speichern.`;
  const lines = [head, note];

  if (!names.length) {
    lines.push(`const char ${spec.name}[] PROGMEM = ${raw(page)};`);
    lines.push(END);
    return { code: lines.join("\n"), bytes: utf8Length(page), params: [] };
  }

  const segments = page.split(PLACEHOLDER);
  // split mit Gruppe liefert abwechselnd Text und Name.
  const body: string[] = [];
  for (let i = 0; i < segments.length; i++) {
    if (i % 2 === 0) {
      if (segments[i]) body.push(`  html += F(${raw(segments[i])});`);
    } else body.push(`  html += ${params[names.indexOf(segments[i])]};`);
  }
  const fixed = segments.filter((_, i) => i % 2 === 0).reduce((n, s) => n + utf8Length(s), 0);
  lines.push(`String ${spec.name}(${params.map((p) => `const String& ${p}`).join(", ")}) {`);
  lines.push("  String html;");
  lines.push(`  html.reserve(${fixed + params.length * 16});`);
  lines.push(...body);
  lines.push("  return html;");
  lines.push("}");
  lines.push(END);
  return { code: lines.join("\n"), bytes: fixed, params };
}

/** Wie man die Seite ausliefert: der Satz, den man neben den Block schreibt. */
export function usageHint(name: string, params: string[]): string {
  return params.length
    ? `server.send(200, "text/html", ${name}(${params.map(() => "String(…)").join(", ")}));`
    : `server.send_P(200, "text/html", ${name});`;
}

/* ---------- Blöcke im Sketch ---------- */

export interface WebBlock {
  /** Zeichenposition des Anfangs (erste Zeile der Markierung) und des Endes (hinter der letzten Markierung). */
  from: number;
  to: number;
  source: string;
  name: string;
  minify: boolean;
}

/** Alle erzeugten Blöcke in einer Datei. Ein Anfang ohne Ende zählt nicht, damit nichts Fremdes überschrieben wird. */
export function findBlocks(text: string): WebBlock[] {
  const out: WebBlock[] = [];
  const lines = text.split("\n");
  let offset = 0;
  let open: { from: number; source: string; opts: string } | null = null;
  for (const line of lines) {
    const trimmed = line.trim();
    if (!open) {
      if (trimmed.startsWith(BEGIN) && /\s/.test(trimmed.charAt(BEGIN.length))) {
        const rest = trimmed.slice(BEGIN.length).trim();
        const source = rest.split(/\s+/)[0];
        if (source) open = { from: offset + (line.length - line.trimStart().length), source, opts: rest.slice(source.length) };
      }
    } else if (trimmed === END) {
      const opt = (key: string) => new RegExp(`(?:^|\\s)${key}=(\\S+)`).exec(open!.opts)?.[1];
      const name = cppIdentifier(opt("name") ?? suggestName(open.source, false));
      out.push({ from: open.from, to: offset + line.trimEnd().length, source: open.source, name, minify: opt("minify") === "1" });
      open = null;
    }
    offset += line.length + 1;
  }
  return out;
}

/** Ersetzt jeden Block durch das, was `render` liefert. `null` lässt den Block, wie er ist. */
export function replaceBlocks(text: string, render: (b: WebBlock) => string | null): string {
  let out = text;
  for (const b of findBlocks(text).reverse()) {
    const next = render(b);
    if (next != null) out = out.slice(0, b.from) + next + out.slice(b.to);
  }
  return out;
}

/** Setzt einen neuen Block vor `setup()` ein, oder ans Ende, wenn es kein `setup()` gibt. */
export function insertBlock(text: string, code: string): string {
  const at = /^[ \t]*void\s+setup\s*\(/m.exec(text);
  if (at) return `${text.slice(0, at.index)}${code}\n\n${text.slice(at.index)}`;
  return `${text.replace(/\s*$/, "")}\n\n${code}\n`;
}

/** Ersetzt den Block mit gleicher Quelle und gleichem Namen, sonst fügt es einen neuen ein. */
export function placeBlock(text: string, spec: EmbedSpec, code: string): { text: string; replaced: boolean } {
  const hit = findBlocks(text).find((b) => b.source === spec.source && b.name === spec.name);
  if (hit) return { text: text.slice(0, hit.from) + code + text.slice(hit.to), replaced: true };
  return { text: insertBlock(text, code), replaced: false };
}
