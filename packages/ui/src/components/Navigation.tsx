import {
  useMemo,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import type { IconName } from "../icons";
import { cx } from "../util";
import { Button, Kbd, StatusDot, type ConnectionState } from "./Controls";
import { Icon } from "./Icon";

/* ---------- Tabs ---------- */
export interface TabItem {
  id: string;
  label: string;
  icon?: IconName;
  dirty?: boolean;
  problems?: boolean;
  count?: number;
  title?: string;
}

export interface TabsProps {
  items: TabItem[];
  variant?: "editor" | "panel";
  value?: string;
  defaultValue?: string;
  onChange?(id: string): void;
  onClose?(id: string): void;
  actions?: ReactNode;
  className?: string;
}

export function Tabs({ items, variant = "editor", value, defaultValue, onChange, onClose, actions, className }: TabsProps) {
  const [inner, setInner] = useState(defaultValue ?? items[0]?.id);
  const current = value ?? inner;

  const select = (id: string) => {
    if (value == null) setInner(id);
    onChange?.(id);
  };

  const onKey = (e: KeyboardEvent<HTMLDivElement>, i: number) => {
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const n = (i + d + items.length) % items.length;
    select(items[n].id);
    (e.currentTarget.parentElement?.children[n] as HTMLElement | undefined)?.focus();
  };

  return (
    <div className={cx("fg-tabs", `fg-tabs--${variant}`, className)} role="tablist">
      {items.map((t, i) => {
        const active = t.id === current;
        return (
          <div
            key={t.id}
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            title={t.title}
            className={cx("fg-tab", active && "is-active", t.dirty && "is-dirty", t.problems && "has-problems")}
            onClick={() => select(t.id)}
            onAuxClick={(e) => {
              if (e.button === 1 && variant === "editor") onClose?.(t.id);
            }}
            onKeyDown={(e) => onKey(e, i)}
          >
            {t.icon ? <Icon name={t.icon} size={14} /> : null}
            <span className="fg-tab-label">{t.label}</span>
            {t.count != null ? <span className="fg-tab-count">{t.count}</span> : null}
            {variant === "editor" ? (
              <button
                className="fg-tab-close"
                tabIndex={-1}
                aria-label={`${t.label} schließen`}
                onClick={(e) => {
                  e.stopPropagation();
                  onClose?.(t.id);
                }}
              >
                <span className="fg-tab-dirty" />
                <Icon name="close" size={12} />
              </button>
            ) : null}
          </div>
        );
      })}
      {actions ? <div className="fg-tabs-actions">{actions}</div> : null}
    </div>
  );
}

/* ---------- Panel ---------- */
export interface PanelProps {
  title?: ReactNode;
  tabs?: ReactNode;
  actions?: ReactNode;
  padded?: boolean;
  className?: string;
  style?: CSSProperties;
  children?: ReactNode;
}

export function Panel({ title, tabs, actions, padded, className, style, children }: PanelProps) {
  const head = title != null || actions || tabs;
  return (
    <section className={cx("fg-panel", className)} style={style} aria-label={typeof title === "string" ? title : undefined}>
      {head ? (
        <header className="fg-panel-head">
          {tabs ?? <span className="fg-overline">{title}</span>}
          <div className="fg-panel-actions">{actions}</div>
        </header>
      ) : null}
      <div className={cx("fg-panel-body", padded && "is-padded")}>{children}</div>
    </section>
  );
}

/* ---------- Activity Bar ---------- */
export interface ActivityItem {
  id: string;
  icon: IconName;
  label: string;
  badge?: ReactNode;
}

export interface ActivityBarProps {
  items: ActivityItem[];
  bottom?: ActivityItem[];
  value?: string;
  onChange?(id: string): void;
}

export function ActivityBar({ items, bottom = [], value, onChange }: ActivityBarProps) {
  const render = (it: ActivityItem) => (
    <button
      key={it.id}
      className={cx("fg-act", it.id === value && "is-active")}
      title={it.label}
      aria-label={it.label}
      aria-pressed={it.id === value}
      onClick={() => onChange?.(it.id)}
    >
      <Icon name={it.icon} size={18} />
      {it.badge ? <span className="fg-act-badge">{it.badge}</span> : null}
    </button>
  );
  return (
    <nav className="fg-activity" aria-label="Ansichten">
      {items.map(render)}
      <div className="fg-activity-spacer" />
      {bottom.map(render)}
    </nav>
  );
}

/* ---------- File Tree ---------- */
export interface TreeNode {
  name: string;
  children?: TreeNode[];
  open?: boolean;
  icon?: IconName;
  status?: "modified" | "error";
  badge?: ReactNode;
}

export interface FileTreeProps {
  nodes: TreeNode[];
  defaultSelected?: string;
  selected?: string;
  onSelect?(path: string, node: TreeNode): void;
  onContextMenu?(path: string, node: TreeNode, e: React.MouseEvent): void;
}

export function FileTree({ nodes, defaultSelected, selected, onSelect, onContextMenu }: FileTreeProps) {
  const initial = useMemo(() => {
    const m: Record<string, boolean> = {};
    const walk = (ns: TreeNode[], base: string) =>
      ns.forEach((n) => {
        const path = `${base}/${n.name}`;
        if (n.open) m[path] = true;
        if (n.children) walk(n.children, path);
      });
    walk(nodes, "");
    return m;
    // Nur der Startzustand zählt; danach gehört er dem Nutzer.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  const [open, setOpen] = useState(initial);
  const [inner, setInner] = useState(defaultSelected);
  const sel = selected !== undefined ? selected : inner;

  const rows = (ns: TreeNode[], depth: number, base: string): ReactNode[] =>
    ns.flatMap((n) => {
      const path = `${base}/${n.name}`;
      const isFolder = !!n.children;
      const isOpen = !!open[path];
      const act = () => {
        if (isFolder) setOpen({ ...open, [path]: !isOpen });
        else {
          setInner(path);
          onSelect?.(path, n);
        }
      };
      const row = (
        <div
          key={path}
          role="treeitem"
          tabIndex={0}
          aria-expanded={isFolder ? isOpen : undefined}
          aria-selected={sel === path}
          className={cx("fg-tree-row", sel === path && "is-selected", n.status && `is-${n.status}`)}
          style={{ paddingLeft: 8 + depth * 12 }}
          onClick={act}
          onContextMenu={(e) => onContextMenu?.(path, n, e)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              act();
            }
          }}
        >
          <span className="fg-tree-chev">
            {isFolder ? <Icon name={isOpen ? "chevron-down" : "chevron-right"} size={12} /> : null}
          </span>
          <Icon
            name={isFolder ? (isOpen ? "folder-open" : "folder") : (n.icon ?? "file")}
            size={14}
            className="fg-tree-icon"
          />
          <span className="fg-tree-name">{n.name}</span>
          {n.badge != null ? <span className="fg-tree-badge">{n.badge}</span> : null}
        </div>
      );
      return isFolder && isOpen ? [row, ...rows(n.children!, depth + 1, path)] : [row];
    });

  return (
    <div className="fg-tree" role="tree">
      {rows(nodes, 0, "")}
    </div>
  );
}

/* ---------- Sidebar ---------- */
const DEFAULT_ACTIVITY: ActivityItem[] = [
  { id: "files", icon: "files", label: "Explorer" },
  { id: "search", icon: "search", label: "Suche" },
  { id: "library", icon: "library", label: "Bibliotheken" },
  { id: "device", icon: "device", label: "Gerät" },
  { id: "plot", icon: "plot", label: "Plotter" },
];

export interface SidebarProps {
  nodes?: TreeNode[];
  activity?: ActivityItem[];
  activityBottom?: ActivityItem[];
  view?: string;
  defaultView?: string;
  onViewChange?(id: string): void;
  title?: string;
  actions?: ReactNode;
  defaultSelected?: string;
  onSelect?(path: string, node: TreeNode): void;
  collapsed?: boolean;
  className?: string;
  children?: ReactNode;
}

export function Sidebar(p: SidebarProps) {
  const [inner, setInner] = useState(p.defaultView ?? "files");
  const view = p.view ?? inner;
  return (
    <div className={cx("fg-sidebar", p.className)}>
      <ActivityBar
        items={p.activity ?? DEFAULT_ACTIVITY}
        bottom={p.activityBottom ?? [{ id: "settings", icon: "settings", label: "Einstellungen" }]}
        value={view}
        onChange={(id) => {
          setInner(id);
          p.onViewChange?.(id);
        }}
      />
      {p.collapsed ? null : (
        <Panel title={p.title ?? "Explorer"} actions={p.actions}>
          {p.children ??
            (p.nodes ? <FileTree nodes={p.nodes} defaultSelected={p.defaultSelected} onSelect={p.onSelect} /> : null)}
        </Panel>
      )}
    </div>
  );
}

/* ---------- StatusBar ---------- */
export interface StatusItemProps {
  icon?: IconName;
  onClick?(): void;
  title?: string;
  className?: string;
  children?: ReactNode;
}

export function StatusItem({ icon, onClick, title, className, children }: StatusItemProps) {
  const Tag = onClick ? "button" : "span";
  return (
    <Tag className={cx("fg-status-item", className)} onClick={onClick} title={title}>
      {icon ? <Icon name={icon} size={12} /> : null}
      {children}
    </Tag>
  );
}

export interface StatusBarProps {
  left?: ReactNode;
  right?: ReactNode;
  progress?: number;
  className?: string;
}

export function StatusBar({ left, right, progress, className }: StatusBarProps) {
  return (
    <footer className={cx("fg-statusbar", className)}>
      {progress != null ? <span className="fg-statusbar-progress" style={{ width: `${Math.round(progress * 100)}%` }} /> : null}
      <div className="fg-statusbar-group">{left}</div>
      <div className="fg-statusbar-group">{right}</div>
    </footer>
  );
}

/* ---------- TitleBar ---------- */
export interface TitleBarProps {
  project?: string;
  /** Mit diesem Handler lässt sich der Projektname direkt in der Titelleiste ändern. */
  onProjectRename?(name: string): void;
  board?: string;
  port?: string;
  connection?: ConnectionState;
  phase?: "idle" | "compiling" | "flashing";
  searchLabel?: string;
  /** Gezeichnete Fensterknöpfe (nur für Vorschauen). */
  windowControls?: boolean;
  /** Platz lassen für die echten macOS-Knöpfe. */
  nativeInset?: boolean;
  /** Sperrt den Flash-Button, z. B. solange kein Board gewählt ist. */
  flashDisabled?: boolean;
  /** Sperrt den Kompilieren-Button, z. B. solange kein Projekt offen ist. */
  buildDisabled?: boolean;
  /** Eine laufende Aktion lässt sich abbrechen: dann wird aus dem Flash-Button „Abbrechen“. */
  onCancel?(): void;
  onSearch?(): void;
  onBoard?(anchor: DOMRect): void;
  onPort?(anchor: DOMRect): void;
  onBuild?(): void;
  onFlash?(): void;
  className?: string;
}

/** Der Projektname als Text, der sich anklicken und ändern lässt. Enter oder Verlassen übernimmt, Esc verwirft. */
function ProjectName(p: { value: string; onRename(name: string): void }) {
  const [draft, setDraft] = useState<string | null>(null);
  const shown = draft ?? p.value;
  const finish = (commit: boolean) => {
    const next = draft?.trim();
    setDraft(null);
    if (commit && next && next !== p.value) p.onRename(next);
  };
  return (
    <input
      id="project-name-input"
      className="fg-titlebar-name"
      aria-label="Projektname"
      title="Zum Umbenennen klicken"
      value={shown}
      size={Math.max(shown.length, 3)}
      spellCheck={false}
      autoComplete="off"
      onFocus={(e) => e.currentTarget.select()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => finish(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          e.currentTarget.blur();
        } else if (e.key === "Escape") {
          e.preventDefault();
          setDraft(null);
          e.currentTarget.blur();
        }
      }}
    />
  );
}

