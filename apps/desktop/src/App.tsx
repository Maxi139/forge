import { Splash } from "@forge/ui";
import { useEffect, useState } from "react";
import { backend } from "./lib/backend";
import { useAi } from "./lib/ai";
import { useHardware } from "./lib/hardware";
import { useStore } from "./lib/store";
import { Shell } from "./ide/Shell";
import { Gallery } from "./views/Gallery";

const MIN_SPLASH_MS = 450;

function useHashRoute() {
  const [hash, setHash] = useState(() => window.location.hash);
  useEffect(() => {
    const on = () => setHash(window.location.hash);
    window.addEventListener("hashchange", on);
    return () => window.removeEventListener("hashchange", on);
  }, []);
  return hash;
}

export function App() {
  const hash = useHashRoute();
  const ready = useStore((s) => s.ready);
  const [splashDone, setSplashDone] = useState(false);

  useEffect(() => {
    const started = performance.now();
    void Promise.all([useStore.getState().init(), useHardware.getState().init(), useAi.getState().init()])
      .then(() => {
        // Beim ersten Start fehlt oft der ESP32-Core. Dann führt Forge direkt durch die Einrichtung.
        const tc = useHardware.getState().toolchain;
        if (tc && !tc.esp32Core) useStore.getState().setDialog({ kind: "toolchain" });
        if (tc?.esp32Core) void useHardware.getState().loadBoards();
      })
      .then(async () => {
        // Gestartet mit einem Pfad (Finder, Terminal)? Dann direkt öffnen.
        const path = await backend().takePendingOpen();
        if (path) await useStore.getState().openProject(path);
      })
      .catch((e) => console.error("Start unvollständig", e))
      .finally(() => {
        // Auch nach einem Fehler beim Start erscheint die Oberfläche, sonst bliebe nur der Startbildschirm.
        const wait = Math.max(0, MIN_SPLASH_MS - (performance.now() - started));
        setTimeout(() => setSplashDone(true), wait);
      });
  }, []);

  // Ein Pfad, der an die laufende App übergeben wird.
  useEffect(() => {
    let off: (() => void) | undefined;
    let cancelled = false;
    void backend()
      .onOpenPath((p) => void useStore.getState().openProject(p))
      .then((u) => (cancelled ? u() : (off = u)));
    return () => {
      cancelled = true;
      off?.();
    };
  }, []);

  if (hash.startsWith("#/gallery")) return <Gallery />;

  return (
    <>
      {ready && splashDone ? <Shell /> : null}
      {!(ready && splashDone) ? (
        <div className="ide-startup">
          <Splash progress={ready ? 1 : 0.45} status="Starte …" />
        </div>
      ) : null}
    </>
  );
}
