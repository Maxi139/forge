import {
  forwardRef,
  useId,
  type ButtonHTMLAttributes,
  type InputHTMLAttributes,
  type ReactNode,
  type SelectHTMLAttributes,
} from "react";
import type { IconName } from "../icons";
import { cx } from "../util";
import { Icon } from "./Icon";

export type Tone = "neutral" | "glut" | "success" | "warning" | "error" | "info";
export type ConnectionState = "idle" | "scanning" | "connected" | "compiling" | "flashing" | "error";

/* ---------- Button ---------- */
export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  icon?: IconName;
  iconRight?: IconName;
  shortcut?: string;
  busy?: boolean;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, iconRight, shortcut, busy, children, className, ...rest },
  ref,
) {
  const iconOnly = children == null && icon;
  return (
    <button
      ref={ref}
      type="button"
      {...rest}
      className={cx("fg-btn", `fg-btn--${variant}`, `fg-btn--${size}`, iconOnly && "fg-btn--icon", busy && "is-busy", className)}
      aria-busy={busy || undefined}
    >
      {icon ? <Icon name={icon} size={size === "lg" ? 16 : 14} /> : null}
      {children != null ? <span>{children}</span> : null}
      {shortcut ? <span className="fg-btn-kbd">{shortcut}</span> : null}
      {iconRight ? <Icon name={iconRight} size={12} /> : null}
    </button>
  );
});

/* ---------- Kbd ---------- */
export interface KbdProps {
  keys?: string[];
  children?: ReactNode;
  className?: string;
}

export function Kbd({ keys, children, className }: KbdProps) {
  const list = keys ?? [children];
  return (
    <span className={cx("fg-kbds", className)}>
      {list.map((k, i) => (
        <kbd key={i} className="fg-kbd">
          {k}
        </kbd>
      ))}
    </span>
  );
}

/* ---------- Badge ---------- */
export interface BadgeProps {
  tone?: Tone;
  icon?: IconName;
  mono?: boolean;
  className?: string;
  children?: ReactNode;
}

export function Badge({ tone = "neutral", icon, mono, className, children }: BadgeProps) {
  return (
    <span className={cx("fg-badge", `fg-badge--${tone}`, mono && "fg-badge--mono", className)}>
      {icon ? <Icon name={icon} size={12} /> : null}
      {children}
    </span>
  );
}

/* ---------- StatusDot ---------- */
const STATUS_LABEL: Record<ConnectionState, string> = {
  idle: "Nicht verbunden",
  scanning: "Suche Port …",
  connected: "Verbunden",
  compiling: "Kompiliere …",
  flashing: "Flashe …",
  error: "Verbindung verloren",
};

export interface StatusDotProps {
  state?: ConnectionState;
  label?: ReactNode | false;
  className?: string;
}

export function StatusDot({ state = "idle", label, className }: StatusDotProps) {
  return (
    <span className={cx("fg-status", `fg-status--${state}`, className)} role="status">
      <span key={state} className="fg-dot" />
      {label === false ? null : <span>{label || STATUS_LABEL[state]}</span>}
    </span>
  );
}

/* ---------- Input ---------- */
export interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  label?: string;
  hint?: string;
  error?: string;
  icon?: IconName;
  mono?: boolean;
  size?: "sm" | "md";
  trailing?: ReactNode;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { label, hint, error, icon, mono, size, trailing, className, ...rest },
  ref,
) {
  const id = useId();
  const note = error ?? hint;
  return (
    <div className={cx("fg-field", error && "is-invalid", className)}>
      {label ? (
        <label className="fg-field-label" htmlFor={id}>
          {label}
        </label>
      ) : null}
      <span className={cx("fg-input", size === "sm" && "fg-input--sm", mono && "fg-input--mono")}>
        {icon ? <Icon name={icon} size={14} /> : null}
        <input
          ref={ref}
          id={id}
          aria-invalid={error ? true : undefined}
          aria-describedby={note ? `${id}-note` : undefined}
          {...rest}
        />
        {trailing ?? null}
      </span>
      {note ? (
        <span id={`${id}-note`} className={error ? "fg-field-error" : "fg-field-hint"}>
          {note}
        </span>
      ) : null}
    </div>
  );
});

/* ---------- Select ---------- */
export interface SelectOption {
  value: string;
  label: string;
}

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> {
  label?: string;
  hint?: string;
  options: Array<string | SelectOption>;
  icon?: IconName;
  mono?: boolean;
  size?: "sm" | "md";
}

export function Select({ label, hint, options, icon, mono, size, className, ...rest }: SelectProps) {
  const id = useId();
  const opts = options.map((x) => (typeof x === "string" ? { value: x, label: x } : x));
  return (
    <div className={cx("fg-field", className)}>
      {label ? (
        <label className="fg-field-label" htmlFor={id}>
          {label}
        </label>
      ) : null}
      <span className={cx("fg-input", "fg-input--select", size === "sm" && "fg-input--sm", mono && "fg-input--mono")}>
        {icon ? <Icon name={icon} size={14} /> : null}
        <select id={id} {...rest}>
          {opts.map((x) => (
            <option key={x.value} value={x.value}>
              {x.label}
            </option>
          ))}
        </select>
        <Icon name="chevron-down" size={12} className="fg-select-chev" />
      </span>
      {hint ? <span className="fg-field-hint">{hint}</span> : null}
    </div>
  );
}

/* ---------- Toggle ---------- */
export interface ToggleProps {
  icon?: IconName;
  on?: boolean;
  title?: string;
  onToggle?(): void;
  children?: ReactNode;
}

export function Toggle({ icon, on, title, onToggle, children }: ToggleProps) {
  return (
    <button
      type="button"
      className={cx("fg-toggle", icon && "fg-toggle--icon")}
      aria-pressed={!!on}
      onClick={onToggle}
      title={title}
      aria-label={icon ? title : undefined}
    >
      {icon ? <Icon name={icon} size={14} /> : <i />}
      {icon ? null : children}
    </button>
  );
}

/* ---------- Checkbox ---------- */
export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "size"> {
  label: ReactNode;
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox({ label, className, ...rest }, ref) {
  return (
    <label className={cx("fg-check", className)}>
      <input ref={ref} type="checkbox" {...rest} />
      <span className="fg-check-box" aria-hidden>
        <Icon name="check" size={12} strokeWidth={2} />
      </span>
      <span>{label}</span>
    </label>
  );
});
