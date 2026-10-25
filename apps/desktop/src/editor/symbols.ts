import { useEffect, useState } from "react";
import { backend, type FunctionFacts, type LspSymbol } from "../lib/backend";
import { useLsp } from "../lib/lsp";
import { useStore } from "../lib/store";

/** Ohne Sprachserver kennt Forge aus der eigenen Analyse nur die Funktionen. */
export function fallbackSymbols(facts: FunctionFacts[], file: string): LspSymbol[] {
  return facts
    .filter((f) => f.file === file)
    .map((f) => ({ name: f.name, detail: f.signature, kind: 12, line: f.startLine, col: 0, endLine: f.endLine, endCol: 0, depth: 0, container: null }))
    .sort((a, b) => a.line - b.line);
}

/** Das innerste Symbol, in dem die Zeile liegt. */
export function currentSymbol(symbols: LspSymbol[], line: number): LspSymbol | null {
  let best: LspSymbol | null = null;
  for (const s of symbols) {
    if (line < s.line || line > s.endLine) continue;
    if (!best || s.depth > best.depth || (s.depth === best.depth && s.line >= best.line)) best = s;
  }
  return best;
}

export async function loadSymbols(file: string): Promise<LspSymbol[]> {
  try {
    const res = await backend().lspSymbols(file);
    if (res) return res;
  } catch {
    /* Dann hilft die eigene Funktionsliste. */
  }
  return fallbackSymbols(useStore.getState().facts, file);
}

/** Gliederung der Datei. Lädt neu, wenn clangd abgeglichen wurde oder die Analyse neue Fakten hat. */
export function useSymbols(file: string | null): LspSymbol[] {
  const [symbols, setSymbols] = useState<LspSymbol[]>([]);
  const synced = useLsp((s) => s.synced);
  const facts = useStore((s) => s.facts);
  useEffect(() => {
    if (!file) return setSymbols([]);
    let alive = true;
    const id = setTimeout(() => void loadSymbols(file).then((s) => alive && setSymbols(s)), 150);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [file, synced, facts]);
  return symbols;
}
