import { type Completion, type CompletionContext, type CompletionResult, snippet } from "@codemirror/autocomplete";
import { type Extension, type Range, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, hoverTooltip, keymap, showTooltip, type Tooltip, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { backend, type LspCompletion, type LspLocation, type LspSignature, type LspTokens } from "../lib/backend";
import { useLsp } from "../lib/lsp";
import { arduinoCompletions } from "./arduinoCompletions";
import { diagnosticsField } from "./diagnostics";

const KIND: Record<number, string> = { 2: "method", 3: "function", 4: "class", 5: "property", 6: "variable", 7: "class", 8: "class", 9: "namespace", 10: "property", 12: "constant", 13: "type", 14: "keyword", 20: "constant", 21: "constant", 22: "type", 25: "type" };

function withTimeout<T>(p: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const id = setTimeout(() => reject(new Error("Zeitüberschreitung")), ms);
    p.then(
      (v) => {
        clearTimeout(id);
        resolve(v);
      },
      (e) => {
        clearTimeout(id);
        reject(e);
      },
    );
  });
}

/** clangd muss den Text sehen, auf den sich die Frage bezieht. Sonst gleichen wir sofort ab. */
async function upToDate(file: string, view: { state: { doc: { toString(): string } } }) {
  const lsp = useLsp.getState();
  if (lsp.texts[file] === view.state.doc.toString()) return;
  try {
    await withTimeout(lsp.sync(), 4000);
  } catch {
    /* Dann antwortet clangd mit dem alten Stand oder gar nicht. */
  }
}

/** LSP-Snippets kennen `$1` und `${0}`, CodeMirror nur `${1}` und `${1:name}`. */
function lspSnippet(text: string): string {
  return text.replace(/(?<!\\)\$(\d+)/g, "${$1}");
}

/** Vorschläge von clangd. Fehlt er oder antwortet zu langsam, gilt die eingebaute Liste. */
export function lspCompletions(file: () => string) {
  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    if (!useLsp.getState().ready) return arduinoCompletions(context);
    const word = context.matchBefore(/[A-Za-z_]\w*/);
    const line = context.state.doc.lineAt(context.pos);
    const prev = context.state.sliceDoc(Math.max(line.from, context.pos - 2), context.pos);
    const afterAccess = prev.endsWith(".") || prev.endsWith("->") || prev.endsWith("::");
    if (!context.explicit && !word && !afterAccess) return null;
    let items: LspCompletion[];
    try {
      await upToDate(file(), { state: context.state });
      if (context.aborted) return null;
      items = await withTimeout(backend().lspComplete(file(), line.number, context.pos - line.from), 2500);
    } catch {
      return arduinoCompletions(context);
    }
    if (context.aborted) return null;
    if (!items.length) return word || context.explicit ? arduinoCompletions(context) : null;
    const start = items.find((i) => i.from != null)?.from;
    const from = start != null ? line.from + start : (word?.from ?? context.pos);
    const options: Completion[] = items.map((i, idx) => ({
      label: i.label,
      type: KIND[i.kind] ?? "variable",
      detail: i.detail ?? undefined,
      info: i.doc ?? undefined,
      apply: i.snippet && /\$/.test(i.insert) ? snippet(lspSnippet(i.insert)) : i.insert !== i.label ? i.insert : undefined,
      boost: 50 - Math.min(idx, 99) * 0.5,
    }));
    return { from, options, validFor: /^\w*$/ };
  };
}

/* ---------- Hover ---------- */

