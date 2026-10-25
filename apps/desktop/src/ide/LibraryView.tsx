import { Button, Input, LibraryItem, Panel } from "@forge/ui";
import { useEffect, useMemo, useState } from "react";
import { t } from "../i18n/de";
import { backend, type LibrarySearchHit } from "../lib/backend";
import { useHardware } from "../lib/hardware";
import { errorText } from "../lib/store";
import "../hardware.css";

export function LibraryView() {
  const installed = useHardware((s) => s.libraries);
  const busy = useHardware((s) => s.libraryBusy);
  const refresh = useHardware((s) => s.refreshLibraries);
  const install = useHardware((s) => s.installLibrary);
  const uninstall = useHardware((s) => s.uninstallLibrary);
  const upgrade = useHardware((s) => s.upgradeLibrary);
  const outdated = useHardware((s) => s.outdated);
  const refreshOutdated = useHardware((s) => s.refreshOutdated);
  const upgradeAll = useHardware((s) => s.upgradeAllLibraries);
  const installZip = useHardware((s) => s.installLibraryZip);
  const query = useHardware((s) => s.libraryQuery);
  const setQuery = (q: string) => useHardware.setState({ libraryQuery: q });
  const [hits, setHits] = useState<LibrarySearchHit[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void refresh();
    void refreshOutdated();
  }, [refresh, refreshOutdated]);

  // Suche mit kurzer Pause nach dem Tippen. Spätere Eingaben machen frühere Antworten ungültig.
  useEffect(() => {
    const q = query.trim();
    if (q.length < 2) {
      setHits(null);
      setError(null);
      return;
    }
    let alive = true;
    const id = setTimeout(() => {
      backend()
        .searchLibraries(q)
        .then((r) => alive && (setHits(r), setError(null)))
        .catch((e) => alive && setError(errorText(e)));
    }, 300);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [query]);

  const installedByName = useMemo(() => new Map(installed.map((l) => [l.name, l])), [installed]);
  const rows = hits ?? null;

  return (
    <Panel title={t.libraries.title}>
      <div className="libs">
        <div className="libs-search">
          <Input icon="search" size="sm" placeholder={t.libraries.search} aria-label={t.libraries.search} value={query} onChange={(e) => setQuery(e.target.value)} />
        </div>
        <div className="libs-actions">
          {outdated.length ? (
            <Button size="sm" variant="primary" icon="refresh" disabled={Object.keys(busy).length > 0} onClick={() => void upgradeAll()}>
              {t.updates.upgradeAll} ({outdated.length})
            </Button>
          ) : null}
          <Button size="sm" variant="ghost" icon="plus" disabled={Object.keys(busy).length > 0} onClick={() => void installZip()} title={t.updates.zipHint}>
            {t.updates.installZip}
          </Button>
        </div>
        <div className="libs-scroll">
          {error ? <div className="sb-empty">{error}</div> : null}
          {rows ? (
            <>
              <div className="libs-heading">{t.libraries.results}</div>
              {rows.length === 0 ? <div className="sb-empty">{t.libraries.noResults(query.trim())}</div> : null}
              {rows.map((h) => {
                const local = installedByName.get(h.name);
                const isBusy = h.name in busy;
                return (
                  <LibraryItem
                    key={h.name}
                    compact
                    name={h.name}
                    version={local?.version ?? h.latest}
                    latest={local?.latest ?? undefined}
                    author={h.author}
                    description={h.sentence}
                    state={isBusy ? "installing" : local ? (local.latest ? "update" : "installed") : "available"}
                    progress={busy[h.name]}
                    onInstall={() => void install(h.name)}
                    onUpdate={() => void upgrade(h.name)}
                    onRemove={() => void uninstall(h.name)}
                  />
                );
              })}
            </>
          ) : (
            <>
              <div className="libs-heading">{t.libraries.installed}</div>
              {installed.length === 0 ? <div className="sb-empty">{t.libraries.none}</div> : null}
              {installed.map((l) => (
                <LibraryItem
                  key={l.name}
                  compact
                  name={l.name}
                  version={l.version}
                  latest={l.latest ?? undefined}
                  author={l.author}
                  description={l.sentence}
                  state={l.name in busy ? "installing" : l.latest ? "update" : "installed"}
                  progress={busy[l.name]}
                  onUpdate={() => void upgrade(l.name)}
                  onRemove={() => void uninstall(l.name)}
                />
              ))}
            </>
          )}
        </div>
      </div>
    </Panel>
  );
}
