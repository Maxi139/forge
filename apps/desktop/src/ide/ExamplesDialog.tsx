import { Button, Dialog, Icon, Input } from "@forge/ui";
import { useEffect, useMemo, useState } from "react";
import { t } from "../i18n/de";
import { backend, type ExampleInfo } from "../lib/backend";
import { errorText, useStore } from "../lib/store";
import "../hardware.css";

/** Beispiele von Core und Bibliotheken. Ein Klick öffnet eine Kopie als neuen Sketch. */
export function ExamplesDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const project = useStore((s) => s.project);
  const [examples, setExamples] = useState<ExampleInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [opening, setOpening] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    // Mit geöffnetem Projekt nur die Beispiele, die zum Board passen.
    backend()
      .examples(project?.config.board.fqbn)
      .then((r) => alive && setExamples(r))
      .catch((e) => alive && (setError(errorText(e)), setExamples([])));
    return () => {
      alive = false;
    };
  }, [project?.config.board.fqbn]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (examples ?? []).filter((e) => !q || e.name.toLowerCase().includes(q) || e.library.toLowerCase().includes(q));
  }, [examples, query]);

  const open = async (e: ExampleInfo) => {
    setOpening(e.path);
    const ok = await useStore.getState().openExample(e.path);
    setOpening(null);
    if (ok) setDialog(null);
  };

  const groups: Array<[string, ExampleInfo[]]> = [
    [t.examples.core, shown.filter((e) => e.source === "core")],
    [t.examples.libraries, shown.filter((e) => e.source === "library")],
  ];

  return (
    <Dialog
      title={t.examples.title}
      icon="file-code"
      width={480}
      onClose={() => setDialog(null)}
      footer={<Button onClick={() => setDialog(null)}>{t.dialogs.cancel}</Button>}
    >
      <div className="examples">
        <Input icon="search" size="sm" autoFocus placeholder={t.examples.search} aria-label={t.examples.search} value={query} onChange={(e) => setQuery(e.target.value)} />
        <div className="examples-list">
          {examples === null ? <div className="sb-empty">{t.examples.loading}</div> : null}
          {error ? <div className="sb-empty">{t.examples.loadFailed}: {error}</div> : null}
          {examples && !error && examples.length === 0 ? <div className="sb-empty">{t.examples.none}</div> : null}
          {examples && examples.length > 0 && shown.length === 0 ? <div className="sb-empty">{t.examples.noMatch(query.trim())}</div> : null}
          {groups.map(([label, list]) =>
            list.length ? (
              <div key={label}>
                <div className="examples-group">{label}</div>
                {list.map((e) => (
                  <button key={e.path} className="example-item" disabled={opening !== null} onClick={() => void open(e)} title={e.path}>
                    <Icon name={opening === e.path ? "refresh" : "file-code"} size={14} />
                    <span className="name">{e.name}</span>
                    <span className="lib">{e.library}</span>
                  </button>
                ))}
              </div>
            ) : null,
          )}
        </div>
        <div className="settings-note">{t.examples.hint}</div>
      </div>
    </Dialog>
  );
}
