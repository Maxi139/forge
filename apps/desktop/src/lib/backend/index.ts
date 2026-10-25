import type { Backend } from "./backend";
import { createMockBackend } from "./mock";
import { createTauriBackend } from "./tauri";

export type { Backend, Unsubscribe } from "./backend";
export * from "./types";

let instance: Backend | null = null;

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** In der App das echte Backend, im Browser und in Tests der Mock. */
export function backend(): Backend {
  if (!instance) {
    if (isTauri()) {
      instance = createTauriBackend();
    } else {
      const mock = createMockBackend();
      // Zum Ausprobieren im Browser: `__forgeMock.unplug()`, `__forgeMock.plug()`.
      if (import.meta.env.DEV) (window as unknown as { __forgeMock: unknown }).__forgeMock = mock.control;
      instance = mock;
    }
  }
  return instance;
}

/** Nur für Tests. */
export function setBackend(b: Backend | null) {
  instance = b;
}
