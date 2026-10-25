import { Button, EmptyState, Plotter, SerialMonitor, type ConnectionState, type MonitorFrame } from "@forge/ui";
import { t } from "../i18n/de";
import { backend } from "../lib/backend";
import { useHardware } from "../lib/hardware";
import { isDecodable, projectFileFor } from "../lib/monitor";
import { useStore } from "../lib/store";

function connection(state: ReturnType<typeof useHardware.getState>["monitor"]["state"]): ConnectionState {
  switch (state.state) {
    case "connected":
      return "connected";
    case "connecting":
      return "scanning";
    case "reconnecting":
      return "error";
    default:
      return "idle";
  }
}

async function exportLog(text: string) {
  try {
    const path = await backend().saveTextAs(t.monitor.exportName, text);
    if (path) useStore.getState().toast({ kind: "success", title: t.monitor.exported(path) });
  } catch (e) {
    useStore.getState().toast({ kind: "error", title: t.errors.generic, message: e instanceof Error ? e.message : String(e) });
  }
}

async function decodeLine(text: string): Promise<MonitorFrame[]> {
  const frames = await backend().decodeBacktrace(text);
  const project = useStore.getState().project;
  return frames.map((f) => ({ ...f, openable: !!project && projectFileFor(f.file, project.path, project.mainFile) != null }));
}

function openFrame(f: MonitorFrame) {
  const project = useStore.getState().project;
  const file = project ? projectFileFor(f.file, project.path, project.mainFile) : null;
  if (file) void useStore.getState().gotoLocation({ file, line: f.line, col: 0 });
}

export function MonitorView() {
  const device = useHardware((s) => s.devices.find((d) => d.id === s.selectedDeviceId) ?? null);
  const monitor = useHardware((s) => s.monitor);
  const lines = useHardware((s) => s.monitorLines);
  const { sendToMonitor, clearMonitor, setBaud, startMonitor, stopMonitor, resetBoard } = useHardware.getState();

  if (!device) {
    return <EmptyState icon="serial" title={t.monitor.noDevice} text={t.monitor.noDeviceText} pattern={false} />;
  }
  return (
    <SerialMonitor
      lines={lines}
      port={device.port}
      baud={monitor.baud}
      state={connection(monitor.state)}
      onSend={(text, ending) => void sendToMonitor(text, ending)}
      onClear={clearMonitor}
      onBaud={(b) => void setBaud(b)}
      onReset={monitor.open ? () => void resetBoard() : undefined}
      canDecode={(l) => l.kind !== "tx" && isDecodable(l.text)}
      onDecode={(l) => decodeLine(l.text)}
      onOpenFrame={openFrame}
      onExport={(text) => void exportLog(text)}
      actions={
        monitor.open ? (
          <Button variant="ghost" size="sm" icon="pause" aria-label="Monitor trennen" title="Monitor trennen" onClick={() => void stopMonitor()} />
        ) : (
          <Button variant="ghost" size="sm" icon="play" onClick={() => void startMonitor()}>
            {t.monitor.start}
          </Button>
        )
      }
    />
  );
}

export function PlotterView() {
  const device = useHardware((s) => s.devices.find((d) => d.id === s.selectedDeviceId) ?? null);
  const plot = useHardware((s) => s.plot);
  const paused = useHardware((s) => s.plotPaused);
  const toggle = useHardware((s) => s.togglePlotPause);

  if (!device) return <EmptyState icon="plot" title={t.monitor.noDevice} text={t.monitor.noDeviceText} pattern={false} />;
  if (plot.length === 0) return <EmptyState icon="plot" title={t.monitor.plotterEmpty} text={t.monitor.plotterEmptyText} pattern={false} />;
  return <Plotter series={plot.slice(0, 4).map((p) => ({ name: p.name, values: p.values }))} window={200} windowLabel="letzte Werte" paused={paused} onPause={toggle} />;
}
