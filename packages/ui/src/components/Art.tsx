import { useMemo, type ReactNode } from "react";
import type { IconName } from "../icons";
import { cx } from "../util";
import { Kbd } from "./Controls";
import { Icon } from "./Icon";

/* ---------- Traces: generative Leiterbahnen ---------- */
function rng(seed: number) {
  let s = seed >>> 0 || 1;
  return () => {
    s ^= s << 13;
    s ^= s >>> 17;
    s ^= s << 5;
    return ((s >>> 0) % 100000) / 100000;
  };
}

const DIRS = [
  [1, 0],
  [1, 1],
  [0, 1],
  [-1, 1],
  [-1, 0],
  [-1, -1],
  [0, -1],
  [1, -1],
];

export interface TracesProps {
  seed?: number;
  width?: number;
  height?: number;
  pitch?: number;
  count?: number;
  lit?: number;
  animate?: boolean;
  raster?: boolean;
  className?: string;
}

export function Traces({ seed = 7, width = 640, height = 360, pitch = 24, count = 16, lit = 3, animate = true, raster = true, className }: TracesProps) {
  const paths = useMemo(() => {
    const r = rng(seed);
    const cols = Math.floor(width / pitch);
    const rows = Math.floor(height / pitch);
    const out: Array<Array<[number, number]>> = [];
    for (let i = 0; i < count; i++) {
      let x = (1 + Math.floor(r() * (cols - 1))) * pitch;
      let y = (1 + Math.floor(r() * (rows - 1))) * pitch;
      const pts: Array<[number, number]> = [[x, y]];
      let dir = Math.floor(r() * 4) * 2;
      const segs = 2 + Math.floor(r() * 3);
      for (let k = 0; k < segs; k++) {
        const d = DIRS[dir];
        const len = (dir % 2 ? 1 + Math.floor(r() * 3) : 2 + Math.floor(r() * 6)) * pitch;
        x += d[0] * len;
        y += d[1] * len;
        pts.push([x, y]);
        dir = (dir + (r() < 0.5 ? 1 : 7)) % 8;
      }
      out.push(pts);
    }
    return out;
  }, [seed, width, height, pitch, count]);

  const dots: ReactNode[] = [];
  if (raster) {
    for (let gx = pitch; gx < width; gx += pitch)
      for (let gy = pitch; gy < height; gy += pitch) dots.push(<circle key={`${gx}-${gy}`} className="dot" cx={gx} cy={gy} r={1.1} />);
  }

  return (
    <svg className={cx("fg-traces", className)} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="xMidYMid slice" aria-hidden>
      {dots}
      {paths.map((pts, i) => {
        const d = pts.map((q) => q.join(",")).join(" ");
        const start = pts[0];
        const end = pts[pts.length - 1];
        return (
          <g key={i}>
            <polyline className="t" points={d} />
            {i < lit && animate ? <polyline className="lit" points={d} pathLength={100} style={{ animationDelay: `${i * 1.1}s` }} /> : null}
            <circle className="pad" cx={start[0]} cy={start[1]} r={3.5} />
            <circle className="pad" cx={end[0]} cy={end[1]} r={3.5} />
          </g>
        );
      })}
    </svg>
  );
}

/* ---------- Empty State ---------- */
export interface EmptyStateProps {
  title: ReactNode;
  text?: ReactNode;
  icon?: IconName;
  actions?: ReactNode;
  hints?: Array<{ keys: string[]; label: string }>;
  seed?: number;
  count?: number;
  lit?: number;
  pattern?: boolean;
  height?: number;
  className?: string;
  /** Zusätzlicher Inhalt unter den Hinweisen, z. B. eine Liste. */
  children?: ReactNode;
}

export function EmptyState({ title, text, icon, actions, hints, seed = 11, count = 22, lit = 2, pattern = true, height, className, children }: EmptyStateProps) {
  return (
    <div className={cx("fg-empty", className)} style={height ? { height, minHeight: 0 } : undefined}>
      {pattern ? (
        <div className="fg-empty-art">
          <Traces seed={seed} count={count} lit={lit} width={960} height={540} />
        </div>
      ) : null}
      <div className="fg-empty-body">
        {icon ? (
          <span className="fg-empty-icon">
            <Icon name={icon} size={20} />
          </span>
        ) : null}
        <h3 className="fg-empty-title">{title}</h3>
        {text ? <p className="fg-empty-text">{text}</p> : null}
        {actions ? <div className="fg-empty-actions">{actions}</div> : null}
        {hints ? (
          <div className="fg-empty-hints">
            {hints.map((x, i) => (
              <span key={i} style={{ display: "contents" }}>
                <Kbd keys={x.keys} />
                <span>{x.label}</span>
              </span>
            ))}
          </div>
        ) : null}
        {children}
      </div>
    </div>
  );
}

/* ---------- Splash ---------- */
export interface SplashProps {
  progress?: number;
  status?: string;
  version?: string;
  seed?: number;
  className?: string;
}

export function Splash({ progress = 0.6, status = "Starte …", version, seed = 3, className }: SplashProps) {
  return (
    <div className={cx("fg-splash", className)} role="img" aria-label="Forge wird gestartet">
      <div className="fg-splash-art">
        <Traces seed={seed} count={14} lit={3} width={560} height={340} />
      </div>
      <div className="fg-splash-body">
        <div className="fg-splash-mark">
          forge
          <b />
        </div>
        <div className="fg-splash-load">
          <i style={{ width: `${Math.round(progress * 100)}%` }} />
        </div>
        <div className="fg-splash-meta">
          <span>{status}</span>
          {version ? <span>{version}</span> : null}
        </div>
      </div>
    </div>
  );
}
