// Drittanbieter-Lizenzen (Rust über cargo metadata, npm über pnpm, gebündelte Schriften)
// -> apps/desktop/src/generated/licenses.json und THIRD-PARTY-LICENSES.md
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outJson = resolve(root, "apps/desktop/src/generated/licenses.json");
const outMd = resolve(root, "THIRD-PARTY-LICENSES.md");
const run = (cmd, args) => execFileSync(cmd, args, { cwd: root, encoding: "utf8", maxBuffer: 512 * 1024 * 1024 });

const texts = new Map(); // Hash -> Text, damit gleiche Lizenztexte nur einmal gespeichert werden
function textId(text) {
  const normal = text.replace(/\r\n/g, "\n").trim();
  const id = createHash("sha256").update(normal).digest("hex").slice(0, 12);
  texts.set(id, normal);
  return id;
}

const LICENSE_FILE = /^(licen[sc]e|copying|notice|unlicense)([-_.].*)?$/i;
function licenseTexts(dir) {
  if (!dir || !existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true })
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name) && !/\.(rs|js|ts|json|toml)$/i.test(e.name))
    .map((e) => textId(readFileSync(join(dir, e.name), "utf8")));
}

// Rust: alle Pakete, die forge-desktop auf macOS in die App linkt (ohne Build- und Dev-Abhängigkeiten).
function rustPackages() {
  const meta = JSON.parse(run("cargo", ["metadata", "--format-version", "1", "--locked", "--filter-platform", "aarch64-apple-darwin"]));
  const byId = new Map(meta.packages.map((p) => [p.id, p]));
  const nodes = new Map(meta.resolve.nodes.map((n) => [n.id, n]));
  const start = meta.packages.find((p) => p.name === "forge-desktop").id;
  const seen = new Set();
  const stack = [start];
  while (stack.length) {
    const id = stack.pop();
    if (seen.has(id)) continue;
    seen.add(id);
    for (const d of nodes.get(id).deps) {
      if (d.dep_kinds.some((k) => k.kind === null)) stack.push(d.pkg);
    }
  }
  return [...seen]
    .map((id) => byId.get(id))
    .filter((p) => p.source) // eigene Crates auslassen
    .map((p) => ({
      name: p.name,
      version: p.version,
      license: p.license ?? "unbekannt",
      repository: p.repository ?? undefined,
      texts: licenseTexts(dirname(p.manifest_path)),
    }))
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

// npm: Laufzeitabhängigkeiten der App, wie pnpm sie auflöst.
function npmPackages() {
  const tree = JSON.parse(run("pnpm", ["ls", "--prod", "--depth", "Infinity", "--json", "--filter", "@forge/desktop"]));
  const found = new Map();
  const walk = (deps) => {
    for (const [name, d] of Object.entries(deps ?? {})) {
      if (d.version?.startsWith("link:") || name.startsWith("@forge/")) {
        walk(d.dependencies);
        continue;
      }
      const key = `${name}@${d.version}`;
      if (!found.has(key)) {
        found.set(key, { name, version: d.version, path: d.path });
        walk(d.dependencies);
      }
    }
  };
  for (const proj of tree) walk(proj.dependencies);
  return [...found.values()]
    .map((p) => {
      const pkg = JSON.parse(readFileSync(join(p.path, "package.json"), "utf8"));
      const license = typeof pkg.license === "string" ? pkg.license : (pkg.license?.type ?? "unbekannt");
      const repo = typeof pkg.repository === "string" ? pkg.repository : pkg.repository?.url;
      return { name: p.name, version: p.version, license, repository: repo?.replace(/^git\+/, ""), texts: licenseTexts(p.path) };
    })
    .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
}

// Schriften: Die Dateien liegen in packages/ui/src/fonts, die Lizenz (SIL OFL 1.1) im jeweiligen Fontsource-Paket.
function fontPackages() {
  const base = join(root, "packages/ui/node_modules");
  const list = [
    ["@fontsource-variable/instrument-sans", "Instrument Sans"],
    ["@fontsource-variable/martian-mono", "Martian Mono"],
    ["@fontsource/ibm-plex-mono", "IBM Plex Mono"],
  ];
  return list.map(([pkgName, family]) => {
    const dir = join(base, pkgName);
    const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
    // Die OFL-Texte liegen auch neben den Schriftdateien, damit sie mit ihnen weitergegeben werden.
    for (const id of licenseTexts(dir)) {
      const slug = family.toLowerCase().replace(/\s+/g, "-");
      writeFileSync(join(root, "packages/ui/src/fonts", `OFL-${slug}.txt`), `${texts.get(id)}\n`);
    }
    return { name: family, version: pkg.version, license: pkg.license ?? "OFL-1.1", repository: "https://fontsource.org", texts: licenseTexts(dir) };
  });
}

const data = { rust: rustPackages(), npm: npmPackages(), fonts: fontPackages() };
for (const group of Object.values(data)) {
  for (const p of group) if (!p.texts.length) console.warn(`Kein Lizenztext: ${p.name}@${p.version} (${p.license})`);
}
const out = { ...data, texts: Object.fromEntries([...texts].sort(([a], [b]) => a.localeCompare(b))) };
mkdirSync(dirname(outJson), { recursive: true });
writeFileSync(outJson, `${JSON.stringify(out)}\n`);

const md = ["# Lizenzen von Drittanbietern", "", "Erzeugt mit `node scripts/gen-licenses.mjs`. Forge selbst steht unter der MIT-Lizenz (`LICENSE`).", ""];
for (const [title, group] of [["Schriften", data.fonts], ["Rust-Crates", data.rust], ["npm-Pakete", data.npm]]) {
  md.push(`## ${title}`, "", ...group.map((p) => `- ${p.name} ${p.version} (${p.license})`), "");
}
md.push("## Lizenztexte", "");
for (const [id, text] of texts) {
  const users = [...Object.values(data)].flat().filter((p) => p.texts.includes(id));
  md.push(`### ${users.slice(0, 3).map((p) => p.name).join(", ")}${users.length > 3 ? ` und ${users.length - 3} weitere` : ""}`, "", "```text", text, "```", "");
}
writeFileSync(outMd, md.join("\n"));
console.log(`Rust: ${data.rust.length}, npm: ${data.npm.length}, Schriften: ${data.fonts.length}, Texte: ${texts.size}`);
