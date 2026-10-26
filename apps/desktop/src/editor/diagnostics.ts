import { type Extension, type Range, StateEffect, StateField } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, GutterMarker, gutter, hoverTooltip, keymap, ViewPlugin, type ViewUpdate, WidgetType } from "@codemirror/view";
import { ICON_PATHS, type IconName } from "@forge/ui";
import type { Diagnostic, Severity, TextEdit } from "../lib/diagnostics";
import { bySeverity } from "../lib/diagnostics";

const SVG_NS = "http://www.w3.org/2000/svg";

function iconEl(name: IconName, size = 14): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "fg-icon");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.5");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("d", ICON_PATHS[name]);
  svg.appendChild(path);
  return svg;
}

const SEV_ICON: Record<Severity, IconName> = { error: "error", warning: "warning", info: "info" };

export const setDiagnostics = StateEffect.define<Diagnostic[]>();

interface DiagState {
  list: Diagnostic[];
  marks: DecorationSet;
  inline: DecorationSet;
}

class InlineDiagnostic extends WidgetType {
  constructor(
    readonly message: string,
    readonly severity: Severity,
  ) {
    super();
  }
  eq(other: InlineDiagnostic) {
    return other.message === this.message && other.severity === this.severity;
  }
  toDOM() {
    // Der Anker ist breitenlos, damit der Cursor direkt hinter dem Zeilentext sitzt. Die Meldung hängt daneben.
    const anchor = document.createElement("span");
    anchor.className = "fg-diag-anchor";
    const el = document.createElement("span");
    el.className = `fg-diag fg-diag--${this.severity}`;
    el.appendChild(iconEl(SEV_ICON[this.severity], 12));
    const text = document.createElement("span");
    text.className = "fg-diag-text";
    text.textContent = this.message;
    el.appendChild(text);
    anchor.appendChild(el);
    return anchor;
  }
  ignoreEvent() {
    return true;
  }
}

function build(view: { doc: import("@codemirror/state").Text }, list: Diagnostic[]): DiagState {
  const { doc } = view;
  // Lücken am Zeilenende verschieben den Start nach vorn. Darum sortiert CodeMirror selbst, statt dass wir Reihenfolgen zusichern.
  const marks: Range<Decoration>[] = [];
  const inline: Range<Decoration>[] = [];
  const sorted = [...list].filter((d) => d.line >= 1 && d.line <= doc.lines).sort((a, b) => doc.line(a.line).from + a.from - (doc.line(b.line).from + b.from));

  const worstPerLine = new Map<number, Diagnostic>();
  for (const d of [...sorted].sort(bySeverity)) if (d.source !== "ai" && !worstPerLine.has(d.line)) worstPerLine.set(d.line, d);

  for (const d of sorted) {
    const line = doc.line(d.line);
    let from = Math.min(line.from + d.from, line.to);
    let to = Math.min(line.from + Math.max(d.to, d.from), line.to);
    if (to <= from) {
      // Leere Spanne: ein Zeichen markieren, am Zeilenende das davor.
      if (from < line.to) to = from + 1;
      else if (from > line.from) from = to - 1;
      else to = Math.min(line.to + 0, from);
    }
    if (to > from) {
      const cls = d.source === "ai" ? `fg-sq fg-sq--ai fg-sq--ai-${d.severity}` : `fg-sq fg-sq--${d.severity}`;
      marks.push(Decoration.mark({ class: cls, diag: d } as never).range(from, to));
    }
  }

  // Inline-Meldung am Zeilenende: nur für Fehler und Warnungen, nicht für leise Hinweise der KI.
  const lines = [...worstPerLine.values()].filter((d) => d.severity !== "info" && d.source !== "ai").sort((a, b) => a.line - b.line);
  for (const d of lines) {
    const line = doc.line(d.line);
    inline.push(Decoration.widget({ widget: new InlineDiagnostic(d.message, d.severity), side: 1 }).range(line.to));
  }
  return { list: sorted, marks: Decoration.set(marks, true), inline: Decoration.set(inline, true) };
}

/** Wie lange nach der letzten Eingabe die Zeile am Cursor still bleibt. */
const TYPING_IDLE_MS = 2500;

const setTyping = StateEffect.define<number | null>();

