import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { cx, formatBytes } from "../util";
import { Badge, Button, Kbd, Select, StatusDot, Toggle, type ConnectionState } from "./Controls";
import { Icon } from "./Icon";

/* ---------- Serial Monitor ---------- */
export interface MonitorLine {
  id?: string | number;
  /** Millisekunden seit Verbindung. */
  t?: number;
  kind?: "rx" | "tx" | "sys" | "err";
  text: string;
}

export type LineEnding = "none" | "nl" | "nlcr";

export interface SerialMonitorProps {
  lines: MonitorLine[];
  port?: string;
  baud?: number;
  state?: ConnectionState;
  timestamps?: boolean;
  onSend?(text: string, ending: LineEnding): void;
  onClear?(): void;
  onBaud?(baud: number): void;
  height?: number;
  className?: string;
  /** Zusätzliche Schaltflächen am Ende der Leiste. */
  actions?: ReactNode;
}

const BAUDS = ["9600", "57600", "115200", "230400", "460800", "921600"];

function formatTimestamp(ms: number): string {
  const s = Math.floor(ms / 1000);
  const mm = Math.floor(s / 60);
  const ss = s % 60;
  return `${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}.${String(ms % 1000).padStart(3, "0")}`;
}

/** Hebt `name:wert`-Paare hervor, wie sie der Plotter liest. */
function keyValues(text: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const re = /(\w+)(:)(-?\d+(?:\.\d+)?)/g;
  let last = 0;
  let n = 0;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text))) {
    if (m.index > last) parts.push(text.slice(last, m.index));
    parts.push(
      <span key={n++} className="k">
        {m[1]}
        {m[2]}
      </span>,
    );
    parts.push(m[3]);
    last = re.lastIndex;
  }
  parts.push(text.slice(last));
  return parts;
}

export function SerialMonitor({
  lines,
  port,
  baud = 115200,
  state = "connected",
  timestamps = true,
  onSend,
  onClear,
  onBaud,
  height,
  className,
  actions,
}: SerialMonitorProps) {
  const [input, setInput] = useState("");
  const [ts, setTs] = useState(timestamps);
  const [auto, setAuto] = useState(true);
  const [ending, setEnding] = useState<LineEnding>("nl");
  const out = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (auto && out.current) out.current.scrollTop = out.current.scrollHeight;
  }, [lines.length, auto]);

  return (
    <div className={cx("fg-monitor", className)} style={height ? { height } : undefined}>
      <div className="fg-mon-bar">
        <StatusDot state={state} />
        <span className="fg-mon-port">{port ?? "Kein Port"}</span>
        <span className="fg-spacer" />
        <Toggle icon="clock" title="Zeitstempel" on={ts} onToggle={() => setTs(!ts)} />
        <Toggle icon="arrow-down" title="Autoscroll" on={auto} onToggle={() => setAuto(!auto)} />
        <Select
          size="sm"
          mono
          aria-label="Baudrate"
          value={String(baud)}
          onChange={(e) => onBaud?.(Number(e.target.value))}
          options={BAUDS}
        />
        <Button variant="ghost" size="sm" icon="trash" aria-label="Ausgabe leeren" title="Ausgabe leeren" onClick={onClear} />
        {actions}
      </div>
      <div className="fg-mon-out" ref={out} role="log" aria-live="off">
        {lines.map((l, i) => {
          const kind = l.kind ?? "rx";
          return (
            <div key={l.id ?? i} className={`fg-mon-line fg-mon-line--${kind}`}>
              {ts && l.t != null ? <span className="fg-mon-ts">{formatTimestamp(l.t)}</span> : null}
              {kind === "tx" ? <span className="fg-mon-dir">→</span> : null}
              <span className="fg-mon-text">{kind === "rx" ? keyValues(l.text) : l.text}</span>
            </div>
          );
        })}
      </div>
      <div className="fg-mon-in">
        <span className="fg-mon-prompt">›</span>
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || !input) return;
            onSend?.(input, ending);
            setInput("");
          }}
          placeholder="An das Board senden …"
          aria-label="Nachricht senden"
        />
        <Select
          size="sm"
          aria-label="Zeilenende"
          value={ending}
          onChange={(e) => setEnding(e.target.value as LineEnding)}
          options={[
            { value: "none", label: "Kein Zeilenende" },
            { value: "nl", label: "NL" },
            { value: "nlcr", label: "NL & CR" },
          ]}
        />
        <Kbd keys={["↵"]} />
      </div>
    </div>
  );
}

