import { t } from "../../i18n/de";
import { backend } from "../backend";
import { errorText, useStore } from "../store";
import { flattenTree, toRel } from "./sync";

const INDEX = `<!doctype html>
<html lang="de">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>ESP32</title>
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <main>
    <h1>ESP32</h1>
    <p class="card">Messwert: <strong id="wert">{{wert}}</strong></p>
    <button id="knopf" type="button">LED umschalten</button>
  </main>
  <script src="app.js"></script>
</body>
</html>
`;

const STYLE = `:root {
  color-scheme: light dark;
  --akzent: #ff6a2b;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  font: 16px/1.5 system-ui, sans-serif;
}

main {
  max-width: 28rem;
  margin: 0 auto;
  padding: 1.5rem 1rem;
}

.card {
  padding: 1rem;
  border: 1px solid color-mix(in srgb, currentColor 20%, transparent);
  border-radius: 0.75rem;
}

button {
  width: 100%;
  padding: 0.9rem;
  border: 0;
  border-radius: 0.75rem;
  background: var(--akzent);
  color: #fff;
  font: inherit;
  font-weight: 600;
}
`;

const SCRIPT = `// Der Sketch kennt die Adresse /led und schaltet die LED um.
document.getElementById("knopf").addEventListener("click", async () => {
  try {
    await fetch("/led", { method: "POST" });
  } catch (e) {
    console.error("Gerät nicht erreichbar", e);
  }
});
`;

const FILES: Array<[string, string]> = [
  ["web/index.html", INDEX],
  ["web/style.css", STYLE],
  ["web/app.js", SCRIPT],
];

/** Legt `web/index.html`, `style.css` und `app.js` an und öffnet die Seite. Vorhandene Dateien bleiben unberührt. */
export async function createWebStarter(): Promise<void> {
  const s = useStore.getState();
  if (!s.project) return;
  const existing = new Set(flattenTree(s.tree).map(toRel));
  if (existing.has("web/index.html")) {
    s.toast({ kind: "info", title: t.web.createExists, ttl: 4000 });
    await s.openFile(`${s.project.path}/web/index.html`);
    return;
  }
  try {
    for (const [rel, text] of FILES) {
      if (existing.has(rel)) continue;
      const abs = `${s.project.path}/${rel}`;
      await backend().createFile(abs, false);
      await backend().writeFile(abs, text);
    }
    await s.refreshTree();
    await s.openFile(`${s.project.path}/web/index.html`);
    useStore.getState().patchLayout({ previewOpen: true });
    s.toast({
      kind: "success",
      title: t.web.created,
      message: t.web.createdText,
      ttl: 8000,
      actions: [{ label: t.web.embed, primary: true, run: () => useStore.getState().setDialog({ kind: "webEmbed", source: "web/index.html" }) }],
    });
  } catch (e) {
    s.toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
  }
}