/** Zeilennummer, in der gerade getippt wird. Dort erscheinen noch keine Meldungen. */
const typingLine = StateField.define<number | null>({
  create: () => null,
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setTyping)) return e.value;
    const line = tr.state.doc.lineAt(tr.state.selection.main.head).number;
    if (tr.docChanged) return tr.isUserEvent("input") || tr.isUserEvent("delete") ? line : null;
    if (tr.selection && value !== null && value !== line) return null;
    return value;
  },
});

const typingIdle = ViewPlugin.fromClass(
  class {
    timer: ReturnType<typeof setTimeout> | undefined;
    constructor(readonly view: EditorView) {}
    update(u: ViewUpdate) {
      if (u.state.field(typingLine) === null) {
        clearTimeout(this.timer);
      } else if (u.docChanged) {
        clearTimeout(this.timer);
        this.timer = setTimeout(() => this.view.dispatch({ effects: setTyping.of(null) }), TYPING_IDLE_MS);
      }
    }
    destroy() {
      clearTimeout(this.timer);
    }
  },
);

function withoutLine(set: DecorationSet, state: import("@codemirror/state").EditorState, n: number | null): DecorationSet {
  if (n === null || n < 1 || n > state.doc.lines) return set;
  const { from, to } = state.doc.line(n);
  return set.update({ filter: (f, t) => t < from || f > to });
}

export const diagnosticsField = StateField.define<DiagState>({
  create: () => ({ list: [], marks: Decoration.none, inline: Decoration.none }),
  update(value, tr) {
    for (const e of tr.effects) if (e.is(setDiagnostics)) return build({ doc: tr.state.doc }, e.value);
    if (tr.docChanged) return { list: value.list, marks: value.marks.map(tr.changes), inline: value.inline.map(tr.changes) };
    return value;
  },
  provide: (f) => [
    EditorView.decorations.compute([f, typingLine], (state) => withoutLine(state.field(f).marks, state, state.field(typingLine))),
    EditorView.decorations.compute([f, typingLine], (state) => withoutLine(state.field(f).inline, state, state.field(typingLine))),
  ],
});

class SeverityMarker extends GutterMarker {
  constructor(readonly severity: Severity) {
    super();
  }
  eq(other: SeverityMarker) {
    return other.severity === this.severity;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `fg-gutter-diag is-${this.severity}`;
    el.appendChild(iconEl(SEV_ICON[this.severity], 14));
    return el;
  }
}

const markers = {
  error: new SeverityMarker("error"),
  warning: new SeverityMarker("warning"),
  info: new SeverityMarker("info"),
};

/** Fehler- und Warnsymbole links neben den Zeilennummern. Hinweise der KI bleiben außen vor. */
const diagnosticsGutter = gutter({
  class: "fg-diag-gutter",
  lineMarker(view, line) {
    const { marks } = view.state.field(diagnosticsField);
    if (view.state.doc.lineAt(line.from).number === view.state.field(typingLine)) return null;
    let worst: Severity | null = null;
    marks.between(line.from, line.to, (_f, _t, value) => {
      const diag = (value.spec as { diag?: Diagnostic }).diag;
      const sev = diag?.source === "ai" ? undefined : diag?.severity;
      if (sev === "error" || (sev === "warning" && worst !== "error")) worst = sev;
    });
    return worst ? markers[worst] : null;
  },
  lineMarkerChange: (u) => u.docChanged || u.transactions.some((tr) => tr.effects.some((e) => e.is(setDiagnostics) || e.is(setTyping))) || u.startState.field(typingLine) !== u.state.field(typingLine),
  initialSpacer: () => markers.error,
});

function diagnosticsAt(view: EditorView, pos: number): Diagnostic[] {
  const out: Diagnostic[] = [];
  view.state.field(diagnosticsField).marks.between(pos, pos, (from, to, value) => {
    const d = (value.spec as { diag?: Diagnostic }).diag;
    if (d && pos >= from && pos <= to) out.push(d);
  });
  return out.sort(bySeverity);
}

const SOURCE_LABEL: Record<Diagnostic["source"], string> = {
  syntax: "Syntax",
  compiler: "Compiler",
  clangd: "clangd",
  analysis: "Analyse",
  ai: "KI",
  web: "Web",
};

/** Eine angenommene Behebung erledigt ihre Meldung sofort, ohne den nächsten Lauf abzuwarten. */
function fixed(d: Diagnostic) {
  window.dispatchEvent(new CustomEvent("forge:fix-applied", { detail: d }));
}

