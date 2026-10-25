import { useEffect } from "react";
import { undo, redo } from "@codemirror/commands";
import { EditorView } from "@codemirror/view";
import { t } from "../i18n/de";
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

export function useNativeEvents() {
  // Menüleiste
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void backend()
      .onMenu((id) => {
        if (id === "edit.undo") return runEdit("undo");
        if (id === "edit.redo") return runEdit("redo");
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
      .onProjectChanged((paths) => {
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
