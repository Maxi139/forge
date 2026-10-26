import { Button, Checkbox, Dialog, formatBytes, Input, Select } from "@forge/ui";
import { useEffect, useMemo, useState } from "react";
import { t } from "../i18n/de";
import { baseName, errorText, useStore } from "../lib/store";
import { cppIdentifier, findBlocks, isHtml, placeBlock, suggestName, usageHint, type Generated } from "../lib/web/embed";
import { CODE_FILE, flattenTree, readProjectText, renderBlock, toAbs, toRel, writeCode } from "../lib/web/sync";

/** Schneidet lange Zeilen für die Anzeige. Verkleinerte Seiten bestehen aus einer einzigen Zeile. */
function excerpt(code: string): string {
  const lines = code.split("\n").map((l) => (l.length > 150 ? `${l.slice(0, 150)} …` : l));
  return lines.length > 16 ? [...lines.slice(0, 15), "…", lines[lines.length - 1]].join("\n") : lines.join("\n");
}

/** Macht aus einer Webseite den C++-Text für die .ino und setzt ihn dort ein. */
export function WebEmbedDialog({ source: wantedSource, target: wantedTarget }: { source?: string; target?: string }) {
  const setDialog = useStore((s) => s.setDialog);
  const project = useStore((s) => s.project);
  const tree = useStore((s) => s.tree);
  const active = useStore((s) => s.active);

  const files = useMemo(() => flattenTree(tree).map(toRel), [tree]);
  const sources = useMemo(() => files.filter((f) => isHtml(f)), [files]);
  const targets = useMemo(() => files.filter((f) => CODE_FILE.test(f)), [files]);

  const [source, setSource] = useState(() => {
    if (wantedSource && sources.includes(wantedSource)) return wantedSource;
    if (active && isHtml(active)) return toRel(active);
    return sources.find((s) => baseName(s) === "index.html") ?? sources[0] ?? "";
  });
  const [target, setTarget] = useState(() => {
    if (wantedTarget && targets.includes(wantedTarget)) return wantedTarget;
    if (active && CODE_FILE.test(active)) return toRel(active);
    return (project && targets.includes(toRel(project.mainFile)) ? toRel(project.mainFile) : targets.find((f) => f.endsWith(".ino"))) ?? targets[0] ?? "";
  });
  const [name, setName] = useState("");
  const [touched, setTouched] = useState(false);
  const [minify, setMinify] = useState(false);
  const [generated, setGenerated] = useState<(Generated & { missing: string[] }) | null>(null);
  const [existing, setExisting] = useState<{ name: string; minify: boolean } | null>(null);
  const [busy, setBusy] = useState(false);

  // Gibt es für diese Seite schon einen Block in der Zieldatei, übernehmen wir seinen Namen und seine Einstellung.
  useEffect(() => {
    let alive = true;
    void (async () => {
      const text = target ? await readProjectText(toAbs(target)) : null;
      const block = text ? findBlocks(text).find((b) => b.source === source) : undefined;
      if (!alive) return;
      setExisting(block ? { name: block.name, minify: block.minify } : null);
      if (block) {
        setName(block.name);
        setMinify(block.minify);
        setTouched(true);
      }
    })();
    return () => {
      alive = false;
    };
  }, [source, target]);

  const valid = name !== "" && cppIdentifier(name) === name;
  const spec = { source, name: valid ? name : cppIdentifier(name || "SEITE"), minify };

  useEffect(() => {
    let alive = true;
    if (!source) {
      setGenerated(null);
      return;
    }
    void renderBlock(spec).then((g) => alive && setGenerated(g));
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source, name, minify, valid]);

  // Ohne eigenen Namen schlagen wir einen vor: eine Konstante, oder mit Platzhaltern eine Funktion.
  useEffect(() => {
    if (touched || !source) return;
    void renderBlock({ source, name: "X", minify: false }).then((g) => {
      if (g) setName(suggestName(source, g.params.length > 0));
    });
  }, [source, touched]);

  const insert = async () => {
    if (!generated || !valid || !target) return;
    setBusy(true);
    try {
      await writeCode(toAbs(target), (text) => placeBlock(text, spec, generated.code).text);
      useStore.getState().toast({ kind: "success", title: t.web.dialog.done(baseName(target)), ttl: 3000 });
      setDialog(null);
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    if (!generated) return;
    try {
      await navigator.clipboard.writeText(generated.code);
      useStore.getState().toast({ kind: "success", title: t.web.dialog.copied, ttl: 2000 });
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  };

  const problem = !source ? t.web.dialog.noSource : !target ? t.web.dialog.noTarget : null;
  const dynamicCount = generated?.params.length ?? 0;

  return (
    <Dialog
      title={t.web.dialog.title}
      icon="file-code"
      tone="glut"
      width={680}
      onClose={() => setDialog(null)}
      footer={
        <>
          <Button variant="ghost" onClick={() => setDialog(null)}>
            {t.dialogs.cancel}
          </Button>
          <span className="fg-spacer" />
          <Button disabled={!generated} onClick={() => void copy()}>
            {t.web.dialog.copy}
          </Button>
          <Button variant="primary" icon="check" busy={busy} disabled={!generated || !valid || !target || busy} onClick={() => void insert()}>
            {existing ? t.web.dialog.update : t.web.dialog.insert}
          </Button>
        </>
      }
    >
      <p>{t.web.dialog.intro}</p>
      {problem ? (
        <p className="ai-task-note is-error" role="alert">
          {problem}
        </p>
      ) : (
        <div className="web-dialog">
          <div className="web-dialog-row">
            <Select label={t.web.dialog.source} value={source} options={sources} onChange={(e) => setSource(e.target.value)} />
            <Select label={t.web.dialog.target} value={target} options={targets} onChange={(e) => setTarget(e.target.value)} />
          </div>
          <div className="web-dialog-row">
            <Input
              label={t.web.dialog.name}
              mono
              value={name}
              error={name !== "" && !valid ? t.web.dialog.nameInvalid : undefined}
              onChange={(e) => {
                setName(e.target.value);
                setTouched(true);
              }}
            />
            <div className="web-dialog-minify">
              <Checkbox label={t.web.dialog.minify} checked={minify} onChange={(e) => setMinify(e.target.checked)} />
              <span className="fg-field-hint">{t.web.dialog.minifyHint}</span>
            </div>
          </div>

          <div className="fg-field-label">{t.web.dialog.preview}</div>
          <pre className="web-code" tabIndex={0}>
            {generated ? excerpt(generated.code) : "…"}
          </pre>
          <p className="web-dialog-meta">
            {generated ? t.web.size(formatBytes(generated.bytes)) : null}
            {generated && target ? ` · ${existing ? t.web.dialog.replaces(baseName(target)) : t.web.dialog.adds(baseName(target))}` : null}
          </p>
          {dynamicCount > 0 ? <p className="web-dialog-meta">{t.web.dialog.dynamic(dynamicCount)}</p> : null}
          {generated?.missing.length ? (
            <p className="ai-task-note is-error" role="status">
              {t.web.dialog.missing(generated.missing.join(", "))}
            </p>
          ) : null}
          {generated ? (
            <>
              <div className="fg-field-label">{t.web.dialog.usage}</div>
              <pre className="web-code web-code--hint">{usageHint(name || spec.name, generated.params)}</pre>
            </>
          ) : null}
        </div>
      )}
    </Dialog>
  );
}
