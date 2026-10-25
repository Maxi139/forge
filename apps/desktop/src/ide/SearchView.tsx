import { Button, Icon, Input, Panel } from "@forge/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "../i18n/de";
import { backend, type SearchHit } from "../lib/backend";
import { baseName, errorText, useStore } from "../lib/store";
import { useReferences } from "../lib/references";
import { replaceEdits, type ReplaceSpec } from "../lib/replace";
import { projectRelative } from "./Explorer";

interface Group {
  file: string;
  hits: SearchHit[];
}

function Row({ hit, onOpen, onReplace }: { hit: SearchHit; onOpen(): void; onReplace?(): void }) {
  const before = hit.text.slice(0, hit.col);
  const match = hit.text.slice(hit.col, hit.endCol);
  const after = hit.text.slice(hit.endCol);
  // Führende Leerzeichen sparen Platz, ohne die Markierung zu verschieben.
  const lead = before.length - before.trimStart().length;
  return (
    <div className="search-row">
      <button type="button" className="search-hit" onClick={onOpen}>
        <span className="search-line">{hit.line}</span>
        <span className="search-text">
          {before.slice(lead)}
          {match ? <mark>{match}</mark> : null}
          {after}
        </span>
      </button>
      {onReplace ? (
        <button type="button" className="search-replace-one" title={t.editorTools.replaceOne} aria-label={t.editorTools.replaceOne} onClick={onReplace}>
          <Icon name="check" size={12} />
        </button>
      ) : null}
    </div>
  );
}

function Groups({ groups, closed, setClosed, onReplace }: { groups: Group[]; closed: Record<string, boolean>; setClosed(f: (c: Record<string, boolean>) => Record<string, boolean>): void; onReplace?(hit: SearchHit): void }) {
  const project = useStore((s) => s.project);
  return (
    <>
      {groups.map((g) => {
        const shut = closed[g.file];
        return (
          <div key={g.file} className="search-group">
            <button type="button" className="search-file" onClick={() => setClosed((c) => ({ ...c, [g.file]: !shut }))} aria-expanded={!shut}>
              <Icon name={shut ? "chevron-right" : "chevron-down"} size={12} />
              <span className="search-name">{baseName(g.file)}</span>
              <span className="search-dir">{project ? projectRelative(project.path, g.file).split("/").slice(0, -1).join("/") : ""}</span>
              <span className="search-n">{g.hits.length}</span>
            </button>
            {shut ? null : g.hits.map((h, i) => <Row key={i} hit={h} onOpen={() => void useStore.getState().gotoLocation({ file: h.file, line: h.line, col: h.col })} onReplace={onReplace ? () => onReplace(h) : undefined} />)}
          </div>
        );
      })}
    </>
  );
}

function groupHits(hits: SearchHit[]): Group[] {
  const by = new Map<string, SearchHit[]>();
  for (const h of hits) by.set(h.file, [...(by.get(h.file) ?? []), h]);
  return [...by].map(([file, list]) => ({ file, hits: list }));
}

/** Text einer Datei: der Stand im Editor, sonst von der Platte. */
async function currentText(file: string): Promise<string> {
  return useStore.getState().buffers[file]?.text ?? (await backend().readFile(file));
}

