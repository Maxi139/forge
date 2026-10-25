import { describe, expect, it } from "vitest";
import { filterLines, formatLog } from "./util";

const lines = [
  { text: "I (12) wifi: Verbunden", kind: "rx" },
  { text: "E (30) i2c: Timeout", kind: "err" },
  { text: "raw:12", kind: "rx" },
];

describe("filterLines", () => {
  it("sucht ohne Beachtung der Schreibweise", () => {
    expect(filterLines(lines, { query: "WIFI", matchCase: false, errorsOnly: false })).toHaveLength(1);
  });
  it("beachtet die Schreibweise auf Wunsch", () => {
    expect(filterLines(lines, { query: "WIFI", matchCase: true, errorsOnly: false })).toHaveLength(0);
    expect(filterLines(lines, { query: "wifi", matchCase: true, errorsOnly: false })).toHaveLength(1);
  });
  it("zeigt nur Fehler", () => {
    expect(filterLines(lines, { query: "", matchCase: false, errorsOnly: true }).map((l) => l.text)).toEqual(["E (30) i2c: Timeout"]);
  });
  it("kombiniert Text und Fehler", () => {
    expect(filterLines(lines, { query: "wifi", matchCase: false, errorsOnly: true })).toHaveLength(0);
  });
});

describe("formatLog", () => {
  it("setzt Zeitstempel und Sendemarke", () => {
    const text = formatLog([{ t: 61_005, kind: "tx", text: "hi" }, { t: 62_000, kind: "rx", text: "ok" }], true);
    expect(text).toBe("01:01.005 → hi\n01:02.000 ok");
  });
  it("lässt Zeitstempel weg", () => {
    expect(formatLog([{ t: 5, text: "a" }, { text: "b" }], false)).toBe("a\nb");
  });
});
