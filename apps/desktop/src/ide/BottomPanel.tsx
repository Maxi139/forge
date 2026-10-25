import { Button, Icon, Panel, Tabs, type TabItem } from "@forge/ui";
import { useEffect, useMemo, useRef, useState } from "react";
import { useHardware } from "../lib/hardware";
import { MonitorView, PlotterView } from "./MonitorView";
import { t } from "../i18n/de";
import { type Diagnostic, bySeverity, countBySeverity } from "../lib/diagnostics";
import { useAi } from "../lib/ai";
import { useStore } from "../lib/store";
import { AiThreadView } from "./AiThread";
import { projectRelative } from "./Explorer";

const SPLIT_KEY = "forge.monitorSplit";

function readSplit(): boolean {
  try {
    return localStorage.getItem(SPLIT_KEY) === "1";
  } catch {
    return false;
  }
}

export function BottomPanel({ onReveal }: { onReveal(d: Diagnostic): void }) {
  const tab = useStore((s) => s.bottomTab);
  const setTab = useStore((s) => s.setBottomTab);
  const patchLayout = useStore((s) => s.patchLayout);
  const diagnostics = useStore((s) => s.diagnostics);
  const clearOutput = useStore((s) => s.clearOutput);
  const [split, setSplit] = useState(readSplit);
  const toggleSplit = () => {
    setSplit(!split);
    try {
      localStorage.setItem(SPLIT_KEY, split ? "0" : "1");
    } catch {
      /* Ohne Speicher gilt die Wahl nur für diese Sitzung. */
    }
  };
  const serial = tab === "monitor" || tab === "plotter";

  // „Besprechen“ im Editor springt hierher und klappt das Gespräch auf.
  useEffect(() => {
    const open = (e: Event) => {
      const id = (e as CustomEvent<string>).detail;
      setTab("problems");
      patchLayout({ bottomOpen: true });
      useAi.getState().setOpenThread(id);
    };
    window.addEventListener("forge:ai-thread", open);
    return () => window.removeEventListener("forge:ai-thread", open);
  }, [setTab, patchLayout]);

  // Wer den Monitor oder Plotter öffnet, will Daten sehen. Außer er hat den Monitor selbst getrennt.
  const hasDevice = useHardware((s) => !!s.selectedDeviceId);
  const monitorOpen = useHardware((s) => s.monitor.open);
  const userClosed = useHardware((s) => s.monitor.userClosed);
  useEffect(() => {
    if ((tab === "monitor" || tab === "plotter") && hasDevice && !monitorOpen && !userClosed) void useHardware.getState().startMonitor();
  }, [tab, hasDevice, monitorOpen, userClosed]);

  const all = useMemo(() => Object.values(diagnostics).flatMap((perFile) => Object.values(perFile ?? {}).flat()), [diagnostics]);
  const counts = countBySeverity(all);
  const problemCount = counts.error + counts.warning;

  const items: TabItem[] = [
    { id: "monitor", label: t.panels.monitor, icon: "serial" },
    { id: "plotter", label: t.panels.plotter, icon: "plot" },
    { id: "problems", label: t.panels.problems, icon: "warning", count: problemCount || undefined, problems: counts.error > 0 },
    { id: "output", label: t.panels.output, icon: "file-code" },
  ];

  return (
    <>
      <Panel
        className="ide-bottom-panel"
        tabs={
          <Tabs
            variant="panel"
            items={items}
            value={tab}
            onChange={(id) => setTab(id as typeof tab)}
            actions={
              <>
                {serial ? (
                  <Button
                    variant="ghost"
                    size="sm"
                    icon="split"
                    aria-pressed={split}
                    aria-label={t.panels.split}
                    title={split ? t.panels.unsplit : t.panels.split}
                    onClick={toggleSplit}
                  />
                ) : null}
                {tab === "output" ? <Button variant="ghost" size="sm" icon="trash" aria-label={t.panels.clearOutput} title={t.panels.clearOutput} onClick={clearOutput} /> : null}
                <Button variant="ghost" size="sm" icon="chevron-down" aria-label={t.panels.hide} title={t.panels.hide} onClick={() => patchLayout({ bottomOpen: false })} />
              </>
            }
          />
        }
        style={{ flex: 1, minHeight: 0 }}
      >
        {tab === "problems" ? <Problems list={all} onReveal={onReveal} /> : null}
        {tab === "output" ? <Output /> : null}
        {serial && split ? (
          <div className="ide-bottom-split">
            <MonitorView />
            <PlotterView />
          </div>
        ) : null}
        {!split && tab === "monitor" ? <MonitorView /> : null}
        {!split && tab === "plotter" ? <PlotterView /> : null}
      </Panel>
    </>
  );
}

