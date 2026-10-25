import { Button, Dialog, Input, Select } from "@forge/ui";
import { useEffect, useMemo, useState } from "react";
import { t } from "../i18n/de";
import { useHardware } from "../lib/hardware";
import { useStore } from "../lib/store";

/** Board und Menüoptionen (Flash-Größe, Partitionsschema, PSRAM, USB-Modus ...) wählen. */
export function BoardPicker() {
  const project = useStore((s) => s.project)!;
  const setDialog = useStore((s) => s.setDialog);
  const boards = useHardware((s) => s.boards);
  const loadBoards = useHardware((s) => s.loadBoards);
  const loadOptions = useHardware((s) => s.loadOptions);
  const setBoard = useHardware((s) => s.setBoard);

  const [query, setQuery] = useState("");
  const [fqbn, setFqbn] = useState(project.config.board.fqbn);
  const [chosen, setChosen] = useState<Record<string, string>>(project.config.board.options);
  const options = useHardware((s) => s.boardOptions[fqbn]);

  useEffect(() => {
    if (!boards.length) void loadBoards();
  }, [boards.length, loadBoards]);
  useEffect(() => {
    void loadOptions(fqbn);
  }, [fqbn, loadOptions]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return boards.filter((b) => !q || b.name.toLowerCase().includes(q) || b.fqbn.toLowerCase().includes(q));
  }, [boards, query]);

  const pick = (next: string) => {
    if (next === fqbn) return;
    setFqbn(next);
    setChosen({});
  };

  const apply = async () => {
    // Nur Abweichungen vom Standard speichern.
    const defaults = Object.fromEntries((options ?? []).map((o) => [o.id, o.default]));
    const diff = Object.fromEntries(Object.entries(chosen).filter(([k, v]) => v && v !== defaults[k] && (options ?? []).some((o) => o.id === k)));
    await setBoard(fqbn, diff);
    setDialog(null);
  };

  return (
    <Dialog
      title={t.boardPicker.title}
      icon="chip"
      tone="glut"
      width={780}
      onClose={() => setDialog(null)}
      footer={
        <>
          <Button onClick={() => setDialog(null)}>{t.boardPicker.cancel}</Button>
          <Button variant="primary" onClick={() => void apply()} disabled={!fqbn}>
            {t.boardPicker.apply}
          </Button>
        </>
      }
    >
      <div className="picker">
        <div className="picker-list">
          <Input icon="search" placeholder={t.boardPicker.search} aria-label={t.boardPicker.search} value={query} onChange={(e) => setQuery(e.target.value)} autoFocus size="sm" />
          <div className="picker-rows" role="listbox" aria-label={t.boardPicker.title}>
            {filtered.length === 0 ? <div className="picker-empty">{boards.length ? t.boardPicker.empty : t.boardPicker.installCore}</div> : null}
            {filtered.slice(0, 250).map((b) => (
              <button key={b.fqbn} role="option" aria-selected={b.fqbn === fqbn} className={`picker-row${b.fqbn === fqbn ? " is-active" : ""}`} onClick={() => pick(b.fqbn)} onDoubleClick={() => void apply()}>
                <span className="name">{b.name}</span>
                <span className="fq">{b.fqbn}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="picker-options">
          <h4>{t.boardPicker.options}</h4>
          {!options ? (
            <div className="picker-empty">Lade …</div>
          ) : options.length === 0 ? (
            <div className="picker-empty">Dieses Board hat keine Optionen.</div>
          ) : (
            options.map((o) => (
              <Select
                key={`${fqbn}:${o.id}`}
                label={o.label}
                value={chosen[o.id] ?? o.default}
                onChange={(e) => setChosen((c) => ({ ...c, [o.id]: e.target.value }))}
                options={o.values.map((v) => ({ value: v.value, label: v.label }))}
                size="sm"
              />
            ))
          )}
        </div>
      </div>
    </Dialog>
  );
}