export function SearchView() {
  const project = useStore((s) => s.project);
  const [query, setQuery] = useState("");
  const [caseSensitive, setCase] = useState(false);
  const [wholeWord, setWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [groups, setGroups] = useState<Group[] | null>(null);
  const [total, setTotal] = useState(0);
  const [truncated, setTruncated] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [closed, setClosed] = useState<Record<string, boolean>>({});
  const [replacing, setReplacing] = useState(false);
  const [replacement, setReplacement] = useState("");
  const [tick, setTick] = useState(0);
  const references = useReferences((s) => s.result);
  const input = useRef<HTMLInputElement>(null);

  // ⇧⌘F setzt den Cursor ins Suchfeld.
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
    const on = () => {
      input.current?.focus();
      input.current?.select();
    };
    window.addEventListener("forge:focus-search", on);
    return () => window.removeEventListener("forge:focus-search", on);
  }, []);

  useEffect(() => {
    const q = query;
    if (!q || !project) {
      setGroups(null);
      setError(null);
      return;
    }
    let alive = true;
    const id = setTimeout(() => {
      const overrides: Record<string, string> = {};
      for (const [path, b] of Object.entries(useStore.getState().buffers)) if (!b.readOnly && b.text !== b.saved) overrides[path] = b.text;
      backend()
        .search(q, { caseSensitive, wholeWord, regex }, overrides)
        .then((r) => {
          if (!alive) return;
          setGroups(groupHits(r.hits));
          setTotal(r.hits.length);
          setTruncated(r.truncated);
          setError(null);
        })
        .catch((e) => alive && (setError(errorText(e)), setGroups(null)));
    }, 200);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [query, caseSensitive, wholeWord, regex, project, tick]);

  const summary = useMemo(() => (groups ? t.search.count(total, groups.length) : ""), [groups, total]);

  const spec = (): ReplaceSpec => ({ query, replacement, caseSensitive, wholeWord, regex });

  /** Wendet die Ersetzung als Editor-Änderung an: die Dateien werden geöffnet, nichts landet ungefragt auf der Platte. */
  const apply = async (files: string[], only?: SearchHit) => {
    const st = useStore.getState();
    let changed = 0;
    let touched = 0;
    try {
      for (const file of files) {
        const edits = replaceEdits(await currentText(file), spec(), only);
        if (!edits.length) continue;
        await st.applyEdits(file, edits);
        await new Promise((r) => setTimeout(r, 150));
        changed += edits.length;
        touched++;
      }
      if (changed) st.toast({ kind: "success", title: t.editorTools.replacedToast(changed, touched), message: t.editorTools.renamedHint, ttl: 5000 });
    } catch (e) {
      st.toast({ kind: "error", title: t.editorTools.replaceFailed, message: errorText(e) });
    }
    setTick((n) => n + 1);
  };

  const replaceAll = async () => {
    if (!groups?.length) return;
    const ok = await useStore.getState().confirm({ title: t.editorTools.replaceConfirmTitle(total), text: t.editorTools.replaceConfirmText, confirm: t.editorTools.replaceAll });
    if (ok) await apply(groups.map((g) => g.file));
  };

  const opt = (label: string, text: string, on: boolean, set: (v: boolean) => void) => (
    <button type="button" className="search-opt" aria-pressed={on} aria-label={label} title={label} onClick={() => set(!on)}>
      {text}
    </button>
  );

  return (
    <Panel title={t.sidebar.search}>
      <div className="search">
        <div className="search-bar">
          <Input ref={input} icon="search" size="sm" placeholder={t.search.placeholder} aria-label={t.search.placeholder} value={query} onChange={(e) => setQuery(e.target.value)} spellCheck={false} />
          <div className="search-opts">
            {opt(t.search.matchCase, "Aa", caseSensitive, setCase)}
            {opt(t.search.wholeWord, "ab", wholeWord, setWord)}
            {opt(t.search.regex, ".*", regex, setRegex)}
            <span style={{ flex: 1 }} />
            {opt(t.editorTools.replaceToggle, "⇄", replacing, setReplacing)}
          </div>
          {replacing ? (
            <>
              <Input icon="chevron-right" size="sm" placeholder={t.editorTools.replacePlaceholder} aria-label={t.editorTools.replacePlaceholder} value={replacement} onChange={(e) => setReplacement(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void replaceAll()} spellCheck={false} />
              <Button size="sm" variant="secondary" disabled={!groups?.length} onClick={() => void replaceAll()}>
                {t.editorTools.replaceAll}
              </Button>
            </>
          ) : null}
        </div>
        <div className="search-scroll">
          {references ? (
            <div className="search-refs">
              <div className="search-refs-head">
                <span>{t.editorTools.refsTitle(references.name)}</span>
                <button type="button" className="search-refs-close" onClick={() => useReferences.getState().set(null)}>
                  {t.editorTools.refsClose}
                </button>
              </div>
              {references.hits.length === 0 ? <div className="sb-empty">{t.editorTools.refsNone}</div> : <Groups groups={groupHits(references.hits)} closed={closed} setClosed={setClosed} />}
            </div>
          ) : null}
          {error ? <div className="sb-empty">{error}</div> : null}
          {groups && groups.length === 0 ? <div className="sb-empty">{t.search.none(query)}</div> : null}
          {groups && groups.length > 0 ? <div className="search-summary">{summary}</div> : null}
          {groups ? <Groups groups={groups} closed={closed} setClosed={setClosed} onReplace={replacing ? (h) => void apply([h.file], h) : undefined} /> : null}
          {truncated ? <div className="sb-empty">{t.search.truncated}</div> : null}
        </div>
      </div>
    </Panel>
  );
}
