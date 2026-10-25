import { create } from "zustand";
import { backend } from "./backend";
import { useHardware } from "./hardware";
import { useStore } from "./store";

interface Lsp {
  /** clangd läuft und kennt den Sketch. */
  ready: boolean;
  /** Zählt jede erfolgreiche Synchronisierung. Anzeigen laden danach neu. */
  synced: number;
  /** Text jeder Datei beim letzten Abgleich. Antworten für einen anderen Stand werden verworfen. */
  texts: Record<string, string>;
  /** Gleicht clangd mit dem Editor ab. Das Ergebnis gilt für den Stand zum Aufruf oder neuer. */
  sync(): Promise<void>;
}

let running: Promise<void> | null = null;
let again = false;

export const useLsp = create<Lsp>((set) => ({
  ready: false,
  synced: 0,
  texts: {},

  sync() {
    const store = useStore.getState();
    const project = store.project;
    if (!project || !useHardware.getState().toolchain?.esp32Core) return Promise.resolve();
    // Läuft schon eine Synchronisierung, folgt genau eine weitere mit dem neuesten Stand.
    if (running) {
      again = true;
      return running;
    }
    running = (async () => {
      try {
        do {
          again = false;
          const s = useStore.getState();
          const board = s.project?.config.board;
          if (!board) return;
          const overrides: Record<string, string> = {};
          const texts: Record<string, string> = {};
          for (const [path, b] of Object.entries(s.buffers)) {
            if (b.readOnly) continue;
            texts[path] = b.text;
            if (b.text !== b.saved) overrides[path] = b.text;
          }
          const ok = await backend().lspSync({ fqbn: board.fqbn, options: board.options }, overrides);
          set((st) => ({ ready: ok, synced: ok ? st.synced + 1 : st.synced, texts }));
        } while (again);
      } catch (e) {
        console.debug("Sprachserver nicht bereit", e);
        set({ ready: false });
      } finally {
        running = null;
      }
    })();
    return running;
  },
}));
