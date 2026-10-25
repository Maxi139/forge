import { create } from "zustand";
import type { SearchHit } from "./backend";

export interface ReferencesResult {
  /** Der gesuchte Name. */
  name: string;
  hits: SearchHit[];
}

/** Ergebnis von „Verwendungen anzeigen“. Die Suchansicht zeigt es über den normalen Treffern. */
export const useReferences = create<{ result: ReferencesResult | null; set(r: ReferencesResult | null): void }>((set) => ({
  result: null,
  set: (result) => set({ result }),
}));
