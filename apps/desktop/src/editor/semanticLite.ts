import { syntaxTree } from "@codemirror/language";
import { type Range } from "@codemirror/state";
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { tokensActive } from "./lsp";

/**
 * Leichte Einfärbung ohne Sprachserver: GROSSBUCHSTABEN sind Konstanten oder Makros,
 * bekannte Arduino-Objekte und CamelCase-Namen sind Typen.
 * Sobald clangd semantische Token liefert, ersetzen die den Heuristik-Teil.
 */
const KNOWN_TYPES = new Set([
  "Serial",
  "Serial0",
  "Serial1",
  "Serial2",
  "Wire",
  "Wire1",
  "SPI",
  "WiFi",
  "String",
  "Preferences",
  "SD",
  "SPIFFS",
  "LittleFS",
  "ESP",
  "Ethernet",
]);

const CONSTANT = /^[A-Z][A-Z0-9_]+$/;
const TYPE_LIKE = /^(?:[A-Z][a-z0-9]+(?:[A-Z][A-Za-z0-9]*)+|[A-Z][A-Za-z0-9]*_[A-Za-z0-9_]+)$/;

const constant = Decoration.mark({ class: "tk-constant" });
const type = Decoration.mark({ class: "tk-type" });

function classify(name: string): Decoration | null {
  if (KNOWN_TYPES.has(name)) return type;
  if (CONSTANT.test(name)) return constant;
  if (TYPE_LIKE.test(name)) return type;
  return null;
}

function build(view: EditorView): DecorationSet {
  if (view.state.field(tokensActive, false)?.size) return Decoration.none;
  const out: Range<Decoration>[] = [];
  const tree = syntaxTree(view.state);
  for (const { from, to } of view.visibleRanges) {
    tree.iterate({
      from,
      to,
      enter(node) {
        if (node.name !== "Identifier" && node.name !== "TypeIdentifier") return;
        const dec = classify(view.state.doc.sliceString(node.from, node.to));
        if (dec) out.push(dec.range(node.from, node.to));
      },
    });
  }
  return Decoration.set(out, true);
}

export const semanticLite = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (u.docChanged || u.viewportChanged || syntaxTree(u.startState) !== syntaxTree(u.state) || u.state.field(tokensActive, false)?.size !== u.startState.field(tokensActive, false)?.size) this.decorations = build(u.view);
    }
  },
  { decorations: (v) => v.decorations },
);