function Problems({ list, onReveal }: { list: Diagnostic[]; onReveal(d: Diagnostic): void }) {
  const root = useStore((s) => s.project?.path ?? "");
  const sorted = useMemo(() => [...list].sort(bySeverity), [list]);
  const threads = useAi((s) => s.threads);
  const openThread = useAi((s) => s.openThread);
  const fixing = useAi((s) => s.fixing);
  const setOpenThread = useAi((s) => s.setOpenThread);
  const [showClosed, setShowClosed] = useState(false);
  const closed = useMemo(
    () => Object.values(threads).filter((th) => th.status === "closed" && th.resolution !== "fixed").sort((a, b) => b.updatedAt - a.updatedAt),
    [threads],
  );
  if (!sorted.length && !closed.length) return <div className="ide-bottom-empty">{t.panels.noProblems}</div>;
  return (
    <div className="problems" role="list">
      {sorted.map((d) => {
        const thread = d.source === "ai" ? threads[d.id.slice(3)] : undefined;
        const expanded = !!thread && openThread === thread.id;
        return (
          <div key={d.id} className={`problem-wrap${expanded ? " is-open" : ""}`}>
        <div className={`problem is-${d.severity}`} role="listitem" onClick={() => onReveal(d)} tabIndex={0} onKeyDown={(e) => e.key === "Enter" && onReveal(d)}>
          <Icon name={d.severity === "error" ? "error" : d.severity === "warning" ? "warning" : "info"} size={14} />
          <span className="msg">{d.message}</span>
          {d.action ? (
            <Button
              size="sm"
              onClick={(e) => {
                e.stopPropagation();
                window.dispatchEvent(new CustomEvent("forge:diagnostic-action", { detail: { action: d.action, diag: d } }));
              }}
            >
              {d.action.label}
            </Button>
          ) : null}
          {d.source === "ai" ? (
            <Button
              size="sm"
              variant="secondary"
              icon="spark"
              title={t.ai.fixTitle}
              busy={!!fixing[d.id]}
              disabled={!!fixing[d.id]}
              onClick={(e) => {
                e.stopPropagation();
                void useAi.getState().fix(d);
              }}
            >
              {fixing[d.id] ? t.ai.fixing : t.ai.fix}
            </Button>
          ) : null}
          {thread ? (
            <Button
              size="sm"
              variant={thread.messages.length > 1 ? "secondary" : "ghost"}
              icon={expanded ? "chevron-down" : "chevron-right"}
              title={t.ai.discussTitle}
              aria-expanded={expanded}
              onClick={(e) => {
                e.stopPropagation();
                setOpenThread(expanded ? null : thread.id);
              }}
            >
              {thread.messages.length > 1 ? `${t.ai.discuss} · ${thread.messages.length}` : t.ai.discuss}
            </Button>
          ) : null}
          <span className="where">{`${projectRelative(root, d.file)}:${d.line}`}</span>
        </div>
        {expanded && thread ? <AiThreadView thread={thread} /> : null}
          </div>
        );
      })}
      {closed.length ? (
        <div className="problem-closed">
          <button type="button" className="problem-closed-toggle" aria-expanded={showClosed} onClick={() => setShowClosed((v) => !v)}>
            <Icon name={showClosed ? "chevron-down" : "chevron-right"} size={12} />
            {t.ai.closedList(closed.length)}
          </button>
          {showClosed
            ? closed.map((th) => (
                <div key={th.id} className={`problem-wrap${openThread === th.id ? " is-open" : ""}`}>
                  <div className="problem is-closed" onClick={() => setOpenThread(openThread === th.id ? null : th.id)} tabIndex={0} onKeyDown={(e) => e.key === "Enter" && setOpenThread(openThread === th.id ? null : th.id)}>
                    <Icon name="check-circle" size={14} />
                    <span className="msg">{th.messages[0]?.text}</span>
                    <span className="where fg-mono">{th.function}</span>
                  </div>
                  {openThread === th.id ? <AiThreadView thread={th} /> : null}
                </div>
              ))
            : null}
        </div>
      ) : null}
    </div>
  );
}

function Output() {
  const lines = useStore((s) => s.output);
  const ref = useRef<HTMLPreElement>(null);
  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [lines.length]);
  if (!lines.length) return <div className="ide-bottom-empty">{t.panels.noOutput}</div>;
  return (
    <pre className="output" ref={ref}>
      {lines.map((l, i) => (
        <div key={i} className={l.kind}>
          {l.text}
        </div>
      ))}
    </pre>
  );
}
