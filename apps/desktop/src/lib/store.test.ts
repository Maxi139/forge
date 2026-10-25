import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createMockBackend } from "./backend/mock";
import { setBackend } from "./backend";
import { isCancelled, stashUnsaved, useStore } from "./store";

const SAMPLE = "/Users/demo/Documents/Forge/neopixel-sensor";
const MAIN = `${SAMPLE}/neopixel-sensor.ino`;

describe("Puffer und Tabs", () => {
  beforeEach(async () => {
    setBackend(createMockBackend({ speed: 0 }));
    // Ein Test davor kann ungespeicherte Änderungen hinterlassen haben. Sonst fragt das Schließen nach.
    useStore.setState({ toasts: [], project: null, tabs: [], active: null, buffers: {}, dialog: null });
    await useStore.getState().openProject(SAMPLE);
  });
  afterEach(() => {
    setBackend(null);
    sessionStorage.clear();
  });

  it("jede Änderung zählt die Version hoch, auch bei gleicher Länge", () => {
    const s = useStore.getState();
    const before = s.buffers[MAIN].version;
    s.updateBuffer(MAIN, "aaaa");
    s.updateBuffer(MAIN, "bbbb");
    expect(useStore.getState().buffers[MAIN].version).toBe(before + 2);
  });

  it("ein unveränderter Text zählt nicht", () => {
    const s = useStore.getState();
    s.updateBuffer(MAIN, "x");
    const v = useStore.getState().buffers[MAIN].version;
    s.updateBuffer(MAIN, "x");
    expect(useStore.getState().buffers[MAIN].version).toBe(v);
  });

  it("beim Schließen eines Tabs vergisst der Editor die Datei", async () => {
    const seen: string[] = [];
    const on = (e: Event) => seen.push((e as CustomEvent<{ key: string }>).detail.key);
    window.addEventListener("forge:forget-doc", on);
    await useStore.getState().closeTab(MAIN);
    window.removeEventListener("forge:forget-doc", on);
    expect(seen).toEqual([MAIN]);
  });

  it("ein Projektwechsel wird angekündigt", async () => {
    const fn = vi.fn();
    window.addEventListener("forge:project-switched", fn);
    await useStore.getState().openProject(SAMPLE);
    window.removeEventListener("forge:project-switched", fn);
    expect(fn).toHaveBeenCalled();
  });

  it("ungespeicherte Texte werden vor dem Neuladen gesichert", () => {
    useStore.getState().updateBuffer(MAIN, "// neu");
    stashUnsaved();
    const snap = JSON.parse(sessionStorage.getItem("forge.recover") ?? "null");
    expect(snap.files[MAIN]).toBe("// neu");
  });

  it("ohne ungespeicherte Texte wird nichts gesichert", () => {
    stashUnsaved();
    expect(sessionStorage.getItem("forge.recover")).toBeNull();
  });
});

describe("Toasts", () => {
  beforeEach(() => useStore.setState({ toasts: [] }));

  it("Fehler bleiben stehen, wenn mehr als drei Toasts kommen", () => {
    const s = useStore.getState();
    s.toast({ kind: "error", title: "Fehler" });
    s.toast({ kind: "warning", title: "a" });
    s.toast({ kind: "warning", title: "b" });
    s.toast({ kind: "warning", title: "c" });
    const titles = useStore.getState().toasts.map((x) => x.title);
    expect(titles).toEqual(["Fehler", "b", "c"]);
  });
});

describe("isCancelled", () => {
  it("erkennt beide Schreibweisen des Backends", () => {
    expect(isCancelled(new Error("Abgebrochen."))).toBe(true);
    expect(isCancelled("abgebrochen")).toBe(true);
  });
  it("lässt andere Fehler durch", () => {
    expect(isCancelled(new Error("Port belegt"))).toBe(false);
    expect(isCancelled(undefined)).toBe(false);
  });
});
