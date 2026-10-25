import type { Backend } from "./backend";
import { createMockEditor } from "./mock-editor";
import { createMockHardware, MOCK_EXAMPLE_SOURCES, type MockControl } from "./mock-hardware";
import type { CreateProjectRequest, FileNode, ForgeConfig, ProjectInfo, RecentProject } from "./types";

const ROOT = "/Users/demo/Documents/Forge";

export const SAMPLE_SKETCH = `#include <Adafruit_NeoPixel.h>

#define LED_PIN    48
#define NUM_LEDS   1
#define SENSOR_PIN 4

// Onboard-RGB-LED des DevKitC-1
Adafruit_NeoPixel strip(NUM_LEDS, LED_PIN, NEO_GRB + NEO_KHZ800);

void setup() {
  Serial.begin(115200);
  strip.begin();
  strip.setBrightness(40);
}

void loop() {
  int raw = analogRead(SENSOR_PIN);
  float volts = raw * 3.3f / 4095.0f;
  Serial.printf("raw:%d volts:%.2f\\n", raw, volts);
  strip.setPixelColor(0, strip.Color(map(raw, 0, 4095, 0, 255), 40, 0));
  strip.show();
  delay(50)
}
`;

function config(name: string, fqbn = "esp32:esp32:esp32s3"): ForgeConfig {
  return {
    project: { name },
    board: { fqbn, options: {} },
    libraries: {},
    secrets: { names: [] },
    ai: { enabled: true },
  };
}

const TEMPLATES: Record<string, string> = {
  blank: "void setup() {\n}\n\nvoid loop() {\n}\n",
  blink:
    "#ifndef LED_BUILTIN\n#define LED_BUILTIN 2\n#endif\n\nvoid setup() {\n  pinMode(LED_BUILTIN, OUTPUT);\n}\n\nvoid loop() {\n  digitalWrite(LED_BUILTIN, HIGH);\n  delay(500);\n  digitalWrite(LED_BUILTIN, LOW);\n  delay(500);\n}\n",
  wifi: '#include <WiFi.h>\n#include <ForgeSecrets.h>\n\nvoid setup() {\n  WiFi.begin(Secret("WIFI_SSID").c_str(), Secret("WIFI_PASS").c_str());\n}\n\nvoid loop() {\n}\n',
  webserver: "#include <WebServer.h>\n\nWebServer server(80);\n\nvoid setup() {\n  server.begin();\n}\n\nvoid loop() {\n  server.handleClient();\n}\n",
};

