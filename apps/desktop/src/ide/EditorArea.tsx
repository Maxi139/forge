import { Badge, Button, EmptyState, Icon, Tabs, type TabItem } from "@forge/ui";
import { useEffect, useMemo, useRef } from "react";
import { applyEdits } from "../editor/diagnostics";
import { ForgeEditor, type CursorInfo, type ForgeEditorHandle, type Reveal } from "../editor/ForgeEditor";
import { t } from "../i18n/de";
import { useAi } from "../lib/ai";
import { baseName, diagnosticsForFile, useStore } from "../lib/store";
import { projectRelative } from "./Explorer";
import { Welcome } from "./Welcome";

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
          <span style={{ flex: 1 }} />
          {!buffer.readOnly ? (
            <Button variant="ghost" size="sm" icon="spark" onClick={() => setDialog({ kind: "aiTask" })} title={t.ai.taskTitle}>
              {t.ai.taskShort}
            </Button>
          ) : null}
          {buffer.readOnly ? (
            <Badge icon="info">{t.editor.readOnly}</Badge>
          ) : (
            <Button variant="ghost" size="sm" icon="check" disabled={buffer.text === buffer.saved} onClick={() => void save(active)} title="Speichern  ⌘S">
              {t.dialogs.save}
            </Button>
          )}
        </div>
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
          onSave={() => void save(active)}
          onCursor={(c: CursorInfo) => setCursor(c)}
        />
      </div>
    </>
  );
}
