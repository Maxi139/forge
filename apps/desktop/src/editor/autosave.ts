import { useEffect } from "react";
import { setAfterSave, setBeforeSave, useStore } from "../lib/store";
import { onFileSaved } from "../lib/web/sync";
import { FORMATTABLE, formatFile, useFormat } from "./format";
import { AUTOSAVE_DELAY, useEditorSettings } from "./settings";

function saveDirty() {
  const s = useStore.getState();
  for (const p of s.tabs) if (!s.buffers[p]?.readOnly && s.isDirty(p)) void s.save(p);
}

/** Automatisch speichern und „Beim Speichern formatieren“. */
export function useEditorHooks() {
  const autosave = useEditorSettings((st) => st.settings.autosave);
  // Ändert sich, sobald sich ein Puffer ändert. Jede Eingabe startet die Pause neu.
  const signature = useStore((s) => s.tabs.map((p) => `${p}:${s.buffers[p]?.version ?? 0}`).join("|"));

  useEffect(() => {
    // Formatiert wird nur beim ausdrücklichen Speichern der aktiven Datei, nicht im Hintergrund.
    setBeforeSave(async (path) => {
      const { formatOnSave } = useEditorSettings.getState().settings;
      if (!formatOnSave || !FORMATTABLE.test(path) || useFormat.getState().available === false) return;
      await formatFile(path, { quiet: true });
    });
    // Eine gespeicherte Webseite aktualisiert ihren Block in der .ino.
    setAfterSave(onFileSaved);
    void useFormat.getState().check();
    return () => {
      setBeforeSave(null);
      setAfterSave(null);
    };
  }, []);

  useEffect(() => {
    if (autosave !== "delay") return;
    const id = setTimeout(saveDirty, AUTOSAVE_DELAY);
    return () => clearTimeout(id);
  }, [autosave, signature]);

  useEffect(() => {
    if (autosave !== "blur") return;
    const onHide = () => {
      if (document.hidden || !document.hasFocus()) saveDirty();
    };
    window.addEventListener("blur", onHide);
    document.addEventListener("visibilitychange", onHide);
    return () => {
      window.removeEventListener("blur", onHide);
      document.removeEventListener("visibilitychange", onHide);
    };
  }, [autosave]);
}
