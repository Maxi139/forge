import { describe, expect, it } from "vitest";
import type { DeviceInfo } from "./backend";
import { chooseDevice, formatBytes, parsePlotLine } from "./hardware";

const dev = (id: string, likelyEsp: boolean): DeviceInfo => ({ id, port: `/dev/cu.${id}`, vid: 0, pid: 0, serial: null, product: null, manufacturer: null, bridge: "other", label: id, likelyEsp, mac: null });

describe("chooseDevice", () => {
  it("nimmt das einzige passende Board", () => {
    expect(chooseDevice([dev("a", true), dev("b", false)], null, null)).toBe("a");
  });
  it("lässt bei mehreren Kandidaten die Wahl offen", () => {
    expect(chooseDevice([dev("a", true), dev("b", true)], null, null)).toBeNull();
  });
  it("bleibt bei der laufenden und dann bei der gemerkten Wahl", () => {
    const list = [dev("a", true), dev("b", true)];
    expect(chooseDevice(list, "b", null)).toBe("b");
    expect(chooseDevice(list, "b", "a")).toBe("a");
    expect(chooseDevice([dev("b", true)], "a", "a")).toBe("b");
  });
  it("wählt nichts, wenn nichts passt", () => {
    expect(chooseDevice([dev("x", false)], null, null)).toBeNull();
    expect(chooseDevice([], "a", "a")).toBeNull();
  });
});

describe("parsePlotLine", () => {
  it("liest name:wert-Paare", () => {
    expect(parsePlotLine("raw:2048 volts:1.65")).toEqual([["raw", 2048], ["volts", 1.65]]);
    expect(parsePlotLine("temp: -3.5, hum:40")).toEqual([["temp", -3.5], ["hum", 40]]);
  });
  it("liest Zahlenreihen", () => {
    expect(parsePlotLine("1,2.5,3")).toEqual([["Wert 1", 1], ["Wert 2", 2.5], ["Wert 3", 3]]);
    expect(parsePlotLine("10\t20")).toEqual([["Wert 1", 10], ["Wert 2", 20]]);
  });
  it("ignoriert Text", () => {
    expect(parsePlotLine("Verbinde mit WLAN")).toEqual([]);
    expect(parsePlotLine("I (123) wifi: connected")).toEqual([]);
  });
});

describe("formatBytes", () => {
  it("nutzt Komma und passende Einheit", () => {
    expect(formatBytes(742_000)).toBe("725 KB");
    expect(formatBytes(1_310_720)).toBe("1,25 MB");
    expect(formatBytes(512)).toBe("512 B");
  });
});

import { boardForChip, chipFamily, familyOfFqbn } from "./boards";

describe("chip detection", () => {
  it("maps esptool names to families", () => {
    expect(chipFamily("ESP32-D0WD-V3")).toBe("ESP32");
    expect(chipFamily("ESP32-S3")).toBe("ESP32-S3");
    expect(chipFamily("ESP32-C6")).toBe("ESP32-C6");
    expect(chipFamily("ESP32")).toBe("ESP32");
  });
  it("reads the family from an fqbn", () => {
    expect(familyOfFqbn("esp32:esp32:esp32s3")).toBe("ESP32-S3");
    expect(familyOfFqbn("esp32:esp32:XIAO_ESP32S3")).toBe("ESP32-S3");
    expect(familyOfFqbn("esp32:esp32:esp32")).toBe("ESP32");
    expect(familyOfFqbn("esp32:esp32:nano_nora")).toBeNull();
  });
  it("suggests the default board", () => {
    expect(boardForChip("ESP32-D0WD-V3").fqbn).toBe("esp32:esp32:esp32");
    expect(boardForChip("ESP32-S3").fqbn).toBe("esp32:esp32:esp32s3");
  });
});
