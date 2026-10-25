import { openUrl } from "@tauri-apps/plugin-opener";

/** Die Projektseite. Muss mit `PROJECT_URL` in menu.rs und den Rechten in capabilities/default.json übereinstimmen. */
export const PROJECT_URL = "https://github.com/maximilianschmidt/forge";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Öffnet eine Adresse im Standardbrowser. Im Browser-Modus (Mock) in einem neuen Tab. */
export async function openExternal(url: string): Promise<void> {
  if (isTauri()) await openUrl(url);
  else window.open(url, "_blank", "noopener");
}