export function createMockBackend(opts: { devicesAtStart?: boolean; speed?: number; secretNames?: string[] } = {}): Backend & { control: MockControl } {
  const files = new Map<string, string>();
  const dirs = new Set<string>([ROOT]);
  const projects = new Map<string, ProjectInfo>();
  let recents: RecentProject[] = [];

  const sample = `${ROOT}/neopixel-sensor`;
  dirs.add(sample).add(`${sample}/data`);
  files.set(`${sample}/neopixel-sensor.ino`, SAMPLE_SKETCH);
  files.set(`${sample}/config.h`, "#pragma once\n\n#define SAMPLE_INTERVAL_MS 50\n");
  files.set(`${sample}/sensor.h`, "#pragma once\n\nint readSensor();\n");
  files.set(`${sample}/sensor.cpp`, '#include <Arduino.h>\n#include "sensor.h"\n\nint readSensor() {\n  return analogRead(4);\n}\n');
  files.set(`${sample}/data/index.html`, "<!doctype html>\n<title>Sensor</title>\n");
  files.set(`${sample}/forge.toml`, '[project]\nname = "neopixel-sensor"\n\n[board]\nfqbn = "esp32:esp32:esp32s3"\n');
  projects.set(sample, {
    kind: "project",
    name: "neopixel-sensor",
    path: sample,
    mainFile: `${sample}/neopixel-sensor.ino`,
    config: { ...config("neopixel-sensor"), libraries: { "Adafruit NeoPixel": "1.12.3" } },
  });
  recents = [{ path: sample, name: "neopixel-sensor", kind: "project", openedAt: 1_761_350_000 }];

  let current = "";
  const projectFiles = () => Object.fromEntries([...files].filter(([p]) => p.startsWith(`${current}/`)));
  const hardware = createMockHardware(projectFiles, opts);
  const changeHandlers = new Set<(paths: string[]) => void>();

  const need = (p: string) => {
    if (!files.has(p)) throw new Error(`Datei nicht gefunden: ${p}`);
    return files.get(p)!;
  };

  function tree(dir: string): FileNode[] {
    const out: FileNode[] = [];
    for (const d of dirs) {
      if (d !== dir && d.startsWith(`${dir}/`) && !d.slice(dir.length + 1).includes("/")) {
        out.push({ name: d.slice(dir.length + 1), path: d, isDir: true, children: tree(d) });
      }
    }
    for (const f of files.keys()) {
      if (f.startsWith(`${dir}/`) && !f.slice(dir.length + 1).includes("/")) {
        out.push({ name: f.slice(dir.length + 1), path: f, isDir: false });
      }
    }
    return out.sort((a, b) => Number(b.isDir) - Number(a.isDir) || a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
  }

  const open = (path: string): ProjectInfo => {
    const info = projects.get(path);
    if (!info) throw new Error(`Ordner nicht gefunden: ${path}`);
    current = path;
    return info;
  };

  return {
    ...hardware.api,
    control: hardware.control,
    setBoard: async (fqbn, options) => {
      const info = projects.get(current);
      if (!info) throw new Error("Es ist kein Projekt geöffnet.");
      const next = { ...info, config: { ...info.config, board: { fqbn, options } } };
      projects.set(current, next);
      return next;
    },
    native: false,
    pickFolder: async () => sample,
    defaultProjectsDir: async () => ROOT,

    openProject: async (p) => open(p),
    createProject: async (req: CreateProjectRequest) => {
      const err = validate(req.name);
      if (err) throw new Error(err);
      const dir = `${req.parent}/${req.name}`;
      if (dirs.has(dir)) throw new Error(`${dir} existiert bereits. Wähle einen anderen Namen oder Ort.`);
      dirs.add(dir);
      files.set(`${dir}/${req.name}.ino`, TEMPLATES[req.template] ?? TEMPLATES.blank);
      files.set(`${dir}/forge.toml`, `[project]\nname = "${req.name}"\n`);
      const secrets = req.template === "wifi" || req.template === "webserver" ? ["WIFI_SSID", "WIFI_PASS"] : [];
      const info: ProjectInfo = {
        kind: "project",
        name: req.name,
        path: dir,
        mainFile: `${dir}/${req.name}.ino`,
        config: { ...config(req.name, req.fqbn), secrets: { names: secrets } },
      };
      projects.set(dir, info);
      return open(dir);
    },
    renameProject: async (name) => {
      const info = projects.get(current);
      if (!info) throw new Error("Es ist kein Projekt geöffnet.");
      const err = validate(name);
      if (err) throw new Error(err);
      const to = `${current.slice(0, current.lastIndexOf("/"))}/${name}`;
      if (dirs.has(to)) throw new Error(`${to} existiert bereits. Wähle einen anderen Namen.`);
      const move = (p: string) => (p === `${current}/${info.name}.ino` ? `${to}/${name}.ino` : p.replace(current, to));
      for (const [p, text] of [...files]) if (p.startsWith(`${current}/`)) (files.delete(p), files.set(move(p), text));
      for (const d of [...dirs]) if (d === current || d.startsWith(`${current}/`)) (dirs.delete(d), dirs.add(d.replace(current, to)));
      const next: ProjectInfo = { ...info, name, path: to, mainFile: `${to}/${name}.ino`, config: { ...info.config, project: { ...info.config.project, name } } };
      projects.delete(current);
      projects.set(to, next);
      current = to;
      return next;
    },
    convertToProject: async () => {
      const info = projects.get(current);
      if (!info) throw new Error("Es ist kein Projekt geöffnet.");
      if (info.kind === "project") throw new Error("Das ist schon ein Projekt.");
      const next: ProjectInfo = { ...info, kind: "project" };
      projects.set(current, next);
      files.set(`${current}/forge.toml`, `[project]\nname = "${info.name}"\n`);
      return next;
    },
    createSketch: async () => {
      let n = 0;
      let dir = "";
      do dir = `${ROOT}/Sketches/sketch_oct25${String.fromCharCode(97 + n++)}`;
      while (dirs.has(dir));
      const name = dir.split("/").pop()!;
      dirs.add(`${ROOT}/Sketches`).add(dir);
      files.set(`${dir}/${name}.ino`, TEMPLATES.blank);
      projects.set(dir, { kind: "sketch", name, path: dir, mainFile: `${dir}/${name}.ino`, config: config(name) });
      return open(dir);
    },
    validateName: async (n) => validate(n),
    projectTree: async () => tree(current),

    readFile: async (p) => need(p),
    writeFile: async (p, t) => void files.set(p, t),
    createFile: async (p, isDir) => {
      if (files.has(p) || dirs.has(p)) throw new Error(`${p.split("/").pop()} existiert bereits.`);
      if (isDir) dirs.add(p);
      else files.set(p, "");
    },
    renameFile: async (from, to) => {
      if (files.has(from)) {
        files.set(to, files.get(from)!);
        files.delete(from);
      }
    },
    trashFile: async (p) => void (files.delete(p) || dirs.delete(p)),

    closeProject: async () => void (current = ""),

    pickProjectFile: async () => null,
    pickSavePath: async () => null,
    setMenuEnabled: async () => undefined,
    packageNeeds: async () => ({ libraries: [], core: null }),
    packageInstallNeeds: async () => undefined,
    packageSaveAs: async () => {
      throw new Error("Speichern unter gibt es nur in der App.");
    },
    packageExportFolder: async (parent) => `${parent}/export`,
    onPackageError: async () => () => undefined,
    takePendingOpen: async () => null,
    onOpenPath: async () => () => undefined,
    onMenu: async () => () => undefined,
    onProjectChanged: async (handler) => {
      changeHandlers.add(handler);
      return () => void changeHandlers.delete(handler);
    },

    recentProjects: async () => recents,
    pushRecent: async (p) => {
      const path = p.package ?? p.path;
      recents = [{ path, name: p.name, kind: p.kind, openedAt: Math.floor(Date.now() / 1000) }, ...recents.filter((r) => r.path !== path)].slice(0, 12);
    },
    removeRecent: async (p) => void (recents = recents.filter((r) => r.path !== p)),

    ...createMockEditor(projectFiles),
    openExample: async (path) => {
      const source = Object.entries(MOCK_EXAMPLE_SOURCES).find(([p]) => p.startsWith(`${path}/`));
      if (!source) throw new Error("Das Beispiel gibt es nicht mehr. Aktualisiere die Liste.");
      const base = path.split("/").pop()!;
      let name = base;
      for (let n = 2; dirs.has(`${ROOT}/Beispiele/${name}`); n++) name = `${base}_${n}`;
      const dir = `${ROOT}/Beispiele/${name}`;
      dirs.add(`${ROOT}/Beispiele`).add(dir);
      files.set(`${dir}/${name}.ino`, source[1]);
      projects.set(dir, { kind: "sketch", name, path: dir, mainFile: `${dir}/${name}.ino`, config: config(name) });
      return open(dir);
    },
  };
}

function validate(name: string): string | null {
  if (!name) return "Gib einen Namen ein.";
  if (name.length > 63) return "Der Name darf höchstens 63 Zeichen haben.";
  if (name.startsWith(".")) return "Der Name darf nicht mit einem Punkt beginnen.";
  const bad = [...name].find((c) => !/[A-Za-z0-9_.-]/.test(c));
  if (bad) return `${bad === " " ? "Leerzeichen" : `„${bad}“`} ist im Namen nicht erlaubt. Erlaubt sind Buchstaben, Ziffern, _ - und .`;
  return null;
}
