// Die Version steht in Cargo.toml ([workspace.package]). Dieses Skript schreibt sie in
// package.json (Wurzel und App) und tauri.conf.json.
//   node scripts/sync-version.mjs          gleicht ab
//   node scripts/sync-version.mjs 0.2.0    setzt eine neue Version und gleicht ab
//   node scripts/sync-version.mjs --check  prüft nur (Exitcode 1 bei Abweichung)
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const path = (p) => resolve(root, p);
const arg = process.argv[2];
const check = arg === "--check";

const cargoPath = path("Cargo.toml");
let cargo = readFileSync(cargoPath, "utf8");
const block = /(\[workspace\.package\][^[]*?\nversion = ")([^"]+)(")/;
if (!block.test(cargo)) throw new Error("Keine Version in [workspace.package] von Cargo.toml");

if (arg && !check) {
  if (!/^\d+\.\d+\.\d+(-[\w.]+)?$/.test(arg)) throw new Error(`Ungültige Version: ${arg}`);
  cargo = cargo.replace(block, `$1${arg}$3`);
  writeFileSync(cargoPath, cargo);
}
const version = cargo.match(block)[2];

// Nur die Zeile "version" ersetzen, damit die Formatierung der Dateien bleibt.
const targets = [
  ["package.json", /("version":\s*")([^"]+)(")/],
  ["apps/desktop/package.json", /("version":\s*")([^"]+)(")/],
  ["apps/desktop/src-tauri/tauri.conf.json", /("version":\s*")([^"]+)(")/],
];
let stale = 0;
for (const [file, re] of targets) {
  const text = readFileSync(path(file), "utf8");
  const current = text.match(re)?.[2];
  if (current === version) continue;
  stale++;
  if (check) console.error(`${file}: ${current} statt ${version}`);
  else {
    writeFileSync(path(file), text.replace(re, `$1${version}$3`));
    console.log(`${file}: ${current} -> ${version}`);
  }
}
if (check && stale) {
  console.error("Versionen weichen ab. `node scripts/sync-version.mjs` gleicht sie an.");
  process.exit(1);
}
console.log(`Version ${version}`);