export function TitleBar(p: TitleBarProps) {
  const phase = p.phase ?? "idle";
  const drag = { "data-tauri-drag-region": "" } as const;
  return (
    <header className={cx("fg-titlebar", p.className)} {...drag}>
      {p.nativeInset ? <span className="fg-titlebar-inset" {...drag} /> : null}
      {p.windowControls === false || p.nativeInset ? null : (
        <span className="fg-titlebar-lights" aria-hidden>
          <i />
          <i />
          <i />
        </span>
      )}
      <span className="fg-wordmark" {...drag}>
        <b />
        forge
      </span>
      <span className="fg-titlebar-project" {...drag}>
        /
        {p.onProjectRename && p.project ? (
          <ProjectName value={p.project} onRename={p.onProjectRename} />
        ) : (
          <strong>{p.project ?? "sketch"}</strong>
        )}
      </span>
      <div className="fg-titlebar-center" {...drag}>
        <button className="fg-titlebar-search" onClick={p.onSearch} type="button">
          <Icon name="search" size={14} />
          <span>{p.searchLabel ?? "Befehle, Dateien, Boards …"}</span>
          <Kbd keys={["⌘", "K"]} />
        </button>
      </div>
      <div className="fg-titlebar-group">
        <div className="fg-target">
          <button type="button" onClick={(e) => p.onBoard?.(e.currentTarget.getBoundingClientRect())} title="Board wählen">
            <Icon name="chip" size={14} />
            {p.board ?? "Board wählen"}
            <Icon name="chevron-down" size={12} />
          </button>
          <button type="button" onClick={(e) => p.onPort?.(e.currentTarget.getBoundingClientRect())} title="Port wählen">
            <StatusDot state={p.connection ?? "idle"} label={false} />
            <span className="fg-mono">{p.port ?? "Kein Port"}</span>
          </button>
        </div>
        {phase !== "idle" && p.onCancel ? (
          <Button variant="ghost" icon="stop" aria-label="Abbrechen" title="Abbrechen" onClick={p.onCancel} />
        ) : (
          <Button variant="ghost" icon="build" aria-label="Kompilieren" title="Kompilieren  ⌘B" onClick={p.onBuild} disabled={p.buildDisabled} />
        )}
        <Button
          variant="primary"
          icon="flash"
          busy={phase === "flashing"}
          onClick={p.onFlash}
          shortcut={phase === "flashing" ? undefined : "⌘U"}
          disabled={p.flashDisabled}
        >
          {phase === "flashing" ? "Flashe …" : "Flash"}
        </Button>
      </div>
    </header>
  );
}
