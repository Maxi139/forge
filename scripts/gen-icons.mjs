// Icon-Pfade aus dem Designsystem-Bundle -> packages/ui/src/icons.ts und assets/icons/*.svg
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundle = readFileSync(resolve(root, "packages/ui/design-source/components/bundle.js"), "utf8");
const m = bundle.match(/var ICONS = (\{[\s\S]*?\n  \});/);
if (!m) throw new Error("ICONS nicht gefunden");
const icons = Function(`"use strict"; return (${m[1]})`)();
const names = Object.keys(icons);

const ts = `// Erzeugt von scripts/gen-icons.mjs aus dem Forge-Designsystem. 16-px-Raster, 1.5 px Strich.
export const ICON_PATHS = ${JSON.stringify(icons, null, 2)} as const;

export type IconName = ${names.map((n) => `"${n}"`).join(" | ")};
export const ICON_NAMES = Object.keys(ICON_PATHS) as IconName[];
`;
writeFileSync(resolve(root, "packages/ui/src/icons.ts"), ts);

const dir = resolve(root, "packages/ui/assets/icons");
mkdirSync(dir, { recursive: true });
for (const [name, d] of Object.entries(icons)) {
  const sw = name === "more" ? 2 : 1.5;
  writeFileSync(
    resolve(dir, `${name}.svg`),
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"><path d="${d}"/></svg>\n`,
  );
}
console.log(`${names.length} Icons`);
