import { type Extension, StateEffect, StateField } from "@codemirror/state";
import { EditorView, GutterMarker, gutter } from "@codemirror/view";
import { ICON_PATHS } from "@forge/ui";
import { t } from "../i18n/de";

export interface AiOk {
  startLine: number;
  endLine: number;
}

interface Span {
  from: number;
  to: number;
  stale: boolean;
}

export const setAiOk = StateEffect.define<AiOk[]>();

/** Spannen der geprüften Funktionen. Sie wandern mit dem Text und werden blass, sobald darin getippt wird. */
const aiOkField = StateField.define<Span[]>({
  create: () => [],
  update(spans, tr) {
    for (const e of tr.effects) {
      if (e.is(setAiOk)) {
        const doc = tr.state.doc;
        return e.value
          .filter((b) => b.startLine >= 1 && b.endLine <= doc.lines)
          .map((b) => ({ from: doc.line(b.startLine).from, to: doc.line(b.endLine).to, stale: false }));
      }
    }
    if (!tr.docChanged) return spans;
    return spans.map((s) => {
      let touched = s.stale;
      tr.changes.iterChangedRanges((fromA, toA) => {
        if (fromA <= s.to && toA >= s.from) touched = true;
      });
      return { from: tr.changes.mapPos(s.from, 1), to: tr.changes.mapPos(s.to, -1), stale: touched };
    });
  },
});

class OkMarker extends GutterMarker {
  constructor(readonly stale: boolean) {
    super();
  }
  eq(other: OkMarker) {
    return other.stale === this.stale;
  }
  toDOM() {
    const el = document.createElement("span");
    el.className = `fg-ai-ok${this.stale ? " is-stale" : ""}`;
    el.title = t.ai.ok;
    el.setAttribute("role", "img");
    el.setAttribute("aria-label", t.ai.ok);
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("width", "12");
    svg.setAttribute("height", "12");
    svg.setAttribute("viewBox", "0 0 16 16");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.75");
    svg.setAttribute("stroke-linecap", "round");
    svg.setAttribute("stroke-linejoin", "round");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
    path.setAttribute("d", ICON_PATHS.check);
    svg.appendChild(path);
    el.appendChild(svg);
    return el;
  }
}

const fresh = new OkMarker(false);
const stale = new OkMarker(true);

const aiGutter = gutter({
  class: "fg-ai-gutter",
  lineMarker(view, line) {
    for (const s of view.state.field(aiOkField)) {
      if (s.from >= line.from && s.from <= line.to) return s.stale ? stale : fresh;
    }
    return null;
  },
  lineMarkerChange: (u) => u.docChanged || u.transactions.some((tr) => tr.effects.some((e) => e.is(setAiOk))),
  initialSpacer: () => fresh,
});

export function forgeAiMarks(): Extension {
  return [aiOkField, aiGutter, EditorView.baseTheme({})];
}