function hoverDom(text: string): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "fg-lsp-hover";
  const parts = text.split(/\n\s*\n/).map((p) => p.trim()).filter(Boolean);
  // Der letzte Absatz ist bei clangd die Deklaration.
  const code = parts.length > 1 && /[(;{]|\w\s+\w/.test(parts[parts.length - 1]) ? parts.pop() : undefined;
  if (code) {
    const pre = document.createElement("pre");
    pre.className = "fg-lsp-code";
    pre.textContent = code;
    dom.appendChild(pre);
  }
  for (const p of parts) {
    const el = document.createElement("div");
    el.className = "fg-lsp-doc";
    el.textContent = p;
    dom.appendChild(el);
  }
  return dom;
}

function lspHover(file: () => string): Extension {
  return hoverTooltip(
    async (view, pos) => {
      if (!useLsp.getState().ready) return null;
      // Wo eine Meldung liegt, zeigt der Hover die Meldung.
      let hasDiag = false;
      view.state.field(diagnosticsField).marks.between(pos, pos, () => {
        hasDiag = true;
      });
      if (hasDiag) return null;
      const line = view.state.doc.lineAt(pos);
      let hover;
      try {
        await upToDate(file(), view);
        hover = await withTimeout(backend().lspHover(file(), line.number, pos - line.from), 2500);
      } catch {
        return null;
      }
      if (!hover?.text) return null;
      const from = line.from + hover.from;
      const to = Math.max(line.from + hover.to, from);
      return { pos: from, end: to, above: true, create: () => ({ dom: hoverDom(hover.text) }) };
    },
    { hoverTime: 350 },
  );
}

/* ---------- Definition ---------- */

function definitionAt(view: EditorView, pos: number, file: () => string, goto: (l: LspLocation) => void) {
  const line = view.state.doc.lineAt(pos);
  void upToDate(file(), view)
    .then(() => backend().lspDefinition(file(), line.number, pos - line.from))
    .then((list) => list[0] && goto(list[0]))
    .catch(() => undefined);
}

function lspDefinition(file: () => string, goto: (l: LspLocation) => void): Extension {
  return [
    EditorView.domEventHandlers({
      mousedown(e, view) {
        if (!(e.metaKey || e.ctrlKey) || e.button !== 0 || !useLsp.getState().ready) return false;
        const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
        if (pos == null) return false;
        e.preventDefault();
        definitionAt(view, pos, file, goto);
        return true;
      },
    }),
    keymap.of([
      {
        key: "F12",
        run(view) {
          if (!useLsp.getState().ready) return false;
          definitionAt(view, view.state.selection.main.head, file, goto);
          return true;
        },
      },
    ]),
  ];
}

/* ---------- Signaturhilfe ---------- */

const setSignature = StateEffect.define<Tooltip | null>();

const signatureField = StateField.define<Tooltip | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setSignature)) return e.value;
    if (value && tr.docChanged) return { ...value, pos: tr.changes.mapPos(value.pos) };
    return value;
  },
  provide: (f) => showTooltip.from(f),
});

function signatureDom(sig: LspSignature): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "fg-lsp-sig";
  const code = document.createElement("div");
  code.className = "fg-lsp-code";
  const range = sig.params[sig.active];
  if (range) {
    const [a, b] = range;
    code.append(sig.label.slice(0, a));
    const strong = document.createElement("b");
    strong.textContent = sig.label.slice(a, b);
    code.append(strong, sig.label.slice(b));
  } else {
    code.textContent = sig.label;
  }
  dom.appendChild(code);
  if (sig.doc) {
    const doc = document.createElement("div");
    doc.className = "fg-lsp-doc";
    doc.textContent = sig.doc;
    dom.appendChild(doc);
  }
  return dom;
}

function lspSignature(file: () => string): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      open = false;
      constructor(readonly view: EditorView) {}
      update(u: ViewUpdate) {
        if (u.docChanged) {
          let typed = "";
          u.changes.iterChanges((_fa, _ta, _fb, _tb, ins) => {
            if (ins.length === 1 || ins.toString().length === 1) typed = ins.toString();
          });
          if (typed === "(" || typed === ",") this.schedule();
          else if (typed === ")") this.close();
          else if (this.open) this.schedule();
        } else if (u.selectionSet && this.open) this.schedule();
        if (u.focusChanged && !u.view.hasFocus) this.close();
      }
      schedule() {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.ask(), 120);
      }
      close() {
        clearTimeout(this.timer);
        if (this.open) {
          this.open = false;
          this.view.dispatch({ effects: setSignature.of(null) });
        }
      }
      async ask() {
        if (!useLsp.getState().ready) return;
        const pos = this.view.state.selection.main.head;
        const line = this.view.state.doc.lineAt(pos);
        let sig: LspSignature | null = null;
        try {
          await upToDate(file(), this.view);
          sig = await withTimeout(backend().lspSignature(file(), line.number, pos - line.from), 2500);
        } catch {
          sig = null;
        }
        if (this.view.state.selection.main.head !== pos) return;
        if (!sig) return this.close();
        this.open = true;
        const tip: Tooltip = { pos, above: true, strictSide: true, create: () => ({ dom: signatureDom(sig!) }) };
        this.view.dispatch({ effects: setSignature.of(tip) });
      }
      destroy() {
        clearTimeout(this.timer);
      }
    },
  );
  return [
    signatureField,
    plugin,
    keymap.of([
      {
        key: "Escape",
        run(view) {
          if (!view.state.field(signatureField)) return false;
          view.dispatch({ effects: setSignature.of(null) });
          return true;
        },
      },
    ]),
  ];
}

