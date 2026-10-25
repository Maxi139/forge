import { describe, expect, it } from "vitest";
import { applyTextEdits, diffEdit } from "./textEdit";

describe("diffEdit", () => {
  it("liefert nichts bei gleichem Text", () => {
    expect(diffEdit("a\nb", "a\nb")).toBeNull();
  });

  it("beschränkt sich auf die geänderte Stelle", () => {
    const before = "void setup() {\nSerial.begin(9600);\n}\n";
    const after = "void setup() {\n  Serial.begin(9600);\n}\n";
    const edit = diffEdit(before, after)!;
    expect(edit).toEqual({ fromLine: 2, fromCol: 0, toLine: 2, toCol: 0, text: "  " });
    expect(applyTextEdits(before, [edit])).toBe(after);
  });

  it("deckt Änderungen über mehrere Zeilen ab", () => {
    const before = "int a;\nint b;\nint c;\n";
    const after = "int a;\nint renamed;\nint c;\n";
    expect(applyTextEdits(before, [diffEdit(before, after)!])).toBe(after);
  });

  it("verträgt Löschen und Anhängen", () => {
    expect(applyTextEdits("abc", [diffEdit("abc", "")!])).toBe("");
    expect(applyTextEdits("", [diffEdit("", "x\ny")!])).toBe("x\ny");
  });
});

describe("applyTextEdits", () => {
  it("wendet mehrere Änderungen unabhängig von der Reihenfolge an", () => {
    const text = "foo(foo);\nfoo();\n";
    const e = (l: number, c: number) => ({ fromLine: l, fromCol: c, toLine: l, toCol: c + 3, text: "bar" });
    expect(applyTextEdits(text, [e(1, 0), e(2, 0), e(1, 4)])).toBe("bar(bar);\nbar();\n");
  });
});
