import { useEffect, useId, useMemo, useState, type KeyboardEvent, type ReactNode } from "react";
import type { IconName } from "../icons";
import { cx, fuzzy, marks } from "../util";
import { Kbd } from "./Controls";
import { Icon } from "./Icon";

/* ---------- Command Palette ---------- */
export interface Command {
  id: string;
  label: string;
  group?: string;
  icon?: IconName;
  hint?: string;
  shortcut?: string[];
  /** Nur sichtbar, solange nichts eingegeben ist (z. B. die Gruppe „Zuletzt“). */
  emptyOnly?: boolean;
  /** Nur sichtbar, sobald etwas eingegeben ist (z. B. Dateien in der Befehlspalette). */
  searchOnly?: boolean;
  /** Zusätzlicher Suchtext neben dem Namen, etwa der Ordner einer Datei. */
  search?: string;
}

export interface CommandPaletteProps {
  items: Command[];
  onRun?(item: Command): void;
  onClose?(): void;
  placeholder?: string;
  defaultQuery?: string;
  autoFocus?: boolean;
  className?: string;
}

interface Ranked {
  it: Command;
  idx: number[];
  score: number;
  i: number;
}

export function CommandPalette({ items, onRun, onClose, placeholder, defaultQuery = "", autoFocus, className }: CommandPaletteProps) {
  const [q, setQ] = useState(defaultQuery);
  const [active, setActive] = useState(0);
  const listId = useId();

  const list = useMemo<Ranked[]>(() => {
    const ranked = items
      .filter((it) => (q ? !it.emptyOnly : !it.searchOnly))
      .map((it) => {
        const m = fuzzy(q, it.label);
        if (m) return { it, idx: m.idx, score: m.score, i: 0 };
        // Trifft nur der Zusatztext, zählt der Treffer weniger und es gibt nichts zu markieren.
        const alt = it.search ? fuzzy(q, it.search) : null;
        return alt ? { it, idx: [], score: alt.score - 10, i: 0 } : null;
      })
      .filter((x): x is Ranked => x !== null);
    if (q) ranked.sort((a, b) => b.score - a.score);
    return ranked.map((r, i) => ({ ...r, i }));
  }, [q, items]);

  const groups = useMemo(() => {
    const out: Array<{ name: string; rows: Ranked[] }> = [];
    for (const r of list) {
      const name = q ? "Treffer" : (r.it.group ?? "");
      const last = out[out.length - 1];
      if (!last || last.name !== name) out.push({ name, rows: [r] });
      else last.rows.push(r);
    }
    return out;
  }, [list, q]);

  const run = (r?: Ranked) => {
    if (r) onRun?.(r.it);
  };

  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive(Math.min(active + 1, list.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive(Math.max(active - 1, 0));
    } else if (e.key === "Enter") run(list[active]);
    else if (e.key === "Escape") onClose?.();
  };

  useEffect(() => {
    document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" });
  }, [active, listId]);

  return (
    <div className={cx("fg-palette", className)} role="dialog" aria-label="Kommandopalette">
      <div className="fg-palette-input">
        <Icon name="command" />
        <input
          autoFocus={autoFocus}
          value={q}
          placeholder={placeholder ?? "Befehl, Datei oder Board suchen …"}
          aria-label="Suche"
          aria-controls={listId}
          aria-activedescendant={list[active] ? `${listId}-${active}` : undefined}
          onChange={(e) => {
            setQ(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKey}
        />
        <Kbd keys={["esc"]} />
      </div>
      <div className="fg-palette-list" role="listbox" id={listId}>
        {list.length ? (
          groups.map((g) => (
            <div key={g.name || "_"}>
              {g.name ? <div className="fg-palette-glabel">{g.name}</div> : null}
              {g.rows.map((r) => {
                const on = r.i === active;
                return (
                  <div
                    key={r.it.id}
                    id={`${listId}-${r.i}`}
                    role="option"
                    aria-selected={on}
                    className={cx("fg-palette-item", on && "is-active")}
                    onMouseMove={() => {
                      if (!on) setActive(r.i);
                    }}
                    onClick={() => run(r)}
                  >
                    <Icon name={r.it.icon ?? "chevron-right"} />
                    <span className="fg-palette-label">{marks(r.it.label, r.idx)}</span>
                    {r.it.hint ? <span className="fg-palette-hint">{r.it.hint}</span> : null}
                    {r.it.shortcut ? <Kbd keys={r.it.shortcut} /> : null}
                  </div>
                );
              })}
            </div>
          ))
        ) : (
          <div className="fg-palette-empty">{`Nichts gefunden für „${q}“`}</div>
        )}
      </div>
      <div className="fg-palette-foot">
        <span>
          <Kbd keys={["↑", "↓"]} />
          auswählen
        </span>
        <span>
          <Kbd keys={["↵"]} />
          ausführen
        </span>
      </div>
    </div>
  );
}

/* ---------- Toast ---------- */
const KIND_ICON = {
  success: "check-circle",
  error: "error",
  warning: "warning",
  info: "info",
  progress: "flash",
} as const satisfies Record<string, IconName>;

export interface ToastProps {
  kind?: keyof typeof KIND_ICON;
  title: ReactNode;
  message?: ReactNode;
  actions?: ReactNode;
  onClose?(): void;
  progress?: number;
  icon?: IconName;
  className?: string;
}

export function Toast({ kind = "info", title, message, actions, onClose, progress, icon, className }: ToastProps) {
  return (
    <div className={cx("fg-toast", `fg-toast--${kind}`, className)} role={kind === "error" ? "alert" : "status"}>
      <span className="fg-toast-icon">
        <Icon name={icon ?? KIND_ICON[kind]} />
      </span>
      <div>
        <div className="fg-toast-title">{title}</div>
        {message ? <div className="fg-toast-msg">{message}</div> : null}
        {actions ? <div className="fg-toast-actions">{actions}</div> : null}
      </div>
      {onClose ? (
        <button className="fg-toast-close" aria-label="Schließen" onClick={onClose}>
          <Icon name="close" size={12} />
        </button>
      ) : (
        <span />
      )}
      {progress != null ? <span className="fg-toast-progress" style={{ width: `${progress * 100}%` }} /> : null}
    </div>
  );
}

export function ToastStack({ children }: { children?: ReactNode }) {
  return (
    <div className="fg-toasts" aria-live="polite">
      {children}
    </div>
  );
}

/* ---------- Dialog ---------- */
export interface DialogProps {
  title: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  icon?: IconName;
  tone?: "glut" | "error" | "warning";
  onClose?(): void;
  inline?: boolean;
  width?: number;
  className?: string;
}

export function Dialog({ title, children, footer, icon, tone, onClose, inline, width, className }: DialogProps) {
  const titleId = useId();
  useEffect(() => {
    if (inline || !onClose) return;
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [inline, onClose]);

  const box = (
    <div
      className={cx("fg-dialog", className)}
      role="dialog"
      aria-modal={inline ? undefined : true}
      aria-labelledby={titleId}
      style={width ? { width } : undefined}
    >
      <div className="fg-dialog-head">
        {icon ? (
          <span className={cx("fg-dialog-icon", tone && `is-${tone}`)}>
            <Icon name={icon} />
          </span>
        ) : null}
        <h2 className="fg-dialog-title" id={titleId}>
          {title}
        </h2>
      </div>
      <div className="fg-dialog-body">{children}</div>
      {footer ? <div className="fg-dialog-foot">{footer}</div> : null}
    </div>
  );
  if (inline) return box;
  return (
    <div
      className="fg-scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose?.();
      }}
    >
      {box}
    </div>
  );
}
