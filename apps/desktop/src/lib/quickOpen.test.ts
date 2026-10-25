import { fuzzy } from "@forge/ui";
import { describe, expect, it } from "vitest";
import type { FileNode } from "./backend";
import { fileCommands, flattenFiles } from "./quickOpen";

const file = (path: string): FileNode => ({ name: path.split("/").pop()!, path, isDir: false });
const tree: FileNode[] = [
  { name: "data", path: "/p/data", isDir: true, children: [file("/p/data/index.html")] },
  file("/p/p.ino"),
  file("/p/sensor.cpp"),
  file("/p/sensor.h"),
];
const labels = { recent: "Zuletzt", files: "Dateien" };

describe("flattenFiles", () => {
  it("liefert nur Dateien, auch aus Unterordnern", () => {
    expect(flattenFiles(tree)).toEqual(["/p/data/index.html", "/p/p.ino", "/p/sensor.cpp", "/p/sensor.h"]);
  });
});

describe("fileCommands", () => {
  it("setzt die zuletzt geöffneten Dateien nach oben und lässt die aktive weg", () => {
    const cmds = fileCommands(tree, "/p", ["/p/sensor.h", "/p/p.ino", "/p/gelöscht.cpp"], labels, "/p/p.ino");
    const recent = cmds.filter((c) => c.group === "Zuletzt");
    expect(recent.map((c) => c.label)).toEqual(["sensor.h"]);
    expect(recent.every((c) => c.emptyOnly)).toBe(true);
    expect(cmds.filter((c) => c.group === "Dateien")).toHaveLength(4);
  });

  it("zeigt Ordner als Hinweis und sucht im ganzen Pfad", () => {
    const [index] = fileCommands(tree, "/p", [], labels, null);
    expect(index).toMatchObject({ label: "index.html", hint: "data", search: "data/index.html" });
    expect(fuzzy("datai", index.search!)).not.toBeNull();
  });

  it("findet Dateien unscharf und bevorzugt Wortanfänge", () => {
    const names = ["sensor.cpp", "config_sensor.h", "unsorted.cpp"];
    const scored = names.map((n) => ({ n, m: fuzzy("sen", n) })).filter((x) => x.m).sort((a, b) => b.m!.score - a.m!.score);
    expect(scored[0].n).toBe("sensor.cpp");
    expect(scored.map((x) => x.n)).not.toContain("unsorted.cpp");
  });
});
