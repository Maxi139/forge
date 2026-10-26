import { Badge, Button, formatBytes, Icon, Input, Select } from "@forge/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { t } from "../i18n/de";
import { backend } from "../lib/backend";
import { baseName, useStore } from "../lib/store";
import { fillPlaceholders, isHtml, placeholders, references, utf8Length, webKind } from "../lib/web/embed";
import { createWebStarter } from "../lib/web/starter";
import { flattenTree, renderPage, toRel } from "../lib/web/sync";

const SAMPLES_KEY = "forge.web.samples";

function readSamples(): Record<string, string> {
  try {
    const v = JSON.parse(localStorage.getItem(SAMPLES_KEY) ?? "{}");
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

/**
 * Meldet Fehler und Konsolenausgaben der Seite an die Oberfläche. Das Vorschaufenster hat keinen eigenen Ursprung,
 * deshalb geht das nur über Nachrichten.
 */
const PROBE = `<script>(function(){function s(k,a){try{parent.postMessage({forge:1,k:k,m:Array.prototype.map.call(a,function(x){try{return typeof x==="string"?x:JSON.stringify(x)}catch(e){return String(x)}}).join(" ")},"*")}catch(e){}}["log","info","warn","error"].forEach(function(k){var o=console[k];console[k]=function(){s(k,arguments);o.apply(console,arguments)}});window.addEventListener("error",function(e){s("error",[e.message+(e.lineno?" (Zeile "+e.lineno+")":"")])});window.addEventListener("unhandledrejection",function(e){s("error",["Unbehandelte Zusage: "+(e.reason&&e.reason.message||e.reason)])})})();</script>`;

export function withProbe(html: string): string {
  const head = /<head\b[^>]*>/i.exec(html) ?? /<html\b[^>]*>/i.exec(html) ?? /<!doctype[^>]*>/i.exec(html);
  if (!head) return PROBE + html;
  const at = head.index + head[0].length;
  return html.slice(0, at) + PROBE + html.slice(at);
}

interface Line {
  kind: "log" | "info" | "warn" | "error";
  text: string;
}

/** Die HTML-Datei, die zum geöffneten Tab passt: die Seite selbst, oder die Seite, die dieses Stylesheet oder Skript einbindet. */
export function pageFor(active: string | null, pages: string[], buffers: Record<string, { text: string }>, chosen: string | null): string | null {
  if (active && isHtml(active)) return active;
  if (chosen && pages.includes(chosen)) return chosen;
  if (active && webKind(active)) {
    const rel = toRel(active);
    const using = pages.find((p) => {
      const text = buffers[p]?.text;
      return text != null && references(text, toRel(p)).some((r) => r.path === rel);
    });
    if (using) return using;
  }
  return pages.find((p) => baseName(p) === "index.html") ?? pages[0] ?? null;
}

export function WebPreview() {
  const active = useStore((s) => s.active);
  const tree = useStore((s) => s.tree);
  const buffers = useStore((s) => s.buffers);
  const patchLayout = useStore((s) => s.patchLayout);
  const [chosen, setChosen] = useState<string | null>(null);
  const [wide, setWide] = useState(false);
  const [samples, setSamples] = useState(readSamples);
  const [source, setSource] = useState<{ src?: string; srcdoc?: string; n: number } | null>(null);
  const [info, setInfo] = useState<{ bytes: number; missing: string[]; names: string[] } | null>(null);
  const [lines, setLines] = useState<Line[]>([]);
  const [consoleOpen, setConsoleOpen] = useState(false);
  const [reload, setReload] = useState(0);
  const frame = useRef<HTMLIFrameElement>(null);
  const counter = useRef(0);

  const pages = useMemo(() => flattenTree(tree).filter(isHtml), [tree]);
  const page = pageFor(active, pages, buffers, chosen);

  // Ändert sich, sobald eine Webdatei sich ändert.
  const signature = useStore((s) =>
    Object.entries(s.buffers)
      .filter(([p]) => webKind(p))
      .map(([p, b]) => `${p}:${b.version}:${b.revision}`)
      .join("|"),
  );

  useEffect(() => {
    if (!page) {
      setSource(null);
      setInfo(null);
      return;
    }
    let alive = true;
    const id = setTimeout(async () => {
      const rendered = await renderPage(toRel(page), false);
      if (!alive || !rendered) return;
      const names = placeholders(rendered.page);
      setInfo({ bytes: utf8Length(rendered.page), missing: rendered.missing, names });
      const doc = withProbe(fillPlaceholders(rendered.page, samples));
      try {
        const res = await backend().webPreview(doc);
        if (!alive) return;
        setLines([]);
        setSource({ ...res, n: ++counter.current });
      } catch {
        /* Ohne Vorschau bleibt der letzte Stand stehen. */
      }
    }, 300);
    return () => {
      alive = false;
      clearTimeout(id);
    };
  }, [page, signature, samples, reload]);

  useEffect(() => {
    const on = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      const d = e.data as { forge?: number; k?: Line["kind"]; m?: unknown } | null;
      if (!d || d.forge !== 1 || typeof d.m !== "string") return;
      setLines((l) => [...l.slice(-99), { kind: d.k ?? "log", text: d.m as string }]);
    };
    window.addEventListener("message", on);
    return () => window.removeEventListener("message", on);
  }, []);

  const setSample = (name: string, value: string) => {
    const next = { ...samples, [name]: value };
    setSamples(next);
    try {
      localStorage.setItem(SAMPLES_KEY, JSON.stringify(next));
    } catch {
      /* Beispielwerte gelten dann nur für diese Sitzung. */
    }
  };

  const errors = lines.filter((l) => l.kind === "error").length;

  return (
    <aside className="web-preview" aria-label={t.web.preview}>
      <div className="web-preview-head">
        <Icon name="play" size={14} />
        <strong>{t.web.preview}</strong>
        {pages.length > 1 && page ? (
          <Select
            size="sm"
            aria-label={t.web.page}
            value={page}
            options={pages.map((p) => ({ value: p, label: toRel(p) }))}
            onChange={(e) => setChosen(e.target.value)}
          />
        ) : page ? (
          <span className="web-preview-name">{toRel(page)}</span>
        ) : null}
        <span style={{ flex: 1 }} />
        <Button variant="ghost" size="sm" onClick={() => setWide((w) => !w)} title={wide ? t.web.phone : t.web.full} aria-pressed={wide}>
          {wide ? t.web.full : t.web.phone}
        </Button>
        <Button variant="ghost" size="sm" icon="refresh" aria-label={t.web.reload} title={t.web.reload} onClick={() => setReload((n) => n + 1)} />
        <Button variant="ghost" size="sm" icon="close" aria-label={t.web.previewClose} title={t.web.previewClose} onClick={() => patchLayout({ previewOpen: false })} />
      </div>

      {!page ? (
        <div className="web-preview-empty">
          <p>{t.web.noPage}</p>
          <Button variant="primary" size="sm" icon="plus" onClick={() => void createWebStarter()}>
            {t.web.noPageAction}
          </Button>
        </div>
      ) : (
        <>
          {info?.names.length ? (
            <div className="web-samples" role="group" aria-label={t.web.samples}>
              {info.names.map((n) => (
                <Input key={n} size="sm" mono placeholder={n} aria-label={t.web.sample(n)} title={t.web.sample(n)} value={samples[n] ?? ""} onChange={(e) => setSample(n, e.target.value)} />
              ))}
            </div>
          ) : null}
          <div className={`web-frame${wide ? " is-wide" : ""}`}>
            {source ? (
              <iframe
                key={source.n}
                ref={frame}
                title={t.web.previewTitle}
                sandbox="allow-scripts allow-forms allow-modals"
                src={source.src}
                srcDoc={source.srcdoc}
              />
            ) : null}
          </div>
          {info?.missing.length ? <div className="web-preview-warn">{t.web.missingFiles(info.missing.join(", "))}</div> : null}
          <div className="web-console">
            <button type="button" className="web-console-head" aria-expanded={consoleOpen} onClick={() => setConsoleOpen((o) => !o)}>
              <Icon name={consoleOpen ? "chevron-down" : "chevron-right"} size={12} />
              <span>{t.web.console}</span>
              {errors ? <Badge tone="error">{errors}</Badge> : lines.length ? <Badge>{lines.length}</Badge> : null}
              <span style={{ flex: 1 }} />
              {info ? <span className="web-size">{t.web.size(formatBytes(info.bytes))}</span> : null}
            </button>
            {consoleOpen ? (
              <div className="web-console-body">
                {lines.length ? (
                  lines.map((l, i) => (
                    <div key={i} className={`web-line is-${l.kind}`}>
                      {l.text}
                    </div>
                  ))
                ) : (
                  <div className="web-line is-empty">{t.web.consoleEmpty}</div>
                )}
                <p className="web-note">{t.web.deviceNote}</p>
              </div>
            ) : null}
          </div>
        </>
      )}
    </aside>
  );
}
