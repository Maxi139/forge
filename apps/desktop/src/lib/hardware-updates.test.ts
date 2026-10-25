import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { createMockBackend } from "./backend/mock";
import { setBackend } from "./backend";
import { useHardware } from "./hardware";
import { useStore } from "./store";

describe("Beispiele, Updates und Speicher im Mock", () => {
  beforeEach(() => {
    setBackend(createMockBackend({ speed: 0 }));
    useStore.setState({ toasts: [], project: null, tabs: [], active: null, buffers: {}, dialog: null });
    useHardware.setState({ outdated: [], libraries: [], libraryBusy: {} });
  });
  afterEach(() => setBackend(null));

  it("Beispiele öffnen sich als Kopie in einem neuen Sketch", async () => {
    const { backend } = await import("./backend");
    const list = await backend().examples();
    expect(list[0].source).toBe("core");
    const scan = list.find((e) => e.name === "WiFiScan")!;
    expect(await useStore.getState().openExample(scan.path)).toBe(true);
    const p = useStore.getState().project!;
    expect(p.kind).toBe("sketch");
    expect(p.name).toBe("WiFiScan");
    expect(useStore.getState().buffers[p.mainFile].text).toContain("WiFi.scanNetworks");
    // Ein zweites Mal bekommt eine eigene Kopie.
    await useStore.getState().openExample(scan.path);
    expect(useStore.getState().project!.name).toBe("WiFiScan_2");
  });

  it("veraltete Bibliotheken zählen für das Badge und lassen sich alle aktualisieren", async () => {
    await useHardware.getState().refreshOutdated();
    expect(useHardware.getState().outdated.map((o) => o.name).sort()).toEqual(["ArduinoJson", "NimBLE-Arduino"]);
    await useHardware.getState().refreshLibraries();
    await useHardware.getState().upgradeAllLibraries();
    expect(useHardware.getState().outdated).toEqual([]);
    expect(useHardware.getState().libraries.find((l) => l.name === "ArduinoJson")?.version).toBe("7.2.1");
  });

  it("ZIP-Installation fügt die Bibliothek hinzu", async () => {
    await useHardware.getState().installLibraryZip();
    expect(useHardware.getState().libraries.some((l) => l.name === "Ultraschall-Sensor")).toBe(true);
  });

  it("Core-Update setzt die installierte auf die neueste Version", async () => {
    await useHardware.getState().init();
    expect(useHardware.getState().toolchain?.esp32Core).toBe("3.2.0");
    await useHardware.getState().upgradeCore();
    expect(useHardware.getState().toolchain?.esp32Core).toBe("3.3.12");
  });

  it("Speicherdetails gibt es erst nach einem Build", async () => {
    await useHardware.getState().loadMemory();
    expect(useHardware.getState().memory.state).toBe("error");
    const { backend } = await import("./backend");
    const scan = (await backend().examples()).find((e) => e.name === "WiFiScan")!;
    await backend().openExample(scan.path);
    await backend().build({ fqbn: "esp32:esp32:esp32s3", options: {} }, false);
    await useHardware.getState().loadMemory();
    const m = useHardware.getState().memory;
    expect(m.state).toBe("ready");
    expect(m.data?.groups.map((g) => g.label)).toEqual(["IRAM", "DRAM", "Flash-Code", "Rodata"]);
    expect(m.data?.symbols[0].size).toBeGreaterThan(m.data!.symbols[1].size);
  });
});
