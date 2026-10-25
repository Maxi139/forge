import { create } from "zustand";
import { t } from "../i18n/de";
import { backend } from "./backend";
import { reportError } from "./log";
import { errorText, useStore } from "./store";

const KEY = "forge.updates";

function readAutoCheck(): boolean {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "{}") as { autoCheck?: unknown };
    return raw.autoCheck !== false;
  } catch {
    return true;
  }
}

interface AppUpdateState {
  /** Beim Start nach einer neuen Version fragen. */
  autoCheck: boolean;
  busy: boolean;
  setAutoCheck(on: boolean): void;
  /** `silent`: Fehler und „aktuell“ bleiben unsichtbar (Prüfung beim Start). */
  check(opts?: { silent?: boolean }): Promise<void>;
}

export const useAppUpdate = create<AppUpdateState>((set, get) => ({
  autoCheck: readAutoCheck(),
  busy: false,
  setAutoCheck(on) {
    set({ autoCheck: on });
    try {
      localStorage.setItem(KEY, JSON.stringify({ autoCheck: on }));
    } catch {
      // Ohne Speicher gilt die Einstellung nur bis zum Beenden.
    }
  },
  async check({ silent = false } = {}) {
    if (get().busy) return;
    set({ busy: true });
    const toast = useStore.getState().toast;
    try {
      const update = await backend().appUpdateCheck();
      if (!update) {
        if (!silent) toast({ kind: "success", title: t.appUpdate.current, ttl: 4000 });
        return;
      }
      toast({
        kind: "info",
        title: t.appUpdate.available(update.version),
        message: update.notes ?? undefined,
        actions: [
          { label: t.appUpdate.install, run: () => void install(update.version) },
          { label: t.appUpdate.later, run: () => undefined },
        ],
      });
    } catch (e) {
      // Ohne Schlüssel oder ohne Netz soll die App nicht stören. Nur eine ausdrückliche Prüfung meldet es.
      if (silent) reportError("Update-Prüfung beim Start", e);
      else toast({ kind: "error", title: t.appUpdate.failed, message: errorText(e) });
    } finally {
      set({ busy: false });
    }
  },
}));

async function install(version: string) {
  const toast = useStore.getState().toast;
  toast({ kind: "info", title: t.appUpdate.installing(version), ttl: 4000 });
  try {
    await backend().appUpdateInstall();
    toast({
      kind: "success",
      title: t.appUpdate.installed(version),
      actions: [
        { label: t.appUpdate.restart, run: () => void backend().appRestart() },
        { label: t.appUpdate.later, run: () => undefined },
      ],
    });
  } catch (e) {
    toast({ kind: "error", title: t.appUpdate.installFailed, message: errorText(e) });
  }
}

/** Kurz nach dem Start, wenn der Nutzer es nicht abgeschaltet hat. */
export function startupUpdateCheck() {
  if (!useAppUpdate.getState().autoCheck) return;
  setTimeout(() => void useAppUpdate.getState().check({ silent: true }), 8000);
}
