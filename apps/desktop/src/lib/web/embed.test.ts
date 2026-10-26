import { describe, expect, it } from "vitest";
import { checkWeb } from "./check";
import {
  buildPage,
  findBlocks,
  fillPlaceholders,
  generate,
  inlineAssets,
  insertBlock,
  minifyCss,
  minifyHtml,
  minifyJs,
  placeBlock,
  placeholders,
  references,
  replaceBlocks,
  resolveLocal,
  suggestName,
  webKind,
} from "./embed";

const files = (o: Record<string, string>) => new Map(Object.entries(o));

describe("Pfade", () => {
  it("löst lokale Verweise auf und lässt Adressen im Netz in Ruhe", () => {
    expect(resolveLocal("web/index.html", "style.css")).toBe("web/style.css");
    expect(resolveLocal("web/index.html", "./js/app.js?v=2")).toBe("web/js/app.js");
    expect(resolveLocal("web/index.html", "/app.js")).toBe("web/app.js");
    expect(resolveLocal("web/index.html", "../shared.css")).toBe("shared.css");
    expect(resolveLocal("web/index.html", "https://cdn.example/x.js")).toBeNull();
    expect(resolveLocal("web/index.html", "//cdn.example/x.js")).toBeNull();
    expect(resolveLocal("web/index.html", "#top")).toBeNull();
  });

  it("erkennt Webdateien", () => {
    expect(webKind("a/index.html")).toBe("html");
    expect(webKind("style.CSS")).toBe("css");
    expect(webKind("app.mjs")).toBe("js");
    expect(webKind("sketch.ino")).toBeNull();
  });
});

describe("Dateien einsetzen", () => {
  const page = '<html><head><link rel="stylesheet" href="style.css"></head><body><script src="app.js"></script></body></html>';

  it("setzt Stylesheet und Skript in die Seite", () => {
    const { html, missing } = inlineAssets(page, "web/index.html", files({ "web/style.css": "body{margin:0}", "web/app.js": "console.log(1)" }));
    expect(missing).toEqual([]);
    expect(html).toContain("<style>\nbody{margin:0}\n</style>");
    expect(html).toContain("<script>\nconsole.log(1)\n</script>");
    expect(html).not.toContain("href=");
  });

  it("meldet fehlende Dateien und lässt den Verweis stehen", () => {
    const { html, missing } = inlineAssets(page, "web/index.html", files({}));
    expect(missing).toEqual(["web/style.css", "web/app.js"]);
    expect(html).toContain('href="style.css"');
  });

  it("verhindert, dass ein Skript die Seite vorzeitig beendet", () => {
    const { html } = inlineAssets('<script src="a.js"></script>', "index.html", files({ "a.js": 'document.write("</script>")' }));
    expect(html.match(/<\/script>/g)).toHaveLength(1);
  });

  it("wartet bei defer auf die Seite", () => {
    const { html } = inlineAssets('<script defer src="a.js"></script>', "index.html", files({ "a.js": "init()" }));
    expect(html).toContain("DOMContentLoaded");
    expect(html).not.toContain("defer");
  });

  it("zählt externe Verweise nicht als fehlend", () => {
    const refs = references('<script src="https://cdn.example/x.js"></script><link rel="stylesheet" href="a.css">', "index.html");
    expect(refs.map((r) => [r.kind, r.external, r.path])).toEqual([
      ["js", true, null],
      ["css", false, "a.css"],
    ]);
  });
});

