import type { CSSProperties } from "react";
import { ICON_PATHS, type IconName } from "../icons";
import { cx } from "../util";

export interface IconProps {
  name: IconName;
  size?: number;
  label?: string;
  strokeWidth?: number;
  className?: string;
  style?: CSSProperties;
}

export function Icon({ name, size = 16, label, strokeWidth, className, style }: IconProps) {
  return (
    <svg
      className={cx("fg-icon", className)}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth ?? (name === "more" ? 2 : 1.5)}
      strokeLinecap="round"
      strokeLinejoin="round"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={style}
    >
      <path d={ICON_PATHS[name] ?? ICON_PATHS.more} />
    </svg>
  );
}
