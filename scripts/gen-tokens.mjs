// tokens.json (Designsystem) -> packages/ui/src/styles/tokens.css
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const src = resolve(root, "packages/ui/design-source/tokens.json");
const out = resolve(root, "packages/ui/src/styles/tokens.css");
const t = JSON.parse(readFileSync(src, "utf8"));

const themes = t.color.themes.map((x) => x.id);
const primary = themes[0];
const alias = (v) => v.replace(/^\{(.+)\}$/, "var(--$1)");
const val = (v, theme) => alias(typeof v === "string" ? v : (v[theme] ?? v[primary]));

const colorVars = (theme) =>
  t.color.tokens.map((k) => `  --${k.name}: ${val(k.value, theme)};`).join("\n");
const shadowVars = (theme) =>
  (t.shadow?.tokens ?? []).map((k) => `  --${k.name}: ${val(k.value, theme)};`).join("\n");

const flat = (family) => (t[family]?.tokens ?? []).map((k) => `  --${k.name}: ${k.value};`).join("\n");

const fonts = Object.entries(t.type.families)
  .map(([k, v]) => `  --font-${k}: ${v};`)
  .join("\n");
const textVars = t.type.groups
  .flatMap((g) => g.styles)
  .map((s) => {
    const lines = [`  --fs-${s.name}: ${s.fontSize};`, `  --lh-${s.name}: ${s.lineHeight};`, `  --fw-${s.name}: ${s.fontWeight};`];
    if (s.letterSpacing) lines.push(`  --ls-${s.name}: ${s.letterSpacing};`);
    return lines.join("\n");
  })
  .join("\n");

const dark = `${colorVars("dark")}\n${shadowVars("dark")}\n  color-scheme: dark;`;
const light = `${colorVars("light")}\n${shadowVars("light")}\n  color-scheme: light;`;

const css = `/* Forge Tokens. Erzeugt aus design-source/tokens.json. Nicht von Hand ändern (pnpm tokens). */
:root {
${fonts}
${textVars}
${flat("spacing")}
${flat("radius")}
${flat("size")}
}

/* Dark ist der Standard. Hell folgt dem System, solange kein Thema gewählt ist. */
:root,
:root[data-theme="dark"] {
${dark}
}

@media (prefers-color-scheme: light) {
  :root:not([data-theme="dark"]) {
${light.replace(/^/gm, "  ")}
  }
}

:root[data-theme="light"] {
${light}
}
`;
writeFileSync(out, css);
console.log(`tokens.css: ${css.split("\n").length} Zeilen`);
