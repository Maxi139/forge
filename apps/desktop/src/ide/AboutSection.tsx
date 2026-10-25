import { Button, Checkbox, Input } from "@forge/ui";
import { useState } from "react";
import { t } from "../i18n/de";
import { useAppUpdate } from "../lib/appUpdate";
import { openExternal, PROJECT_URL } from "../lib/links";
import { reportError } from "../lib/log";

interface Pkg {
  name: string;
  version: string;
  license: string;
  repository?: string;
  texts: string[];
}
interface Licenses {
  fonts: Pkg[];
  rust: Pkg[];
  npm: Pkg[];
  texts: Record<string, string>;
}

const GROUPS = ["fonts", "rust", "npm"] as const;

function LicenseList({ data }: { data: Licenses }) {
  const [filter, setFilter] = useState("");
  const [open, setOpen] = useState<string | null>(null);
  const q = filter.trim().toLowerCase();
  const groups = GROUPS.map((g) => ({ id: g, items: data[g].filter((p) => !q || p.name.toLowerCase().includes(q)) })).filter((g) => g.items.length);
  return (
    <>
      <Input label={t.about.filter} value={filter} onChange={(e) => setFilter(e.target.value)} spellCheck={false} />
      <div className="license-list">
        {groups.length === 0 ? <div className="settings-note">{t.about.none}</div> : null}
        {groups.map((g) => (
          <div key={g.id}>
            <div className="license-group">{t.about.groups[g.id]}</div>
            {g.items.map((p) => {
              const key = `${g.id}:${p.name}@${p.version}`;
              const expanded = open === key;
              return (
                <div key={key}>
                  <button type="button" className="license-row" aria-expanded={expanded} onClick={() => setOpen(expanded ? null : key)}>
                    <span>{`${p.name} ${p.version}`}</span>
                    <span className="license-id">{p.license}</span>
                  </button>
                  {expanded ? (
                    <pre className="license-text">
                      {p.texts.length ? p.texts.map((id) => data.texts[id]).join("\n\n———\n\n") : `${t.about.noText} ${p.license}`}
                    </pre>
                  ) : null}
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </>
  );
}

export function AboutSection() {
  const [shown, setShown] = useState(false);
  const [data, setData] = useState<Licenses | null>(null);
  const autoCheck = useAppUpdate((s) => s.autoCheck);
  const busy = useAppUpdate((s) => s.busy);

  const toggle = () => {
    setShown((v) => !v);
    // Die Lizenzdaten sind groß und kommen erst auf Wunsch.
    if (!data) {
      import("../generated/licenses.json")
        .then((m) => setData(m.default as Licenses))
        .catch((e) => reportError("Lizenzen lassen sich nicht laden", e));
    }
  };

  return (
    <section className="settings-group" aria-label={t.about.section}>
      <h3 className="settings-heading">{t.about.section}</h3>
      <div className="form">
        <div className="about-version">{t.about.version(__APP_VERSION__)}</div>
        <div className="settings-note">{t.about.copyright}</div>
        <div className="about-links">
          <Button variant="secondary" size="sm" onClick={() => void openExternal(PROJECT_URL).catch((e) => reportError("Projektseite", e))}>
            {t.about.project}
          </Button>
          <Button variant="ghost" size="sm" onClick={toggle}>
            {shown ? t.about.hide : t.about.show}
          </Button>
        </div>
        <div className="about-links">
          <Button variant="secondary" size="sm" busy={busy} onClick={() => void useAppUpdate.getState().check()}>
            {busy ? t.appUpdate.checking : t.appUpdate.checkNow}
          </Button>
        </div>
        <Checkbox label={t.appUpdate.auto} checked={autoCheck} onChange={(e) => useAppUpdate.getState().setAutoCheck(e.target.checked)} />
        {shown ? (
          <>
            <div className="settings-note">{t.about.licensesText}</div>
            {data ? <LicenseList data={data} /> : <div className="settings-note">{t.about.loading}</div>}
          </>
        ) : null}
      </div>
    </section>
  );
}
