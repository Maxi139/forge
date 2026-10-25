import { describe, expect, it } from "vitest";
import { buildRegex, expandReplacement, replaceEdits, type ReplaceSpec } from "./replace";
import { applyTextEdits } from "./textEdit";

const spec = (over: Partial<ReplaceSpec>): ReplaceSpec => ({ query: "", replacement: "", caseSensitive: false, wholeWord: false, regex: false, ...over });
const run = (text: string, s: ReplaceSpec) => applyTextEdits(text, replaceEdits(text, s));

describe("Ersetzen im Projekt", () => {
  it("ersetzt wörtlich und ignoriert Groß-/Kleinschreibung", () => {
    expect(run("Led led LED\nled", spec({ query: "led", replacement: "pin" }))).toBe("pin pin pin\npin");
  });

  it("achtet auf Groß-/Kleinschreibung und ganze Wörter", () => {
    expect(run("led LED Led", spec({ query: "led", replacement: "x", caseSensitive: true }))).toBe("x LED Led");
    expect(run("pin pinMode pins", spec({ query: "pin", replacement: "p", wholeWord: true }))).toBe("p pinMode pins");
  });

  it("nimmt Sonderzeichen ohne Regex wörtlich, auch $", () => {
    expect(run("a.b a-b", spec({ query: "a.b", replacement: "$1" }))).toBe("$1 a-b");
  });

  it("setzt Gruppen mit Regex ein", () => {
    const s = spec({ query: "digitalWrite\\((\\w+),\\s*(\\w+)\\)", replacement: "set($2, $1)", regex: true });
    expect(run("digitalWrite(LED, HIGH);", s)).toBe("set(HIGH, LED);");
    expect(run("x", spec({ query: "(x)", replacement: "$$1 $&", regex: true }))).toBe("$1 x");
  });

  it("unterscheidet $12 ohne zwölfte Gruppe von Gruppe 1 plus Ziffer", () => {
    const m = "ab".match(/(a)(b)/)!;
    expect(expandReplacement("$12", m, true)).toBe("a2");
    expect(expandReplacement("$3", m, true)).toBe("$3");
  });

  it("ersetzt mit Regex mehrere Treffer je Zeile und über mehrere Zeilen", () => {
    expect(run("a1 a2\na3", spec({ query: "a(\\d)", replacement: "b$1", regex: true }))).toBe("b1 b2\nb3");
  });

  it("ersetzt nur den gewählten Treffer und nur, wenn er noch dort steht", () => {
    const text = "foo foo\nfoo";
    const s = spec({ query: "foo", replacement: "bar" });
    expect(applyTextEdits(text, replaceEdits(text, s, { line: 1, col: 4, endCol: 7 }))).toBe("foo bar\nfoo");
    expect(replaceEdits(text, s, { line: 2, col: 1, endCol: 4 })).toEqual([]);
  });

  it("meldet einen ungültigen Ausdruck", () => {
    expect(() => buildRegex({ query: "(", regex: true, caseSensitive: false, wholeWord: false })).toThrow("ungültig");
  });

  it("überspringt leere Treffer und verträgt Windows-Zeilenenden", () => {
    expect(replaceEdits("abc", spec({ query: "x*", replacement: "-", regex: true }))).toEqual([]);
    expect(run("a;\r\nb;\r\n", spec({ query: ";$", replacement: "!", regex: true }))).toBe("a!\r\nb!\r\n");
  });
});
