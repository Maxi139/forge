import { Badge, BuildProgress, Button, Icon, MemoryMeter, PartitionMap, Panel, Pinout, StatusDot } from "@forge/ui";
import { useEffect, useMemo } from "react";
import { t } from "../i18n/de";
import { boardName, chipOf } from "../lib/boards";
import { useHardware } from "../lib/hardware";
import { usedPins } from "../lib/pins";
import { baseName, useStore } from "../lib/store";
import { BUILD_STAGES } from "@forge/ui";

const FLASH_SIZES: Record<string, number> = { "2M": 2, "4M": 4, "8M": 8, "16M": 16, "32M": 32, "64M": 64 };

export function DevicePanel() {
  const project = useStore((s) => s.project);
  const buffers = useStore((s) => s.buffers);
  const mainText = useStore((s) => (s.project ? s.buffers[s.project.mainFile]?.text : undefined));
  const device = useHardware((s) => s.devices.find((d) => d.id === s.selectedDeviceId) ?? null);
  const identity = useHardware((s) => (s.selectedDeviceId ? s.identity[s.selectedDeviceId] : undefined));
  const identifying = useHardware((s) => s.identifying !== null && s.identifying === s.selectedDeviceId);
  const activity = useHardware((s) => s.activity);
  const build = useHardware((s) => s.build);
  const last = useHardware((s) => s.lastBuild);
  const options = useHardware((s) => (project ? s.boardOptions[project.config.board.fqbn] : undefined));
  const flash = useHardware((s) => s.flash);
  const secrets = useHardware((s) => s.secrets);
  const refreshSecrets = useHardware((s) => s.refreshSecrets);

  useEffect(() => {
    void refreshSecrets();
  }, [refreshSecrets, project?.path]);

  const fqbn = project?.config.board.fqbn ?? "";
  const chip = chipOf(fqbn);
  const isS3 = chip === "ESP32-S3";

  // Pins aus allen offenen Dateien.
  const pins = useMemo(() => {
    const all = Object.values(buffers).filter((b) => !b.readOnly).map((b) => b.text).join("\n");
    return usedPins(all || mainText || "");
  }, [buffers, mainText]);

  const flashMb = useMemo(() => {
    const v = project?.config.board.options.FlashSize ?? options?.find((o) => o.id === "FlashSize")?.default ?? "4M";
    return FLASH_SIZES[v] ?? 4;
  }, [project, options]);

  const state = activity === "compiling" ? "compiling" : activity === "flashing" ? "flashing" : device ? "connected" : "idle";

  return (
    <Panel title={t.sidebar.device}>
      <div className="board-card">
        <span className="ic">
          <Icon name="chip" size={18} />
        </span>
        <div style={{ minWidth: 0, flex: 1 }}>
          <b>{project ? boardName(fqbn) : t.titlebar.noBoard}</b>
          <small>{device ? (identifying ? t.device.identifying : `${baseName(device.port)}`) : t.device.noDevice}</small>
        </div>
        <StatusDot state={state} label={false} />
      </div>

      {device ? (
        <div className="kv">
          <span>{t.device.chip}</span>
          <span className="fg-mono">{identity?.chip ?? (device.mac ? chip : device.label)}</span>
          {identity?.mac ?? device.mac ? (
            <>
              <span>{t.device.mac}</span>
              <span className="fg-mono">{identity?.mac ?? device.mac}</span>
            </>
          ) : null}
          <span>{t.device.port}</span>
          <span className="fg-mono" title={device.port}>
            {device.port}
          </span>
        </div>
      ) : (
        <div className="sb-empty" style={{ padding: 0 }}>
          {t.device.noDeviceText}
        </div>
      )}

      {build ? (
        <>
          <div className="sec">Build</div>
          <BuildProgress
            stages={build.mode === "flash" ? BUILD_STAGES : BUILD_STAGES.slice(0, 2)}
            stage={Math.min(build.stage, build.mode === "flash" ? 5 : 1)}
            progress={build.progress}
            status={build.status === "running" ? "running" : build.status === "done" ? "done" : "error"}
            title={build.title}
            target={build.target}
            detail={build.detail}
          />
        </>
      ) : null}

      {last ? (
        <>
          <div className="sec">{t.device.memory}</div>
          <MemoryMeter items={last.sizes.map((s) => ({ label: s.label, used: s.used, total: s.total }))} />
          {last.partitions.length ? (
            <>
              <div className="sec">{t.device.partitions}</div>
              <PartitionMap partitions={last.partitions.map((p) => ({ name: p.name, type: p.type, offset: p.offset, size: p.size }))} flashSize={flashMb * 1024 * 1024} />
            </>
          ) : null}
        </>
      ) : (
        <>
          <div className="sec">{t.device.memory}</div>
          <div className="sb-empty" style={{ padding: 0 }}>
            {t.device.noBuildText}
          </div>
        </>
      )}

      {isS3 ? (
        <>
          <div className="sec">{t.device.pinout}</div>
          <Pinout used={pins} boardLabel="DevKitC-1" />
        </>
      ) : null}

      <div className="sec">{t.secrets.title}</div>
      <div className="secret-section">
        {secrets.length === 0 ? <div className="sb-empty" style={{ padding: 0 }}>{t.secrets.none}</div> : null}
        {secrets.map((x) => (
          <div key={x.name} className="secret-chip">
            <span className="fg-mono">{x.name}</span>
            {x.set ? <Badge tone="success" icon="check">{t.secrets.set}</Badge> : <Badge tone="warning" icon="warning">{t.secrets.missing}</Badge>}
          </div>
        ))}
        <div>
          <Button size="sm" variant="ghost" icon="wifi" onClick={() => useStore.getState().setDialog({ kind: "secrets" })}>
            {t.secrets.manage}
          </Button>
        </div>
      </div>

      {device ? (
        <div className="device-actions">
          <Button size="sm" variant="ghost" icon="refresh" onClick={() => void useHardware.getState().identify(device.id, true)} disabled={activity !== "idle" || identifying}>
            {t.device.identifyAgain}
          </Button>
          <Button size="sm" variant="ghost" icon="erase" onClick={async () => {
              const ok = await useStore.getState().confirm({ title: t.device.eraseTitle, text: t.device.eraseText, confirm: t.device.eraseConfirm, tone: "error" });
              if (ok) void flash({ eraseAll: true, force: true });
            }} disabled={activity !== "idle"}>
            {t.device.eraseFlash}
          </Button>
        </div>
      ) : null}
    </Panel>
  );
}