/* ---------- Plotter ---------- */
export interface PlotSeries {
  name: string;
  values: number[];
  unit?: string;
}

export interface PlotterProps {
  series: PlotSeries[];
  min?: number;
  max?: number;
  window?: number;
  interval?: number;
  windowLabel?: string;
  paused?: boolean;
  onPause?(): void;
  actions?: ReactNode;
  height?: number;
  className?: string;
}

export function Plotter({ series, min: fixedMin, max: fixedMax, window: n = 160, interval = 50, windowLabel, paused, onPause, actions, height, className }: PlotterProps) {
  const H = 100;
  const W = 100;
  const all = series.flatMap((s) => s.values.slice(-n));
  const min = fixedMin ?? Math.min(0, ...all);
  let max = fixedMax ?? Math.max(1, ...all);
  if (max === min) max = min + 1;
  const y = (v: number) => H - ((v - min) / (max - min)) * H;
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => min + (max - min) * f);
  const fmt = (v: number) => {
    const a = Math.abs(max - min);
    return a >= 100 ? String(Math.round(v)) : a >= 10 ? v.toFixed(1) : v.toFixed(2);
  };

  return (
    <div className={cx("fg-plot", paused && "is-paused", className)} style={height ? { height } : undefined}>
      <div className="fg-plot-head">
        {series.map((s, i) => {
          const last = s.values[s.values.length - 1];
          return (
            <span key={s.name} className="fg-legend">
              <i className={`fg-sw fg-sw-${i + 1}`} />
              {s.name}
              <span className="fg-mono">{last == null ? "" : fmt(last) + (s.unit ? ` ${s.unit}` : "")}</span>
            </span>
          );
        })}
        <span className="fg-spacer" />
        {actions}
        {onPause ? (
          <Button variant="ghost" size="sm" icon={paused ? "play" : "pause"} onClick={onPause}>
            {paused ? "Weiter" : "Pause"}
          </Button>
        ) : null}
      </div>
      <div className="fg-plot-area">
        {ticks.map((t, i) => (
          <span key={i} className="fg-plot-y" style={{ top: `${100 - i * 25}%` }}>
            {fmt(t)}
          </span>
        ))}
        <span className="fg-plot-x" style={{ left: 0 }}>{`−${windowLabel ?? `${((n * interval) / 1000).toFixed(0)} s`}`}</span>
        <span className="fg-plot-x" style={{ right: 0 }}>jetzt</span>
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" aria-hidden>
          {ticks.map((t, i) => (
            <line key={i} className={cx("fg-plot-grid", t === 0 && "fg-plot-zero")} x1={0} x2={W} y1={y(t)} y2={y(t)} />
          ))}
          {series.map((s, i) => {
            const v = s.values.slice(-n);
            const off = n - v.length;
            const d = v
              .map((val, j) => `${j ? "L" : "M"}${(((j + off) / (n - 1)) * W).toFixed(2)} ${y(val).toFixed(2)}`)
              .join("");
            return <path key={s.name} className={`fg-plot-line fg-pl-${i + 1}`} d={d} />;
          })}
        </svg>
        <span className="fg-plot-now" />
        {series.map((s, i) => {
          const last = s.values[s.values.length - 1];
          return last == null ? null : <span key={s.name} className={`fg-plot-head-dot d-${i + 1}`} style={{ top: `${y(last)}%` }} />;
        })}
      </div>
    </div>
  );
}

/* ---------- Build Progress ---------- */
export const BUILD_STAGES = ["Kompilieren", "Linken", "Verbinden", "Löschen", "Schreiben", "Prüfen"];

export interface BuildProgressProps {
  stage?: number;
  progress?: number;
  status?: "running" | "done" | "error";
  stages?: string[];
  title?: string;
  target?: string;
  detail?: ReactNode;
  className?: string;
}

