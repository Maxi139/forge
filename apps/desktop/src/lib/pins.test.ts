import { describe, expect, it } from "vitest";
import { usedPins } from "./pins";
import { SAMPLE_SKETCH } from "./backend/mock";

describe("usedPins", () => {
  it("findet Pins aus defines, auch wenn sie nur in Konstruktoren stehen", () => {
    expect(usedPins(SAMPLE_SKETCH)).toEqual({ 48: "LED", 4: "SENSOR" });
  });

  it("liest Pin-Aufrufe mit Zahlen und Namen", () => {
    const src = "const int buttonPin = 0;\nvoid setup() { pinMode(2, OUTPUT); pinMode(buttonPin, INPUT_PULLUP); attachInterrupt(digitalPinToInterrupt(buttonPin), f, FALLING); }";
    expect(usedPins(src)).toEqual({ 2: "belegt", 0: "BUTTON" });
  });

  it("ignoriert Zahlen, die kein Pin sind", () => {
    expect(usedPins("#define NUM_LEDS 1\n#define BAUD 115200\nint x = 5;\nvoid f() { delay(50); }")).toEqual({});
  });
});
