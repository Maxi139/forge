import { css } from "@codemirror/lang-css";
import { html } from "@codemirror/lang-html";
import { javascript } from "@codemirror/lang-javascript";
import type { Extension } from "@codemirror/state";
import { Decoration, type DecorationSet, type EditorView, MatchDecorator, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { abbreviationTracker, EmmetKnownSyntax } from "@emmetio/codemirror6-plugin";
import type { WebKind } from "../lib/web/embed";

const mark = Decoration.mark({ class: "cm-forge-placeholder" });
const matcher = new MatchDecorator({ regexp: /\{\{\s*[A-Za-z_]\w*\s*\}\}/g, decoration: () => mark });

/** Hebt `{{platzhalter}}` hervor: Sie werden im Sketch durch Werte ersetzt. */
const placeholders = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = matcher.createDeco(view);
    }
    update(u: ViewUpdate) {
      this.decorations = matcher.updateDeco(u, this.decorations);
    }
  },
  { decorations: (v) => v.decorations },
);

/**
 * Sprache und Hilfen für Webdateien: Farben, Tag-Vervollständigung, Emmet (`ul>li*3` + Tab) und Platzhalter.
 * Die Vorschläge kommen aus den Sprachpaketen selbst, nicht aus clangd.
 */
export function webExtensions(kind: WebKind): Extension[] {
  if (kind === "js") return [javascript(), placeholders];
  const emmet = abbreviationTracker({
    syntax: kind === "css" ? EmmetKnownSyntax.css : EmmetKnownSyntax.html,
    previewEnabled: false,
    markTagPairs: false,
    autoRenameTags: false,
  });
  if (kind === "css") return [css(), emmet, placeholders];
  return [html({ autoCloseTags: true, matchClosingTags: true }), emmet, placeholders];
}