export function BuildProgress({ stage = 0, progress = 0, status = "running", stages = BUILD_STAGES, title, target, detail, className }: BuildProgressProps) {
  const total = status === "done" ? 1 : (stage + progress) / stages.length;
  const state: ConnectionState = status === "done" ? "connected" : status === "error" ? "error" : stage <= 1 ? "compiling" : "flashing";
  return (
    <div
      className={cx("fg-build", `is-${status}`, className)}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(total * 100)}
      aria-label={title ?? "Build"}
    >
      <div className="fg-build-top">
        <StatusDot state={state} label={false} />
        <div className="fg-build-title">
          {title ?? "Flashe Sketch"}
          {target ? <span>{` → ${target}`}</span> : null}
        </div>
        <span className="fg-build-pct">{status === "error" ? "Abbruch" : `${Math.round(total * 100)} %`}</span>
      </div>
      <div className="fg-build-track">
        {stages.map((s, i) => {
          const done = status === "done" || i < stage;
          const active = status === "running" && i === stage;
          const err = status === "error" && i === stage;
          const w = done ? 100 : active || err ? progress * 100 : 0;
          return (
            <div key={s} className={cx("fg-stage", done && "is-done", active && "is-active", err && "is-error")}>
              <div className="fg-stage-bar">
                <span className="fg-stage-fill" style={{ width: `${w}%` }} />
              </div>
              <div className="fg-stage-label">
                {done ? <Icon name="check" size={12} /> : err ? <Icon name="error" size={12} /> : null}
                {s}
              </div>
            </div>
          );
        })}
      </div>
      <div className="fg-build-current">
        {status === "done"
          ? "Abgeschlossen"
          : status === "error"
            ? `Abbruch in Schritt ${stage + 1}: ${stages[stage] ?? ""}`
            : `Schritt ${Math.min(stage + 1, stages.length)} von ${stages.length}: ${stages[Math.min(stage, stages.length - 1)]}`}
      </div>
      {detail ? <div className="fg-build-detail">{detail}</div> : null}
    </div>
  );
}

/* ---------- Pinout (ESP32-S3-DevKitC-1) ---------- */
type Pin = number | string;
const S3_LEFT: Pin[] = ["3V3", "3V3", "RST", 4, 5, 6, 7, 15, 16, 17, 18, 8, 3, 46, 9, 10, 11, 12, 13, 14, "5V", "GND"];
const S3_RIGHT: Pin[] = ["GND", 43, 44, 1, 2, 42, 41, 40, 39, 38, 37, 36, 35, 0, 45, 48, 47, 21, 20, 19, "GND", "GND"];

function pinCaps(g: number): string[] {
  const c: string[] = [];
  if (g >= 1 && g <= 10) c.push(`ADC1_CH${g - 1}`);
  if (g >= 11 && g <= 20) c.push(`ADC2_CH${g - 11}`);
  if (g >= 1 && g <= 14) c.push(`TOUCH${g}`);
  if (g === 19) c.push("USB D−");
  if (g === 20) c.push("USB D+");
  if (g === 43) c.push("U0TXD");
  if (g === 44) c.push("U0RXD");
  if ([0, 3, 45, 46].includes(g)) c.push("Strapping-Pin");
  if ([35, 36, 37].includes(g)) c.push("Octal-PSRAM (nicht frei)");
  if (g === 48) c.push("RGB-LED");
  return c;
}

export interface PinoutProps {
  /** GPIO-Nummer zu Funktionsname, z. B. `{ 4: "SENSOR" }`. */
  used?: Record<number, string>;
  highlight?: number;
  ledOn?: boolean;
  legend?: boolean;
  boardLabel?: string;
  className?: string;
}

