import { useEffect } from "react";
import { undo, redo } from "@codemirror/commands";
import { findNext, findPrevious, openSearchPanel } from "@codemirror/search";
import { EditorView } from "@codemirror/view";
import { t } from "../i18n/de";
import { startupUpdateCheck } from "../lib/appUpdate";
import { backend } from "../lib/backend";
import { baseName, useStore } from "../lib/store";
import { buildCommands } from "./commands";

/** Befehle, die nur aus der Menüleiste kommen und keinen Eintrag in der Palette brauchen. */
function runEdit(kind: "undo" | "redo") {
  const cm = document.activeElement?.closest(".cm-editor");
  if (cm) {
    const view = EditorView.findFromDOM(cm as HTMLElement);
    if (view) return void (kind === "undo" ? undo(view) : redo(view));
  }
  document.execCommand(kind);
}

/** Suchen im aktiven Editor. Ohne Editor im Fokus nimmt der letzte Editor des Fensters die Anfrage. */
function runFind(kind: "find" | "next" | "previous") {
  const dom = document.activeElement?.closest(".cm-editor") ?? document.querySelector(".cm-editor");
  const view = dom ? EditorView.findFromDOM(dom as HTMLElement) : null;
  if (!view) return;
  view.focus();
  if (kind === "find") openSearchPanel(view);
  else if (kind === "next") findNext(view);
  else findPrevious(view);
}

export function useNativeEvents() {
  useEffect(() => startupUpdateCheck(), []);

  // Menüleiste
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void backend()
      .onMenu((id) => {
        if (id === "edit.undo") return runEdit("undo");
        if (id === "edit.redo") return runEdit("redo");
        if (id === "edit.find") return runFind("find");
        if (id === "edit.findNext") return runFind("next");
        if (id === "edit.findPrevious") return runFind("previous");
        if (id === "view.palette") return useStore.getState().togglePalette(true);
        buildCommands().find((c) => c.id === id)?.run();
      })
      .then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  // Menüeinträge, die nur für bestimmte Projekte gelten, sind sonst ausgegraut.
  useEffect(() => {
    const apply = () => {
      const p = useStore.getState().project;
      void backend()
        .setMenuEnabled({
          "file.convertToProject": p?.kind === "sketch",
          "file.convertToPackage": !!p && !p.package,
          "file.saveAs": !!p,
          "file.exportFolder": !!p?.package,
        })
        .catch(() => undefined);
    };
    apply();
    return useStore.subscribe((s, prev) => {
      if (s.project?.kind !== prev.project?.kind || s.project?.package !== prev.project?.package || !!s.project !== !!prev.project) apply();
    });
  }, []);

  // Die Projektdatei konnte nicht geschrieben werden (Platte voll, Datei gesperrt, Laufwerk getrennt).
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void backend()
      .onPackageError((message) => {
        useStore.getState().toast({ kind: "error", title: t.package.saveError, message });
      })
      .then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  // Änderungen von außen
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void backend()
      .onProjectChanged(async (paths) => {
        const s = useStore.getState();
        if (!s.project) return;
        clearTimeout(timer);
        timer = setTimeout(() => void s.refreshTree(), 120);
        for (const path of paths) {
          const b = s.buffers[path];
          if (!b) continue;
          if (b.text === b.saved) {
            void s.reloadFromDisk(path);
          } else {
            const name = baseName(path);
            // Auch das eigene Speichern löst den Wächter aus. Gewarnt wird nur, wenn auf der Platte wirklich etwas Fremdes steht.
            const disk = await backend().readFile(path).catch(() => null);
            const cur = useStore.getState().buffers[path];
            if (disk == null || !cur || disk === cur.saved || disk === cur.text) continue;
            s.toast({
              kind: "warning",
              title: t.external.changedTitle(name),
              message: t.external.changedText,
              actions: [
                {
                  label: t.external.reload,
                  run: () => {
                    // Ungespeicherte Änderungen verwerfen und die Datei von der Platte laden.
                    useStore.setState((st) => ({ buffers: { ...st.buffers, [path]: { ...st.buffers[path], saved: st.buffers[path].text } } }));
                    void useStore.getState().reloadFromDisk(path);
                  },
                },
                { label: t.external.keep, run: () => undefined },
              ],
            });
          }
        }
      })
      .then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
      clearTimeout(timer);
    };
  }, []);
}
