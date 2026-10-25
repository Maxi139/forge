import { describe, expect, it } from "vitest";
import type { FunctionFacts, LspSymbol } from "../lib/backend";
import { currentSymbol, fallbackSymbols } from "./symbols";

const sym = (name: string, line: number, endLine: number, depth = 0): LspSymbol => ({ name, detail: null, kind: 12, line, col: 0, endLine, endCol: 0, depth, container: null });

describe("currentSymbol", () => {
  const list = [sym("setup", 3, 8), sym("tmp", 4, 4, 1), sym("loop", 10, 20)];
  it("findet die Funktion um die Zeile", () => {
    expect(currentSymbol(list, 6)?.name).toBe("setup");
    expect(currentSymbol(list, 15)?.name).toBe("loop");
  });
  it("bevorzugt das innerste Symbol", () => {
    expect(currentSymbol(list, 4)?.name).toBe("tmp");
  });
  it("liefert nichts zwischen den Funktionen", () => {
    expect(currentSymbol(list, 9)).toBeNull();
    expect(currentSymbol([], 1)).toBeNull();
  });
});

describe("fallbackSymbols", () => {
  it("nimmt nur Funktionen der Datei, sortiert nach Zeile", () => {
    const f = (file: string, name: string, startLine: number): FunctionFacts => ({ file, name, signature: `void ${name}()`, startLine, endLine: startLine + 2, hash: "", contexts: [], callers: [], callees: [], references: 0, globalsUsed: [], pins: [] });
    const out = fallbackSymbols([f("/a.ino", "loop", 9), f("/b.ino", "x", 1), f("/a.ino", "setup", 2)], "/a.ino");
    expect(out.map((s) => s.name)).toEqual(["setup", "loop"]);
    expect(out[0]).toMatchObject({ line: 2, endLine: 4, kind: 12 });
  });
});
