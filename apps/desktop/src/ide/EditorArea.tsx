import { Badge, Button, EmptyState, Icon, Splitter, Tabs, type TabItem } from "@forge/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { applyEdits } from "../editor/diagnostics";
import { currentSymbol, useSymbols } from "../editor/symbols";
import { ForgeEditor, type CursorInfo, type ForgeEditorHandle, type Reveal } from "../editor/ForgeEditor";
import { t } from "../i18n/de";
import { useAi } from "../lib/ai";
import { baseName, diagnosticsForFile, useStore } from "../lib/store";
import { isHtml, webKind } from "../lib/web/embed";
import { CODE_FILE, flattenTree, toRel } from "../lib/web/sync";
import { projectRelative } from "./Explorer";
import { Welcome } from "./Welcome";
import { WebPreview } from "./WebPreview";

export function EditorArea({ reveal }: { reveal: Reveal | null }) {
  const project = useStore((s) => s.project);
  const tabs = useStore((s) => s.tabs);
  const active = useStore((s) => s.active);
  const buffers = useStore((s) => s.buffers);
  const diagnostics = useStore((s) => s.diagnostics);
  const setActive = useStore((s) => s.setActive);
  const closeTab = useStore((s) => s.closeTab);
  const updateBuffer = useStore((s) => s.updateBuffer);
  const save = useStore((s) => s.save);
  const setDialog = useStore((s) => s.setDialog);
  const setCursor = useStore((s) => s.setCursor);

  const editor = useRef<ForgeEditorHandle>(null);
  const layout = useStore((s) => s.layout);
  const patchLayout = useStore((s) => s.patchLayout);
  const tree = useStore((s) => s.tree);
  const [livePreview, setLivePreview] = useState<number | null>(null);

  // Textänderungen von außen, etwa „In Schlüsselbund legen“.
  useEffect(() => {
    const on = (e: Event) => {
      const { file, edits } = (e as CustomEvent<{ file: string; edits: Parameters<typeof applyEdits>[1] }>).detail;
      const view = editor.current?.view();
      if (view && useStore.getState().active === file) applyEdits(view, edits);
    };
    window.addEventListener("forge:apply-edits", on);
    return () => window.removeEventListener("forge:apply-edits", on);
  }, []);

  const items: TabItem[] = useMemo(
    () =>
      tabs.map((path) => {
        const b = buffers[path];
        const problems = diagnosticsForFile({ diagnostics }, path).some((d) => d.severity === "error");
        return { id: path, label: baseName(path), icon: "file-code" as const, dirty: !!b && b.text !== b.saved, problems, title: project ? projectRelative(project.path, path) : path };
      }),
    [tabs, buffers, diagnostics, project],
  );

  const symbols = useSymbols(active);
  const cursorLine = useStore((s) => s.cursor.line);
  const here = useMemo(() => currentSymbol(symbols, cursorLine), [symbols, cursorLine]);

  const aiBlocks = useAi((s) => (active ? s.blocks[active] : undefined));
  const fileDiagnostics = useMemo(() => (active ? diagnosticsForFile({ diagnostics }, active) : []), [diagnostics, active]);
  // Ein Haken sagt „kein Logikfehler“. Liegt in der Funktion ein Fehler des Compilers, wäre er irreführend.
  const aiOk = useMemo(() => {
    const errors = fileDiagnostics.filter((d) => d.severity === "error" && d.source !== "ai");
    return (aiBlocks ?? [])
      .filter((b) => b.state === "ok" && !errors.some((d) => d.line >= b.startLine && d.line <= b.endLine))
      .map((b) => ({ startLine: b.startLine, endLine: b.endLine }));
  }, [aiBlocks, fileDiagnostics]);

  if (!project) return <Welcome />;

  if (!active || !buffers[active]) {
    return (
      <>
        <div className="ide-tabs-row">
          <Tabs items={[]} />
        </div>
        <EmptyState icon="file-code" title={t.editor.noFile} text={t.editor.noFileText} seed={5} count={14} lit={1} />
      </>
    );
  }

  const buffer = buffers[active];
  const isWeb = !!webKind(active);
  const hasPages = flattenTree(tree).some(isHtml);
  const showPreview = isWeb && layout.previewOpen;
  const crumbs = buffer.readOnly ? active.split("/").slice(-4) : [project.name, ...projectRelative(project.path, active).split("/")];

  return (
    <>
      <div className="ide-tabs-row">
        <Tabs variant="editor" items={items} value={active} onChange={setActive} onClose={(p) => void closeTab(p)} />
      </div>
      <div className="ide-editor">
        <div className="fg-crumbs">
          {crumbs.map((c, i) => (
            <span key={i} style={{ display: "contents" }}>
              {i ? <Icon name="chevron-right" size={12} /> : null}
              <span>{c}</span>
            </span>
          ))}
          {here ? (
            <>
              <Icon name="chevron-right" size={12} />
              <button type="button" className="fg-crumb-symbol" title={t.commands.gotoSymbol} onClick={() => useStore.getState().togglePalette(true, "symbols")}>
                {here.name}
              </button>
            </>
          ) : null}
          <span style={{ flex: 1 }} />
          {isWeb ? (
            <>
              <Button variant="ghost" size="sm" icon="play" aria-pressed={layout.previewOpen} onClick={() => patchLayout({ previewOpen: !layout.previewOpen })} title={t.web.previewTitle}>
                {t.web.preview}
              </Button>
              <Button variant="ghost" size="sm" icon="file-code" onClick={() => setDialog({ kind: "webEmbed", source: isHtml(active) ? toRel(active) : undefined })} title={t.web.embedTitle}>
                {t.web.embed}
              </Button>
            </>
          ) : hasPages && CODE_FILE.test(active) && !buffer.readOnly ? (
            <Button variant="ghost" size="sm" icon="file-code" onClick={() => setDialog({ kind: "webEmbed", target: toRel(active) })} title={t.web.embedTitle}>
              {t.web.embed}
            </Button>
          ) : null}
          {!buffer.readOnly ? (
            <Button variant="ghost" size="sm" icon="spark" onClick={() => setDialog({ kind: "aiTask" })} title={t.ai.taskTitle}>
              {t.ai.taskShort}
            </Button>
          ) : null}
          {buffer.readOnly ? (
            <Badge icon="info">{t.editor.readOnly}</Badge>
          ) : (
            <Button variant="ghost" size="sm" icon="check" disabled={buffer.text === buffer.saved} onClick={() => void save()} title="Speichern  ⌘S">
              {t.dialogs.save}
            </Button>
          )}
        </div>
        <div className="ide-editor-body">
        <div className="ide-editor-main">
        <ForgeEditor
          ref={editor}
          docKey={active}
          text={buffer.text}
          revision={buffer.revision}
          diagnostics={fileDiagnostics}
          aiOk={aiOk}
          readOnly={buffer.readOnly}
          onGoto={(loc) => void useStore.getState().gotoLocation(loc)}
          reveal={reveal}
          autoFocus
          onChange={(text) => updateBuffer(active, text)}
          onSave={() => void save()}
          onCursor={(c: CursorInfo) => setCursor(c)}
        />
        </div>
        {showPreview ? (
          <>
            <Splitter
              orientation="vertical"
              label="Breite der Vorschau"
              value={livePreview ?? layout.previewWidth}
              min={260}
              max={760}
              reset={420}
              invert
              onChange={setLivePreview}
              onCommit={(v) => {
                patchLayout({ previewWidth: v });
                setLivePreview(null);
              }}
            />
            <div className="ide-preview" style={{ width: livePreview ?? layout.previewWidth }}>
              <WebPreview />
            </div>
          </>
        ) : null}
        </div>
      </div>
    </>
  );
}