/* ---------- Semantische Farben ---------- */

interface TokenRange {
  from: number;
  to: number;
  cls: string;
}

const setTokens = StateEffect.define<TokenRange[]>();

/** Ob clangd-Farben gelten. Dann tritt die leichte Heuristik zurück. */
export const tokensActive = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(value, tr) {
    for (const e of tr.effects) {
      if (e.is(setTokens)) {
        const ranges: Range<Decoration>[] = e.value.map((r) => Decoration.mark({ class: r.cls }).range(r.from, r.to));
        return Decoration.set(ranges, true);
      }
    }
    return tr.docChanged ? value.map(tr.changes) : value;
  },
  provide: (f) => EditorView.decorations.from(f),
});

const TYPE_CLASS: Record<string, string> = {
  function: "fn",
  method: "fn",
  class: "type",
  enum: "type",
  type: "type",
  typeParameter: "type",
  concept: "type",
  namespace: "type",
  enumMember: "const",
  macro: "macro",
  comment: "inactive",
  parameter: "param",
  property: "prop",
};

function tokenClass(tokens: LspTokens, kind: number, mods: number): string | null {
  const base = TYPE_CLASS[tokens.types[kind] ?? ""];
  if (!base) return null;
  const names = tokens.modifiers.filter((_, i) => mods & (1 << i));
  return `tk-lsp tk-lsp-${base}${names.includes("deprecated") ? " tk-lsp-deprecated" : ""}`;
}

function lspTokens(file: () => string): Extension {
  const plugin = ViewPlugin.fromClass(
    class {
      timer: ReturnType<typeof setTimeout> | undefined;
      off: () => void;
      constructor(readonly view: EditorView) {
        this.off = useLsp.subscribe((s, prev) => {
          if (s.synced !== prev.synced || s.ready !== prev.ready) this.schedule(150);
        });
        this.schedule(300);
      }
      update(u: ViewUpdate) {
        void u;
      }
      schedule(ms: number) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => void this.refresh(), ms);
      }
      async refresh() {
        const lsp = useLsp.getState();
        const f = file();
        if (!lsp.ready) {
          if (this.view.state.field(tokensActive).size) this.view.dispatch({ effects: setTokens.of([]) });
          return;
        }
        let res: LspTokens | null = null;
        try {
          res = await withTimeout(backend().lspTokens(f), 4000);
        } catch {
          return;
        }
        if (!res) return;
        const doc = this.view.state.doc;
        // Nur anwenden, wenn der Text noch der ist, den clangd gesehen hat.
        const seen = useLsp.getState().texts[f];
        if (seen !== undefined && seen !== doc.toString()) return;
        const out: TokenRange[] = [];
        for (const t of res.tokens) {
          if (t.line < 1 || t.line > doc.lines) continue;
          const line = doc.line(t.line);
          const from = line.from + t.col;
          const to = Math.min(from + t.len, line.to);
          const cls = tokenClass(res, t.kind, t.mods);
          if (cls && to > from) out.push({ from, to, cls });
        }
        this.view.dispatch({ effects: setTokens.of(out) });
      }
      destroy() {
        clearTimeout(this.timer);
        this.off();
      }
    },
  );
  return [tokensActive, plugin];
}

export function forgeLsp(opts: { file: () => string; goto: (l: LspLocation) => void }): Extension {
  return [lspHover(opts.file), lspDefinition(opts.file, opts.goto), lspSignature(opts.file), lspTokens(opts.file)];
}