describe("Verkleinern", () => {
  it("entfernt Kommentare und Leerraum, schont aber vorformatierten Text", () => {
    const out = minifyHtml("<div>\n  <!-- weg -->\n  <p>Hallo   Welt</p>\n</div>\n<pre>a\n  b</pre>");
    expect(out).toBe("<div><p>Hallo Welt</p></div><pre>a\n  b</pre>");
  });

  it("lässt Text in Anführungszeichen im CSS unberührt", () => {
    expect(minifyCss("a  {\n  content: 'x   y';\n  color: red;\n}\n/* c */")).toBe("a{content: 'x   y';color: red}");
  });

  it("verändert JavaScript mit Schablonentext nicht", () => {
    const js = "const t = `a\n   b`;";
    expect(minifyJs(js)).toBe(js);
    expect(minifyJs("  // Kommentar\n  let a = 1;\n\n  a++;")).toBe("let a = 1;\na++;");
  });

  it("verkleinert Stile und Skripte in der Seite", () => {
    const out = minifyHtml("<style>\n  a { color: red; }\n</style>\n<script>\n  // x\n  go();\n</script>");
    expect(out).toContain("<style>a{color: red}</style>");
    expect(out).toContain("<script>\ngo();\n</script>");
  });

  it("buildPage führt zusammen und verkleinert auf Wunsch", () => {
    const f = files({ "w/a.css": "a { b: c; }" });
    const input = '<link rel="stylesheet" href="a.css">\n<p>x</p>';
    expect(buildPage(input, "w/i.html", f, false).html).toBe("<style>\na { b: c; }\n</style>\n<p>x</p>");
    expect(buildPage(input, "w/i.html", f, true).html).toBe("<style>a{b: c}</style><p>x</p>");
  });
});

describe("Erzeugter Code", () => {
  it("macht aus einer festen Seite eine Konstante", () => {
    const g = generate("<h1>Hallo</h1>", { source: "web/index.html", name: "INDEX_HTML", minify: false });
    expect(g.code).toBe(
      [
        "// >>> forge:web web/index.html name=INDEX_HTML",
        "// Erzeugt aus web/index.html. Änderungen gehören in die Webdatei, sie überschreibt diesen Block beim Speichern.",
        'const char INDEX_HTML[] PROGMEM = R"forge(<h1>Hallo</h1>)forge";',
        "// <<< forge:web",
      ].join("\n"),
    );
    expect(g.bytes).toBe(14);
    expect(g.params).toEqual([]);
  });

  it("wählt einen Begrenzer, der im Text nicht vorkommt", () => {
    const g = generate('<p>)forge"</p>', { source: "a.html", name: "A", minify: true });
    expect(g.code).toContain('R"forge1(<p>)forge"</p>)forge1"');
    expect(g.code).toContain("name=A minify=1");
  });

  it("macht aus Platzhaltern eine Funktion", () => {
    const html = "<p>{{temp}} Grad, LED {{ led }}</p><i>{{temp}}</i>";
    expect(placeholders(html)).toEqual(["temp", "led"]);
    const g = generate(html, { source: "i.html", name: "renderIndex", minify: false });
    expect(g.params).toEqual(["temp", "led"]);
    expect(g.code).toContain("String renderIndex(const String& temp, const String& led) {");
    expect(g.code).toContain('html += F(R"forge(<p>)forge");');
    expect(g.code).toContain("html += temp;");
    expect(g.code).toContain('html += F(R"forge(</i>)forge");');
    expect(g.code.match(/html \+= temp;/g)).toHaveLength(2);
    expect(g.code).toContain("return html;");
  });

  it("ersetzt Platzhalter in der Vorschau", () => {
    expect(fillPlaceholders("{{a}} und {{b}}", { a: "1" })).toBe("1 und ‹b›");
  });

  it("schlägt Namen vor", () => {
    expect(suggestName("web/index.html", false)).toBe("INDEX_HTML");
    expect(suggestName("web/mein-seite.html", true)).toBe("renderMeinSeite");
    expect(suggestName("web/1.html", false)).toBe("_1_HTML");
  });
});

