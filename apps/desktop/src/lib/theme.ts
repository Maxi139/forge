import { useSyncExternalStore } from "react";

export type ThemePreference = "system" | "dark" | "light";
export type ResolvedTheme = "dark" | "light";

const KEY = "forge.theme";
const listeners = new Set<() => void>();
const media = typeof window !== "undefined" && window.matchMedia ? window.matchMedia("(prefers-color-scheme: light)") : null;

function read(): ThemePreference {
  try {
    const v = localStorage.getItem(KEY);
    return v === "dark" || v === "light" ? v : "system";
  } catch {
    return "system";
  }
}

let preference: ThemePreference = read();

function resolve(): ResolvedTheme {
  if (preference !== "system") return preference;
  return media?.matches ? "light" : "dark";
}

function apply() {
  const root = document.documentElement;
  if (preference === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", preference);
  listeners.forEach((l) => l());
}

export function setThemePreference(next: ThemePreference) {
  preference = next;
  try {
    if (next === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, next);
  } catch {
    /* Ohne Speicher gilt die Auswahl nur für diese Sitzung. */
  }
  apply();
}

export function toggleTheme() {
  setThemePreference(resolve() === "dark" ? "light" : "dark");
}

media?.addEventListener("change", () => {
  if (preference === "system") listeners.forEach((l) => l());
});

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function useTheme(): { preference: ThemePreference; resolved: ResolvedTheme } {
  const resolved = useSyncExternalStore(subscribe, resolve, () => "dark" as ResolvedTheme);
  const pref = useSyncExternalStore(subscribe, () => preference, () => "system" as ThemePreference);
  return { preference: pref, resolved };
}
