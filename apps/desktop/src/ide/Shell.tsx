import { ErrorBoundary } from "./ErrorBoundary";
import { ActivityBar, CommandPalette, ContextMenu, Panel, Splitter, TitleBar, Toast, ToastStack, type ActivityItem, type MenuItem } from "@forge/ui";
import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { useEditorHooks } from "../editor/autosave";
import type { Reveal } from "../editor/ForgeEditor";
import { loadSymbols } from "../editor/symbols";
import { t } from "../i18n/de";
import { backend } from "../lib/backend";
import { boardName } from "../lib/boards";
import type { Diagnostic } from "../lib/diagnostics";
import { useAi } from "../lib/ai";
import { useHardware } from "../lib/hardware";
import { baseName, useStore } from "../lib/store";
import { fileCommands, useRecentFiles } from "../lib/quickOpen";
import type { LspSymbol } from "../lib/backend";
import type { Command } from "@forge/ui";
import { BottomPanel } from "./BottomPanel";
import { buildCommands } from "./commands";
import { DevicePanel } from "./DevicePanel";
import { Dialogs } from "./Dialogs";
import { EditorArea } from "./EditorArea";
import { Explorer } from "./Explorer";
import { LibraryView } from "./LibraryView";
import { StatusBarView } from "./StatusBarView";
import { useShortcuts } from "./shortcuts";
import { useLiveAi, useLiveAnalysis, useLiveChecks, useLiveLsp, useLiveWebChecks } from "./useLiveChecks";
import { SearchView } from "./SearchView";
import { useNativeEvents } from "./useNativeEvents";

const ACTIVITY: ActivityItem[] = [
  { id: "files", icon: "files", label: t.sidebar.files },
  { id: "search", icon: "search", label: t.sidebar.search },
  { id: "library", icon: "library", label: t.sidebar.library },
  { id: "device", icon: "device", label: t.sidebar.device },
  { id: "plot", icon: "plot", label: t.sidebar.plot },
];

interface PortMenu {
  x: number;
  y: number;
}

