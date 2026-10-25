import { error as logError } from "@tauri-apps/plugin-log";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

function describe(value: unknown): string {
  if (value instanceof Error) return value.stack ?? `${value.name}: ${value.message}`;
  try {
    return typeof value === "string" ? value : JSON.stringify(value);
  } catch {
    return String(value);
  }
}

/** Schreibt einen Fehler der Oberfläche in die Logdatei der App. Im Browser nur in die Konsole. */
export function reportError(where: string, value: unknown) {
  const text = `${where}: ${describe(value)}`;
  console.error(text);
  if (isTauri()) void logError(text).catch(() => undefined);
}

/** Unbehandelte Fehler und abgelehnte Promises landen im Protokoll. */
export function installErrorHandlers() {
  window.addEventListener("error", (e) => reportError("Unbehandelter Fehler", e.error ?? e.message));
  window.addEventListener("unhandledrejection", (e) => reportError("Unbehandelte Ablehnung", e.reason));
}
