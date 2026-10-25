import { useEffect } from "react";
import { backend } from "../lib/backend";
import { useStore } from "../lib/store";
import { buildCommands } from "./commands";

/**
 * Tastenkürzel für Browser-Vorschau und Tests. In der App verwaltet macOS sie über die Menüleiste
 * (siehe `menu.rs`), dort würden sie doppelt auslösen.
 */
export function useShortcuts() {
  // Tasten, die der Editor selbst abfängt (F2, ⇧F12, ⇧⌥F). Sie gelten in der App wie im Browser.
  useEffect(() => {
    const on = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      buildCommands().find((c) => c.id === id)?.run();
    };
    window.addEventListener("forge:run-command", on);
    return () => window.removeEventListener("forge:run-command", on);
  }, []);

  useEffect(() => {
    if (backend().native) return;
    const onKey = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      // Mit ⌥ liefert `e.key` auf macOS Sonderzeichen („ß“ statt „s“), darum zählt dann die Taste selbst.
      const key = e.altKey && e.code.startsWith("Key") ? e.code.slice(3).toLowerCase() : e.key.toLowerCase();
      const s = useStore.getState();
      const run = (id: string) => buildCommands().find((c) => c.id === id)?.run();

      if (key === "," && !e.shiftKey && !e.altKey) {
        e.preventDefault();
        run("app.settings");
      } else if (key === "k" && !e.shiftKey) {
        e.preventDefault();
        s.togglePalette();
      } else if (key === "j") {
        e.preventDefault();
        run("view.panel");
      } else if (key === "\\") {
        e.preventDefault();
        run("view.sidebar");
      } else {
        const map: Record<string, string> = e.shiftKey
          ? { n: "file.newProject", m: "view.monitor", f: "view.search", i: "ai.task", s: "file.saveAs", o: "editor.symbols" }
          : e.altKey
            ? { s: "file.saveAll", d: "view.device" }
            : { n: "file.newSketch", o: "file.open", s: "file.save", w: "file.close", u: "build.flash", b: "build.compile", p: "file.quickOpen" };
        const id = map[key];
        if (id) {
          e.preventDefault();
          run(id);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