export function Shell() {
  useShortcuts();
  useNativeEvents();
  useLiveChecks();
  useLiveAnalysis();
  useLiveAi();
  useLiveLsp();
  useLiveWebChecks();
  useEditorHooks();

  const project = useStore((s) => s.project);
  const layout = useStore((s) => s.layout);
  const sidebarView = useStore((s) => s.sidebarView);
  const paletteOpen = useStore((s) => s.paletteOpen);
  const toasts = useStore((s) => s.toasts);
  const patchLayout = useStore((s) => s.patchLayout);
  const setSidebarView = useStore((s) => s.setSidebarView);
  const setBottomTab = useStore((s) => s.setBottomTab);
  const togglePalette = useStore((s) => s.togglePalette);
  const dismissToast = useStore((s) => s.dismissToast);
  const openFile = useStore((s) => s.openFile);
  const setDialog = useStore((s) => s.setDialog);

  const devices = useHardware((s) => s.devices);
  const selectedId = useHardware((s) => s.selectedDeviceId);
  const activity = useHardware((s) => s.activity);
  const selectDevice = useHardware((s) => s.selectDevice);
  const refreshDevices = useHardware((s) => s.refreshDevices);
  const device = devices.find((d) => d.id === selectedId) ?? null;

  const [reveal, setReveal] = useState<Reveal | null>(null);
  const [portMenu, setPortMenu] = useState<PortMenu | null>(null);
  const [live, setLive] = useState<Partial<Record<"sidebarWidth" | "bottomHeight" | "deviceWidth", number>>>({});

  const onReveal = useCallback(
    async (d: Diagnostic) => {
      await openFile(d.file);
      setReveal({ line: d.line, col: d.from + 1, nonce: Date.now() });
    },
    [openFile],
  );

  // Eine angenommene Behebung räumt ihre Meldung sofort ab.
  useEffect(() => {
    const on = (e: Event) => {
      const d = (e as CustomEvent<Diagnostic>).detail;
      useStore.getState().dismissDiagnostic(d);
      if (d.source === "ai") useAi.getState().dismiss(d.file, d.id);
    };
    window.addEventListener("forge:fix-applied", on);
    return () => window.removeEventListener("forge:fix-applied", on);
  }, []);

  // „Beheben“ im Editor: die KI arbeitet die Änderung aus.
  useEffect(() => {
    const on = (e: Event) => void useAi.getState().fix((e as CustomEvent<Diagnostic>).detail);
    window.addEventListener("forge:ai-fix", on);
    return () => window.removeEventListener("forge:ai-fix", on);
  }, []);

  // „Zum Fehler“ aus einer Meldung.
  useEffect(() => {
    const on = (e: Event) => void onReveal((e as CustomEvent<Diagnostic>).detail);
    window.addEventListener("forge:reveal", on);
    return () => window.removeEventListener("forge:reveal", on);
  }, [onReveal]);

  // Aktionen aus Meldungen, z. B. „Bibliothek suchen“.
  useEffect(() => {
    const on = (e: Event) => {
      const { action: a, diag } = (e as CustomEvent<{ action: { kind: string; arg: string }; diag: Diagnostic }>).detail;
      if (a.kind === "searchLibrary") useHardware.getState().searchLibrary(a.arg);
      if (a.kind === "moveToKeychain") {
        // Der Wert steht in der Datei, nie in der Meldung. Die Spalten der Meldung umfassen die Anführungszeichen.
        const text = useStore.getState().buffers[diag.file]?.text ?? "";
        const line = text.split("\n")[diag.line - 1] ?? "";
        const value = line.slice(diag.from + 1, diag.to - 1);
        const [name, mode] = a.arg.split("|");
        setDialog({ kind: "moveSecret", file: diag.file, line: diag.line, from: diag.from, to: diag.to, value, name, mode: mode === "call" ? "call" : "decl" });
      }
    };
    window.addEventListener("forge:diagnostic-action", on);
    return () => window.removeEventListener("forge:diagnostic-action", on);
  }, [setDialog]);

  const commands = useMemo(() => (paletteOpen ? buildCommands() : []), [paletteOpen]);
  const paletteMode = useStore((s) => s.paletteMode);
  const tree = useStore((s) => s.tree);
  const activeFile = useStore((s) => s.active);
  const recentFiles = useRecentFiles((s) => s.files);
  const [symbols, setSymbols] = useState<LspSymbol[]>([]);
  useEffect(() => {
    if (!paletteOpen || paletteMode !== "symbols" || !activeFile) return;
    let alive = true;
    void loadSymbols(activeFile).then((list) => alive && setSymbols(list));
    return () => {
      alive = false;
    };
  }, [paletteOpen, paletteMode, activeFile]);
  // Je nach Modus sucht die Palette Befehle, Dateien oder Symbole der aktiven Datei.
  const paletteItems = useMemo<Command[]>(() => {
    if (!paletteOpen) return [];
    const labels = { recent: t.editorTools.recent, files: t.editorTools.files };
    const files = project ? fileCommands(tree, project.path, recentFiles, labels, activeFile) : [];
    if (paletteMode === "files") return files;
    if (paletteMode === "symbols") {
      return symbols.map((s, i) => ({
        id: `sym:${i}`,
        label: s.name,
        icon: "file-code" as const,
        hint: [s.container, t.editorTools.symbolLine(s.line)].filter(Boolean).join(" · "),
        group: t.editorTools.symbolKinds[s.kind] ?? t.editorTools.files,
        search: s.detail ?? undefined,
      }));
    }
    // Im Befehlsmodus tauchen Dateien erst auf, wenn man tippt.
    return [...commands, ...files.filter((f) => !f.emptyOnly).map((f) => ({ ...f, searchOnly: true }))];
  }, [paletteOpen, paletteMode, commands, tree, project, recentFiles, activeFile, symbols]);

  const style = {
    "--sidebar-w": `${live.sidebarWidth ?? layout.sidebarWidth}px`,
    "--bottom-h": `${live.bottomHeight ?? layout.bottomHeight}px`,
    "--device-w": `${live.deviceWidth ?? layout.deviceWidth}px`,
  } as CSSProperties;

  const activityValue = layout.sidebarOpen ? sidebarView : undefined;
  const outdatedCount = useHardware((s) => s.outdated.length);
  const activityItems = useMemo(() => ACTIVITY.map((i) => (i.id === "library" && outdatedCount > 0 ? { ...i, badge: outdatedCount } : i)), [outdatedCount]);

  const onActivity = (id: string) => {
    if (id === "files" || id === "search" || id === "library") setSidebarView(id);
    else if (id === "device") patchLayout({ deviceOpen: !layout.deviceOpen });
    else if (id === "plot") setBottomTab("plotter");
  };

  const portItems: MenuItem[] = devices.length
    ? [
        ...devices.map((d) => ({ id: d.id, label: `${baseName(d.port)}  ${d.label}`, icon: (d.id === selectedId ? "check" : d.likelyEsp ? "usb" : "serial") as MenuItem["icon"] })),
        { id: "__rescan", label: t.hardware.rescan, icon: "refresh" as const, separatorBefore: true },
      ]
    : [
        { id: "__none", label: t.hardware.noPortsItem, icon: "usb" as const, disabled: true },
        { id: "__rescan", label: t.hardware.rescan, icon: "refresh" as const, separatorBefore: true },
      ];

  const phase = activity === "compiling" ? "compiling" : activity === "flashing" ? "flashing" : "idle";

  return (
    <div className="ide" style={style}>
      <TitleBar
        nativeInset={backend().native}
        project={project?.name ?? t.app.name}
        onProjectRename={project ? (n) => void useStore.getState().renameProject(n) : undefined}
        board={project ? boardName(project.config.board.fqbn) : t.titlebar.noBoard}
        port={device ? baseName(device.port) : t.titlebar.noPort}
        connection={activity === "compiling" ? "compiling" : activity === "flashing" ? "flashing" : device ? "connected" : "idle"}
        phase={phase}
        searchLabel={t.titlebar.search}
        flashDisabled={!project || activity === "compiling"}
        buildDisabled={!project}
        onSearch={() => togglePalette(true)}
        onBoard={() => project && setDialog({ kind: "boardPicker" })}
        onPort={(a) => setPortMenu({ x: a.left, y: a.bottom + 4 })}
        onBuild={() => void useHardware.getState().compile()}
        onFlash={() => void useHardware.getState().flash()}
        onCancel={() => void useHardware.getState().cancel()}
      />

      <div className="ide-main">
        <div className={`ide-side${layout.sidebarOpen ? "" : " is-collapsed"}`}>
          <div className="fg-sidebar">
            <ActivityBar items={activityItems} bottom={[...(project ? [{ id: "home", icon: "folder" as const, label: t.sidebar.home }] : []), { id: "settings", icon: "settings" as const, label: t.sidebar.settings }]} value={activityValue} onChange={(id) => (id === "settings" ? setDialog({ kind: "settings" }) : id === "home" ? void useStore.getState().closeProject() : onActivity(id))} />
            {layout.sidebarOpen ? (
              sidebarView === "library" ? (
                <ErrorBoundary region><LibraryView /></ErrorBoundary>
              ) : sidebarView === "search" ? (
                <ErrorBoundary region><SearchView /></ErrorBoundary>
              ) : project ? (
                <ErrorBoundary region><Explorer /></ErrorBoundary>
              ) : (
                <Panel title={t.sidebar.files}>
                  <div className="sb-empty">{t.sidebar.noProject}</div>
                </Panel>
              )
            ) : null}
          </div>
        </div>
        {layout.sidebarOpen ? (
          <Splitter
            orientation="vertical"
            label="Breite der Seitenleiste"
            value={live.sidebarWidth ?? layout.sidebarWidth}
            min={180}
            max={520}
            reset={248}
            onChange={(v) => setLive((l) => ({ ...l, sidebarWidth: v }))}
            onCommit={(v) => {
              patchLayout({ sidebarWidth: v });
              setLive((l) => ({ ...l, sidebarWidth: undefined }));
            }}
          />
        ) : null}

        <div className="ide-center">
          <ErrorBoundary region><EditorArea reveal={reveal} /></ErrorBoundary>
          {layout.bottomOpen && project ? (
            <>
              <Splitter
                orientation="horizontal"
                label="Höhe des unteren Panels"
                value={live.bottomHeight ?? layout.bottomHeight}
                min={120}
                max={640}
                reset={248}
                invert
                onChange={(v) => setLive((l) => ({ ...l, bottomHeight: v }))}
                onCommit={(v) => {
                  patchLayout({ bottomHeight: v });
                  setLive((l) => ({ ...l, bottomHeight: undefined }));
                }}
              />
              <div className="ide-bottom">
                <ErrorBoundary region><BottomPanel onReveal={(d) => void onReveal(d)} /></ErrorBoundary>
              </div>
            </>
          ) : null}
        </div>

        {layout.deviceOpen ? (
          <>
            <Splitter
              orientation="vertical"
              label="Breite der Geräteansicht"
              value={live.deviceWidth ?? layout.deviceWidth}
              min={260}
              max={520}
              reset={312}
              invert
              onChange={(v) => setLive((l) => ({ ...l, deviceWidth: v }))}
              onCommit={(v) => {
                patchLayout({ deviceWidth: v });
                setLive((l) => ({ ...l, deviceWidth: undefined }));
              }}
            />
            <aside className="ide-device">
              <ErrorBoundary region><DevicePanel /></ErrorBoundary>
            </aside>
          </>
        ) : null}
      </div>

      <StatusBarView />

      {paletteOpen ? (
        <div
          className="ide-overlay"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) togglePalette(false);
          }}
        >
          <CommandPalette
            key={paletteMode}
            autoFocus
            items={paletteItems}
            placeholder={paletteMode === "files" ? t.editorTools.quickPlaceholder : paletteMode === "symbols" ? t.editorTools.symbolPlaceholder : undefined}
            onClose={() => togglePalette(false)}
            onRun={(item) => {
              togglePalette(false);
              const sep = item.id.indexOf(":");
              const kind = sep > 0 ? item.id.slice(0, sep) : "";
              const rest = item.id.slice(sep + 1);
              if (kind === "file" || kind === "recent") void openFile(rest);
              else if (kind === "sym") {
                const sym = symbols[Number(rest)];
                if (sym && activeFile) void useStore.getState().gotoLocation({ file: activeFile, line: sym.line, col: sym.col });
              } else commands.find((c) => c.id === item.id)?.run();
            }}
          />
        </div>
      ) : null}

      {portMenu ? (
        <ContextMenu
          x={portMenu.x}
          y={portMenu.y}
          items={portItems}
          onClose={() => setPortMenu(null)}
          onSelect={(id) => {
            setPortMenu(null);
            if (id === "__rescan") void refreshDevices();
            else selectDevice(id);
          }}
        />
      ) : null}

      <Dialogs />

      <div className="ide-toasts">
        <ToastStack>
          {toasts.map((x) => (
            <Toast
              key={x.id}
              kind={x.kind}
              title={x.title}
              message={x.message}
              progress={x.progress}
              onClose={x.kind === "progress" ? undefined : () => dismissToast(x.id)}
              actions={
                x.actions?.length || (x.kind === "progress" && x.onCancel) ? (
                  <>
                    {x.actions?.map((a) => (
                      <button key={a.label} className={`fg-btn fg-btn--${a.primary ? "primary" : "secondary"} fg-btn--sm`} onClick={() => (dismissToast(x.id), a.run())}>
                        <span>{a.label}</span>
                      </button>
                    ))}
                    {x.kind === "progress" && x.onCancel ? (
                      <button className="fg-btn fg-btn--ghost fg-btn--sm" onClick={x.onCancel}>
                        <span>{t.hardware.cancel}</span>
                      </button>
                    ) : null}
                  </>
                ) : undefined
              }
            />
          ))}
        </ToastStack>
      </div>
    </div>
  );
}