export function Pinout({ used = {}, highlight, ledOn, legend = true, boardLabel, className }: PinoutProps) {
  const [hot, setHot] = useState<number | null>(highlight ?? null);
  const kind = (x: Pin) => (x === "GND" ? "gnd" : x === "3V3" || x === "5V" ? "power" : x === "RST" ? "ctl" : "gpio");
  const label = (x: Pin) => (typeof x === "number" ? (x === 43 ? "TX 43" : x === 44 ? "RX 44" : String(x)) : x);

  const pin = (x: Pin, side: "l" | "r", row: number) => {
    const k = kind(x);
    const isGpio = typeof x === "number";
    const u = isGpio ? used[x] : undefined;
    const isHot = isGpio && hot === x;
    const cells = [
      <span key="n" className="fg-pin-num">
        {label(x)}
      </span>,
      u ? (
        <span key="f" className="fg-pin-fn">
          {u}
        </span>
      ) : null,
    ];
    if (side === "l") cells.reverse();
    return (
      <Fragment key={`${side}${row}`}>
        <div
          tabIndex={isGpio ? 0 : -1}
          className={cx("fg-pin", `is-${k}`, side === "l" && "is-left", isHot && "is-hot")}
          style={{ gridRow: row + 1, gridColumn: side === "l" ? 1 : 5 }}
          onMouseEnter={() => isGpio && setHot(x)}
          onFocus={() => isGpio && setHot(x)}
        >
          {cells}
        </div>
        <span
          className={cx("fg-pad", `is-${k}`, u && "is-used", isHot && "is-hot")}
          style={{ gridRow: row + 1, gridColumn: side === "l" ? 2 : 4 }}
        />
      </Fragment>
    );
  };

  return (
    <div className={cx("fg-pinout", className)}>
      <div className="fg-pin-grid">
        {S3_LEFT.map((x, i) => pin(x, "l", i))}
        <div className="fg-board" aria-hidden>
          <div className="fg-board-mod">
            <b>ESP32-S3</b>
            <span>
              WROOM-1
              <br />
              N16R8
            </span>
          </div>
          <span className="fg-board-antenna" />
          <span className={cx("fg-board-led", ledOn && "is-on")} />
          <span className="fg-board-name">{boardLabel ?? "DevKitC-1"}</span>
          <span className="fg-board-usb" style={{ left: 18 }} />
          <span className="fg-board-usb" style={{ right: 18 }} />
        </div>
        {S3_RIGHT.map((x, i) => pin(x, "r", i))}
      </div>
      <div className="fg-pin-info" aria-live="polite">
        {hot != null ? (
          <>
            <strong>{`GPIO ${hot}`}</strong>
            {pinCaps(hot).map((c) => (
              <Badge key={c} mono>
                {c}
              </Badge>
            ))}
            {used[hot] ? (
              <Badge tone="glut" mono>{`belegt: ${used[hot]}`}</Badge>
            ) : (
              <span>frei</span>
            )}
          </>
        ) : (
          <span>Pin wählen, um Funktionen zu sehen.</span>
        )}
      </div>
      {legend ? (
        <div className="fg-pin-legend">
          <span><i className="fg-pad is-used" />im Sketch belegt</span>
          <span><i className="fg-pad" />GPIO</span>
          <span><i className="fg-pad is-power" />Versorgung</span>
          <span><i className="fg-pad is-gnd" />GND</span>
        </div>
      ) : null}
    </div>
  );
}

/* ---------- Memory Meter ---------- */
export interface MemoryItem {
  label: string;
  used: number;
  total: number;
  note?: string;
}

export interface MemoryMeterProps {
  items: MemoryItem[];
  large?: boolean;
  warnAt?: number;
  errorAt?: number;
  className?: string;
}