export function applyEdits(view: EditorView, edits: TextEdit[]) {
  const doc = view.state.doc;
  const changes = edits
    .filter((e) => e.fromLine >= 1 && e.toLine <= doc.lines)
    .map((e) => ({
      from: Math.min(doc.line(e.fromLine).from + e.fromCol, doc.line(e.fromLine).to),
      to: Math.min(doc.line(e.toLine).from + e.toCol, doc.line(e.toLine).to),
      insert: e.text,
    }));
  if (changes.length) view.dispatch({ changes, userEvent: "input.fix" });
}

/** Hover: Meldung, Quelle und, falls vorhanden, die Behebung. */
function hoverInfo(): Extension {
  return hoverTooltip(
    (view, pos) => {
      const found = diagnosticsAt(view, pos);
      if (!found.length) return null;
      return {
        pos,
        above: true,
        create() {
          const dom = document.createElement("div");
          dom.className = "fg-hover";
          for (const d of found) {
            const row = document.createElement("div");
            row.className = `fg-hover-row is-${d.severity}`;
            row.appendChild(iconEl(SEV_ICON[d.severity], 14));
            const body = document.createElement("div");
            const msg = document.createElement("div");
            msg.className = "fg-hover-msg";
            msg.textContent = d.message;
            const meta = document.createElement("div");
            meta.className = "fg-hover-meta";
            meta.textContent = SOURCE_LABEL[d.source];
            body.append(msg, meta);
            if (d.source === "ai") {
              const talk = document.createElement("button");
              talk.type = "button";
              talk.className = "fg-btn fg-btn--secondary fg-btn--sm fg-hover-fix";
              talk.textContent = "Besprechen";
              talk.addEventListener("mousedown", (e) => {
                e.preventDefault();
                window.dispatchEvent(new CustomEvent("forge:ai-thread", { detail: d.id.slice(3) }));
              });
              body.appendChild(talk);
            }
            if (d.source === "ai" && !d.fix) {
              const auto = document.createElement("button");
              auto.type = "button";
              auto.className = "fg-btn fg-btn--secondary fg-btn--sm fg-hover-fix";
              auto.textContent = "Beheben";
              auto.title = "Die KI behebt den Hinweis. ⌘Z nimmt es zurück.";
              auto.addEventListener("mousedown", (e) => {
                e.preventDefault();
                auto.disabled = true;
                auto.classList.add("is-busy");
                auto.setAttribute("aria-busy", "true");
                auto.textContent = "Behebe\u00a0…";
                window.dispatchEvent(new CustomEvent("forge:ai-fix", { detail: d }));
              });
              body.appendChild(auto);
            }
            if (d.fix) {
              const btn = document.createElement("button");
              btn.type = "button";
              btn.className = "fg-btn fg-btn--secondary fg-btn--sm fg-hover-fix";
              btn.textContent = d.fix.label;
              btn.addEventListener("mousedown", (e) => {
                e.preventDefault();
                applyEdits(view, d.fix!.edits);
                fixed(d);
              });
              body.appendChild(btn);
            }
            if (d.action) {
              const btn = document.createElement("button");
              btn.type = "button";
              btn.className = "fg-btn fg-btn--secondary fg-btn--sm fg-hover-fix";
              btn.textContent = d.action.label;
              const action = d.action;
              btn.addEventListener("mousedown", (e) => {
                e.preventDefault();
                window.dispatchEvent(new CustomEvent("forge:diagnostic-action", { detail: { action, diag: d } }));
              });
              body.appendChild(btn);
            }
            row.appendChild(body);
            dom.appendChild(row);
          }
          return { dom };
        },
      };
    },
    { hoverTime: 250 },
  );
}

/** ⌘. wendet die Behebung der Meldung am Cursor an. */
const quickFix = keymap.of([
  {
    key: "Mod-.",
    run(view) {
      const head = view.state.selection.main.head;
      const line = view.state.doc.lineAt(head);
      const candidates = view.state
        .field(diagnosticsField)
        .list.filter((d) => d.fix && d.line === line.number)
        .sort(bySeverity);
      const fix = candidates[0]?.fix;
      if (!fix) return false;
      applyEdits(view, fix.edits);
      fixed(candidates[0]);
      return true;
    },
  },
]);

export function forgeDiagnostics(): Extension {
  return [typingLine, typingIdle, diagnosticsField, diagnosticsGutter, hoverInfo(), quickFix];
}
