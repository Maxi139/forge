import { describe, expect, it } from "vitest";
import { isDecodable, projectFileFor } from "./monitor";

describe("isDecodable", () => {
  it("erkennt Backtrace- und Registerzeilen", () => {
    expect(isDecodable("Backtrace: 0x400d1234:0x3ffb1f50 0x400d1288:0x3ffb1f70")).toBe(true);
    expect(isDecodable("PC      : 0x400d1234  PS      : 0x00060b30")).toBe(true);
    expect(isDecodable("MEPC    : 0x42001234  RA      : 0x42001300")).toBe(true);
  });
  it("ignoriert normale Zeilen", () => {
    expect(isDecodable("I (312) wifi: Verbunden")).toBe(false);
    expect(isDecodable("EXCVADDR: 0x00000000")).toBe(false);
    expect(isDecodable("raw:2048 volts:1.65")).toBe(false);
  });
});

describe("projectFileFor", () => {
  const root = "/p/Blink";
  const main = "/p/Blink/Blink.ino";
  it("nimmt Dateien im Projekt", () => {
    expect(projectFileFor("/p/Blink/util.cpp", root, main)).toBe("/p/Blink/util.cpp");
  });
  it("bildet die Sketch-Kopie auf die .ino ab", () => {
    expect(projectFileFor("/cache/sketch/Blink.ino.cpp", root, main)).toBe(main);
    expect(projectFileFor("/cache/sketch/Extra.ino.cpp", root, main)).toBe("/p/Blink/Extra.ino");
  });
  it("lässt Kerncode unverlinkt", () => {
    expect(projectFileFor("/esp32/cores/esp32/main.cpp", root, main)).toBeNull();
    expect(projectFileFor("??", root, main)).toBeNull();
  });
});
