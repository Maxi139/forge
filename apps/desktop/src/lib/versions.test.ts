import { describe, expect, it } from "vitest";
import { version_cmp } from "./versions";

describe("version_cmp", () => {
  it("vergleicht numerisch statt als Text", () => {
    expect(version_cmp("3.3.12", "3.3.2")).toBeGreaterThan(0);
    expect(version_cmp("1.9.9", "1.10.0")).toBeLessThan(0);
    expect(version_cmp("2.0", "2.0.0")).toBe(0);
  });
});