export function MemoryMeter({ items, large, warnAt = 0.8, errorAt = 0.95, className }: MemoryMeterProps) {
  return (
    <div className={cx("fg-mem", large && "is-large", className)}>
      {items.map((it) => {
        const f = Math.min(1, it.used / it.total);
        const lvl = f >= errorAt ? "error" : f >= warnAt ? "warning" : null;
        return (
          <div key={it.label} role="meter" aria-valuemin={0} aria-valuemax={it.total} aria-valuenow={it.used} aria-label={it.label}>
            <div className="fg-mem-head">
              <span className="fg-mem-label">{it.label}</span>
              <span className="fg-mem-val">
                <b>{formatBytes(it.used)}</b>
                {` / ${formatBytes(it.total)}`}
              </span>
              <span className="fg-mem-pct">{`${Math.round(f * 100)}%`}</span>
            </div>
            <div className="fg-meter">
              <span className={cx("fg-meter-fill", lvl && `is-${lvl}`)} style={{ width: `${f * 100}%` }} />
            </div>
            {lvl ? (
              <div className={`fg-mem-note is-${lvl}`}>
                <Icon name={lvl === "error" ? "error" : "warning"} size={12} />
                {it.note ?? (lvl === "error" ? "Kaum noch Platz. Größeres Partitionsschema wählen." : "Wird knapp.")}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

/* ---------- Partition Map ---------- */
export interface Partition {
  name: string;
  type: string;
  offset: number;
  size: number;
  used?: number;
  color?: 1 | 2 | 3 | 4 | 5;
}

export interface PartitionMapProps {
  partitions: Partition[];
  flashSize?: number;
  className?: string;
}

const hex = (n: number) => `0x${n.toString(16).toUpperCase().padStart(6, "0")}`;

export function PartitionMap({ partitions, flashSize, className }: PartitionMapProps) {
  const end = partitions.reduce((a, x) => Math.max(a, x.offset + x.size), 0);
  const total = flashSize ?? end;
  const free = total - end;
  const rows: Array<Partition & { free?: boolean }> = [
    ...partitions,
    ...(free > 0 ? [{ name: "frei", type: "", offset: end, size: free, free: true }] : []),
  ];
  const color = (x: { free?: boolean; color?: number }, i: number) => (x.free ? 6 : (x.color ?? (i % 5) + 1));

  return (
    <div className={cx("fg-parts", className)}>
      <div className="fg-parts-bar" role="img" aria-label="Partitionstabelle">
        {rows.map((x, i) => (
          <span key={x.name} className={`fg-part c-${color(x, i)}`} style={{ flex: `${x.size} 0 0` }} title={`${x.name}: ${formatBytes(x.size)}`}>
            {x.used ? <span className="fg-part-used" style={{ width: `${Math.min(100, (x.used / x.size) * 100)}%` }} /> : null}
          </span>
        ))}
      </div>
      <table className="fg-parts-table">
        <tbody>
          {rows.map((x, i) => (
            <tr key={x.name}>
              <td className="n">
                <i className={`fg-sw c-${color(x, i)}`} />
                {x.name}
              </td>
              <td>{x.type}</td>
              <td className="m">{hex(x.offset)}</td>
              <td className="m">
                {x.used ? `${formatBytes(x.used)} / ` : ""}
                {formatBytes(x.size)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/* ---------- Library Item ---------- */
export interface LibraryItemProps {
  name: string;
  version: string;
  latest?: string;
  author?: string;
  downloads?: string;
  description?: string;
  state?: "available" | "installing" | "installed" | "update";
  onInstall?(): void;
  onUpdate?(): void;
  onRemove?(): void;
  /** Eine Spalte, für schmale Seitenleisten. */
  compact?: boolean;
  /** Fortschritt einer laufenden Installation (0 bis 1). */
  progress?: number | null;
  className?: string;
}

export function LibraryItem({ name, version, latest, author, downloads, description, state = "available", onInstall, onUpdate, onRemove, compact, progress, className }: LibraryItemProps) {
  return (
    <div className={cx("fg-lib", compact && "fg-lib--compact", className)}>
      <div className="fg-lib-name">
        <span>{name}</span>
        <Badge mono>{version}</Badge>
        {state === "installed" ? <Badge tone="success" icon="check">Installiert</Badge> : null}
        {state === "update" ? (
          <Badge tone="info" icon="arrow-down">
            {latest ? `${latest} verfügbar` : "Update"}
          </Badge>
        ) : null}
      </div>
      <div className="fg-lib-meta">
        {author}
        {downloads ? (
          <span>
            {" · "}
            <span className="fg-mono">{downloads}</span> Installationen
          </span>
        ) : null}
      </div>
      {description ? <div className="fg-lib-desc">{description}</div> : null}
      <div className="fg-lib-act">
        {state === "available" ? (
          <Button size="sm" icon="plus" onClick={onInstall}>
            Installieren
          </Button>
        ) : null}
        {state === "installing" ? (
          <Button size="sm" busy disabled>
            Installiere …
          </Button>
        ) : null}
        {state === "update" ? (
          <Button size="sm" variant="primary" icon="arrow-down" onClick={onUpdate}>
            Aktualisieren
          </Button>
        ) : null}
        {state === "installed" ? (
          <Button size="sm" variant="ghost" icon="trash" onClick={onRemove}>
            Entfernen
          </Button>
        ) : null}
      </div>
      {state === "installing" ? (
        <div className="fg-lib-progress" role="progressbar" aria-label={`${name} wird installiert`}>
          <i style={progress != null ? { width: `${Math.round(progress * 100)}%` } : undefined} className={progress == null ? "is-indeterminate" : undefined} />
        </div>
      ) : null}
    </div>
  );
}