describe("Blöcke im Sketch", () => {
  const spec = { source: "web/index.html", name: "INDEX_HTML", minify: false };
  const block = (page: string) => generate(page, spec).code;
  const sketch = (page: string) => `#include <WebServer.h>\n\n${block(page)}\n\nvoid setup() {\n}\nvoid loop() {}\n`;

  it("findet Blöcke samt Optionen", () => {
    const found = findBlocks(`x\n${generate("<b>", { source: "w/a.html", name: "SEITE", minify: true }).code}\ny`);
    expect(found).toHaveLength(1);
    expect(found[0]).toMatchObject({ source: "w/a.html", name: "SEITE", minify: true });
  });

  it("ignoriert einen Anfang ohne Ende", () => {
    expect(findBlocks("// >>> forge:web web/a.html name=A\nint x;\n")).toEqual([]);
  });

  it("ersetzt nur den Block und nichts darum herum", () => {
    const before = sketch("<p>alt</p>");
    const after = replaceBlocks(before, (b) => generate("<p>neu</p>", { source: b.source, name: b.name, minify: b.minify }).code);
    expect(after).toBe(sketch("<p>neu</p>"));
    expect(after).toContain("void setup()");
  });

  it("fügt einen neuen Block vor setup() ein, einen vorhandenen aktualisiert es", () => {
    const text = "#include <WebServer.h>\n\nvoid setup() {\n}\n";
    const first = placeBlock(text, spec, block("<p>1</p>"));
    expect(first.replaced).toBe(false);
    expect(first.text.indexOf("forge:web")).toBeLessThan(first.text.indexOf("void setup()"));
    const second = placeBlock(first.text, spec, block("<p>2</p>"));
    expect(second.replaced).toBe(true);
    expect(second.text.match(/>>> forge:web/g)).toHaveLength(1);
    expect(second.text).toContain("<p>2</p>");
  });

  it("hängt an, wenn es kein setup() gibt", () => {
    expect(insertBlock("int x;\n", "// block")).toBe("int x;\n\n// block\n");
  });
});

describe("Prüfung", () => {
  const msgs = (file: string, text: string, exists?: (p: string) => boolean) => checkWeb(file, text, { exists }).map((d) => `${d.line}:${d.severity}:${d.message.slice(0, 24)}`);

  it("lässt saubere Dateien in Ruhe, auch mit Platzhaltern in Skript und Stil", () => {
    const html = "<style>a{color:{{farbe}}}</style><div>{{x}}</div><script>var t = {{temp}};</script>";
    expect(checkWeb("w/i.html", html)).toEqual([]);
    expect(checkWeb("w/a.js", "const a = x?.y ?? 2;\nclass A { #p = 1 }\nawait 1;")).toEqual([]);
    expect(checkWeb("w/a.css", "a { color: red }\n@media (max-width:600px){ a{ b:c } }")).toEqual([]);
  });

  it("meldet falsch verschachtelte Tags mit Zeile", () => {
    const out = checkWeb("i.html", "<div>\n<span>Hi</div>\n</span>");
    expect(out.some((d) => d.severity === "error" && d.line === 3 && /passt zu keinem/.test(d.message))).toBe(true);
  });

  it("findet Fehler in Skript, Stil und eigenen Dateien", () => {
    expect(checkWeb("i.html", "<div>\n<script>\nfunction a( { x }\n</script></div>").some((d) => d.line === 3 && /JavaScript/.test(d.message))).toBe(true);
    expect(checkWeb("i.html", "<style>\na { color: red;\n</style>").some((d) => /CSS/.test(d.message))).toBe(true);
    expect(checkWeb("a.js", "const a = 1;\nfunction f( {\n}").some((d) => d.severity === "error")).toBe(true);
    expect(checkWeb("a.css", "a { color: red;\n b { }").some((d) => d.severity === "error")).toBe(true);
  });

  it("warnt vor externen und fehlenden Dateien", () => {
    const html = '<script src="https://cdn.example/x.js"></script>\n<link rel="stylesheet" href="fehlt.css">';
    const out = msgs("web/i.html", html, () => false);
    expect(out).toEqual(["1:warning:Externe Datei (https://c", "2:warning:Die Datei fehlt.css gibt"]);
    expect(msgs("web/i.html", '<link rel="stylesheet" href="a.css">', (p) => p === "web/a.css")).toEqual([]);
  });

  it("legt Spalten in die Zeile", () => {
    for (const d of checkWeb("a.js", "let a = (;\n")) {
      expect(d.from).toBeLessThan(d.to);
      expect(d.line).toBe(1);
    }
  });
});
