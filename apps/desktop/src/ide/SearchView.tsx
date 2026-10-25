import { Icon, Input, Panel } from "@forge/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "../i18n/de";
import { backend, type SearchHit } from "../lib/backend";
import { baseName, errorText, useStore } from "../lib/store";
import { projectRelative } from "./Explorer";

interface Group {
  file: string;
  hits: SearchHit[];
}

function Row({ hit, onOpen }: { hit: SearchHit; onOpen(): void }) {
  const before = hit.text.slice(0, hit.col);
  const match = hit.text.slice(hit.col, hit.endCol);
  const after = hit.text.slice(hit.endCol);
  // Führende Leerzeichen sparen Platz, ohne die Markierung zu verschieben.
  const lead = before.length - before.trimStart().length;
  return (
    <button type="button" className="search-hit" onClick={onOpen}>
      <span className="search-line">{hit.line}</span>
      <span className="search-text">
        {before.slice(lead)}
        {match ? <mark>{match}</mark> : null}
        {after}
      </span>
    </button>
  );
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
          const by = new Map<string, SearchHit[]>();
          for (const h of r.hits) by.set(h.file, [...(by.get(h.file) ?? []), h]);
          setGroups([...by].map(([file, hits]) => ({ file, hits })));
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
  }, [query, caseSensitive, wholeWord, regex, project]);

  const summary = useMemo(() => (groups ? t.search.count(total, groups.length) : ""), [groups, total]);

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
          </div>
        </div>
        <div className="search-scroll">
          {error ? <div className="sb-empty">{error}</div> : null}
          {groups && groups.length === 0 ? <div className="sb-empty">{t.search.none(query)}</div> : null}
          {groups && groups.length > 0 ? <div className="search-summary">{summary}</div> : null}
          {groups?.map((g) => {
            const shut = closed[g.file];
            return (
              <div key={g.file} className="search-group">
                <button type="button" className="search-file" onClick={() => setClosed((c) => ({ ...c, [g.file]: !shut }))} aria-expanded={!shut}>
                  <Icon name={shut ? "chevron-right" : "chevron-down"} size={12} />
                  <span className="search-name">{baseName(g.file)}</span>
                  <span className="search-dir">{project ? projectRelative(project.path, g.file).split("/").slice(0, -1).join("/") : ""}</span>
                  <span className="search-n">{g.hits.length}</span>
                </button>
                {shut ? null : g.hits.map((h, i) => <Row key={i} hit={h} onOpen={() => void useStore.getState().gotoLocation({ file: h.file, line: h.line, col: h.col })} />)}
              </div>
            );
          })}
          {truncated ? <div className="sb-empty">{t.search.truncated}</div> : null}
        </div>
      </div>
    </Panel>
  );
}
