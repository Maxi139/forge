import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { cx } from "../util";

export interface SplitterProps {
  /** `vertical` ist eine senkrechte Linie, die die Breite ändert. */
  orientation: "vertical" | "horizontal";
  value: number;
  min: number;
  max: number;
  /** Wird beim Ziehen laufend aufgerufen. */
  onChange(value: number): void;
  /** Wird am Ende des Ziehens aufgerufen, zum Speichern. */
  onCommit?(value: number): void;
  /** Standardwert für Doppelklick. */
  reset?: number;
  /** Bei `true` wächst der Wert, wenn der Zeiger nach links bzw. oben geht. */
  invert?: boolean;
  label: string;
}

export function Splitter({ orientation, value, min, max, onChange, onCommit, reset, invert, label }: SplitterProps) {
  const start = useRef<{ pointer: number; value: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const vertical = orientation === "vertical";
  const clamp = (v: number) => Math.min(max, Math.max(min, Math.round(v)));

  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    start.current = { pointer: vertical ? e.clientX : e.clientY, value };
    setDragging(true);
    document.body.style.userSelect = "none";
    document.body.style.cursor = vertical ? "col-resize" : "row-resize";
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const delta = (vertical ? e.clientX : e.clientY) - start.current.pointer;
    onChange(clamp(start.current.value + (invert ? -delta : delta)));
  };
  const up = (e: PointerEvent<HTMLDivElement>) => {
    if (!start.current) return;
    const delta = (vertical ? e.clientX : e.clientY) - start.current.pointer;
    const next = clamp(start.current.value + (invert ? -delta : delta));
    start.current = null;
    setDragging(false);
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    onChange(next);
    onCommit?.(next);
  };
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.shiftKey ? 64 : 16;
    const grow = vertical ? "ArrowRight" : "ArrowDown";
    const shrink = vertical ? "ArrowLeft" : "ArrowUp";
    let next: number | null = null;
    if (e.key === grow) next = value + (invert ? -step : step);
    else if (e.key === shrink) next = value + (invert ? step : -step);
    else if (e.key === "Home") next = min;
    else if (e.key === "End") next = max;
    if (next == null) return;
    e.preventDefault();
    const v = clamp(next);
    onChange(v);
    onCommit?.(v);
  };

  return (
    <div
      role="separator"
      aria-orientation={vertical ? "vertical" : "horizontal"}
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      className={cx("fg-split", vertical ? "fg-split--v" : "fg-split--h", dragging && "is-dragging")}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
      onKeyDown={key}
      onDoubleClick={() => {
        if (reset == null) return;
        onChange(reset);
        onCommit?.(reset);
      }}
    />
  );
}
