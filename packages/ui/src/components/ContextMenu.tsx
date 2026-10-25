import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { IconName } from "../icons";
import { cx } from "../util";
import { Icon } from "./Icon";

export interface MenuItem {
  id: string;
  label: string;
  icon?: IconName;
  danger?: boolean;
  disabled?: boolean;
  separatorBefore?: boolean;
  shortcut?: string;
}

export interface ContextMenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  onSelect(id: string): void;
  onClose(): void;
  children?: ReactNode;
}

export function ContextMenu({ x, y, items, onSelect, onClose }: ContextMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  const [active, setActive] = useState(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(8, Math.min(x, window.innerWidth - r.width - 8)),
      y: Math.max(8, Math.min(y, window.innerHeight - r.height - 8)),
    });
    el.focus();
  }, [x, y]);

  useEffect(() => {
    const away = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", away, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", away, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  const move = (d: number) => {
    let i = active;
    for (let n = 0; n < items.length; n++) {
      i = (i + d + items.length) % items.length;
      if (!items[i].disabled) break;
    }
    setActive(i);
  };

  return (
    <div
      ref={ref}
      className="fg-menu"
      role="menu"
      tabIndex={-1}
      style={{ left: pos.x, top: pos.y }}
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
        else if (e.key === "ArrowDown") (e.preventDefault(), move(1));
        else if (e.key === "ArrowUp") (e.preventDefault(), move(-1));
        else if (e.key === "Enter" && !items[active]?.disabled) onSelect(items[active].id);
      }}
      onContextMenu={(e) => e.preventDefault()}
    >
      {items.map((it, i) => (
        <div key={it.id}>
          {it.separatorBefore ? <div className="fg-menu-sep" role="separator" /> : null}
          <div
            role="menuitem"
            aria-disabled={it.disabled || undefined}
            className={cx("fg-menu-item", i === active && "is-active", it.danger && "is-danger", it.disabled && "is-disabled")}
            onMouseMove={() => setActive(i)}
            onClick={() => !it.disabled && onSelect(it.id)}
          >
            {it.icon ? <Icon name={it.icon} size={14} /> : <span className="fg-menu-gap" />}
            <span className="fg-menu-label">{it.label}</span>
            {it.shortcut ? <span className="fg-menu-kbd">{it.shortcut}</span> : null}
          </div>
        </div>
      ))}
    </div>
  );
}
