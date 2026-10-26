import { describe, expect, it } from "vitest";
import { checkWeb } from "./check";

const msgs = (file: string, text: string, exists?: (p: string) => boolean) => checkWeb(file, text, { exists });

describe("Webdateien prüfen", () => {
  it("lässt sauberen Code in Ruhe, auch mit moderner Syntax und Platzhaltern", () => {
    expect(msgs("a.html", '<!doctype html><title>x</title><ul><li>a<li>b</ul><p>{{temp}}</p><script>let t = {{temp}}; const a = t?.x ?? 2;</script><style>p{color:{{farbe}}}</style>')).toEqual([]);
    expect(msgs("a.js", "class A { #p = 1; static { } }\nconst x = a?.b ?? 2;\nawait go();")).toEqual([]);
    expect(msgs("a.css", "a { color: red }\n@media (max-width:600px){ a{ b:c } }")).toEqual([]);
  });

  it("meldet ein falsch geschlossenes Tag mit Zeile", () => {
    const d = msgs("a.html", "<div>\n<span>Hi</div></span>\n");
    const bad = d.find((x) => x.message.includes("</span>"));
    expect(bad).toMatchObject({ line: 2, severity: "error", source: "web", file: "a.html" });
    expect(bad!.to).toBeGreaterThan(bad!.from);
  });

  it("sagt, in welcher Sprache der Fehler liegt", () => {
    expect(msgs("a.html", "<script>\nfunction a( { x }\n</script>")[0].message).toContain("JavaScript");
    expect(msgs("a.html", "<style>a { color: red; </style>")[0].message).toContain("CSS");
    expect(msgs("a.js", "const a = 1;\nfunction f( {\n}")[0]).toMatchObject({ line: 3, message: expect.stringContaining("JavaScript") });
    expect(msgs("a.css", "a { color: red; \n b { }")[0].message).toContain("CSS");
  });

  it("meldet pro Zeile nur einen Fehler", () => {
    const d = msgs("a.js", "const a = (1 + ;");
    expect(new Set(d.map((x) => x.line)).size).toBe(d.length);
  });

  it("warnt vor Dateien im Netz und vor fehlenden Dateien", () => {
    const html = '<link rel="stylesheet" href="https://cdn.example/x.css">\n<script src="gibtsnicht.js"></script>\n<script src="da.js"></script>';
    const d = msgs("web/i.html", html, (p) => p === "web/da.js");
    expect(d).toHaveLength(2);
    expect(d[0]).toMatchObject({ line: 1, severity: "warning" });
    expect(d[0].message).toContain("kein Internet");
    expect(d[1]).toMatchObject({ line: 2, severity: "warning" });
    expect(d[1].message).toContain("gibtsnicht.js");
  });

  it("prüft ohne Angabe nicht auf fehlende Dateien", () => {
    expect(msgs("web/i.html", '<script src="gibtsnicht.js"></script>')).toEqual([]);
  });

  it("ignoriert andere Dateien", () => {
    expect(msgs("a.ino", "void setup( {")).toEqual([]);
  });
});
