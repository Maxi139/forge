import { StatusBar, StatusDot, StatusItem, type ConnectionState } from "@forge/ui";
import { useMemo } from "react";
import { t } from "../i18n/de";
import { boardName } from "../lib/boards";
import { countBySeverity } from "../lib/diagnostics";
import { baseName, useStore } from "../lib/store";
import { useAi } from "../lib/ai";
import { useHardware } from "../lib/hardware";

export function StatusBarView() {
  const project = useStore((s) => s.project);
  const cursor = useStore((s) => s.cursor);
  const diagnostics = useStore((s) => s.diagnostics);
  const active = useStore((s) => s.active);
  const dirty = useStore((s) => (s.active ? s.buffers[s.active]?.text !== s.buffers[s.active]?.saved : false));
  const setBottomTab = useStore((s) => s.setBottomTab);
  const device = useHardware((s) => s.devices.find((d) => d.id === s.selectedDeviceId) ?? null);
  const devicesLoaded = useHardware((s) => s.devicesLoaded);
  const activity = useHardware((s) => s.activity);
  const build = useHardware((s) => s.build);
  const failed = useAi((s) => s.failed);
  const unchecked = useMemo(() => Object.values(failed).flat(), [failed]);

  const counts = useMemo(() => countBySeverity(Object.values(diagnostics).flatMap((perFile) => Object.values(perFile ?? {}).flat()).filter((d) => d.source !== "ai")), [diagnostics]);

  const state: ConnectionState = activity === "compiling" ? "compiling" : activity === "flashing" ? "flashing" : device ? "connected" : devicesLoaded ? "idle" : "scanning";
  const progress = build && build.status === "running" ? (build.stage + build.progress) / (build.mode === "compile" ? 2 : 6) : undefined;

  return (
    <StatusBar
      progress={progress}
      left={
        <>
          <StatusItem onClick={() => useStore.getState().setBottomTab("monitor")} title={device?.port}>
            <StatusDot state={state} label={device && activity === "idle" ? baseName(device.port) : state === "idle" ? t.titlebar.noPort : undefined} />
          </StatusItem>
          {project ? (
            <StatusItem icon="chip" onClick={() => useStore.getState().setDialog({ kind: "boardPicker" })}>
              {boardName(project.config.board.fqbn)}
            </StatusItem>
          ) : null}
          <StatusItem icon={counts.error ? "error" : counts.warning ? "warning" : "check-circle"} onClick={() => setBottomTab("problems")} title={t.panels.problems}>
            {counts.error || counts.warning ? t.status.problems(counts.error, counts.warning) : t.status.ready}
          </StatusItem>
          {unchecked.length ? (
            <StatusItem icon="spark" onClick={() => useStore.getState().setDialog({ kind: "settings" })} title={unchecked[0].message}>
              {t.ai.notChecked(unchecked.length)}
            </StatusItem>
          ) : null}
        </>
      }
      right={
        active ? (
          <>
            <StatusItem>{t.status.line(cursor.line, cursor.col)}</StatusItem>
            {cursor.selected ? <StatusItem>{t.status.selected(cursor.selected)}</StatusItem> : null}
            <StatusItem>{dirty ? t.status.unsaved : t.status.saved}</StatusItem>
            <StatusItem>UTF-8</StatusItem>
            <StatusItem>C++ (Arduino)</StatusItem>
          </>
        ) : null
      }
    />
  );
}
