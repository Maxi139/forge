import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { EditorView } from "@codemirror/view";
import { tags as t } from "@lezer/highlight";

/**
 * Das Editor-Theme verwendet nur CSS-Variablen aus den Forge-Tokens.
 * Dadurch wechselt der Editor mit dem Thema, ohne dass CodeMirror neu aufgebaut wird.
 */
export const forgeTheme = EditorView.theme({
  "&": {
    height: "100%",
    color: "var(--syn-variable)",
    backgroundColor: "var(--surface)",
    fontSize: "var(--fs-code)",
  },
  "&.cm-focused": { outline: "none" },
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
    lineHeight: "var(--lh-code)",
    fontVariantLigatures: "none",
    overflow: "auto",
  },
  ".cm-content": { caretColor: "var(--cursor)", padding: "8px 0 12px", tabSize: "2" },
  ".cm-line": { padding: "0 24px 0 12px" },
  ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--cursor)", borderLeftWidth: "2px", marginLeft: "-1px" },
  ".cm-selectionBackground": { background: "var(--selection) !important" },
  "&.cm-focused .cm-selectionBackground": { background: "var(--selection) !important" },
  // Halbtransparent, sonst deckt die aktive Zeile die Markierung zu.
  ".cm-activeLine": { backgroundColor: "color-mix(in srgb, var(--surface-panel) 55%, transparent)" },
  ".cm-gutters": {
    backgroundColor: "var(--surface)",
    color: "var(--ink-faint)",
    border: "none",
    fontFamily: "var(--font-mono)",
    fontSize: "12px",
  },
  ".cm-lineNumbers .cm-gutterElement": { minWidth: "40px", padding: "0 14px 0 0", fontVariantNumeric: "tabular-nums" },
  ".cm-activeLineGutter": { backgroundColor: "var(--surface-panel)", color: "var(--ink)" },
  ".cm-foldGutter .cm-gutterElement": { cursor: "pointer", color: "var(--ink-faint)", width: "16px", textAlign: "center" },
  ".cm-foldPlaceholder": {
    backgroundColor: "var(--surface-active)",
    border: "none",
    color: "var(--ink-muted)",
    borderRadius: "var(--radius-xs)",
    padding: "0 6px",
  },
  "&.cm-focused .cm-matchingBracket": {
    backgroundColor: "var(--glut-soft)",
    outline: "1px solid var(--glut)",
    borderRadius: "2px",
  },
  "&.cm-focused .cm-nonmatchingBracket": { backgroundColor: "var(--error-soft)", outline: "1px solid var(--error)" },
  ".cm-searchMatch": { backgroundColor: "var(--glut-soft)", outline: "1px solid var(--glut)", borderRadius: "2px" },
  ".cm-searchMatch.cm-searchMatch-selected": { backgroundColor: "var(--glut)", color: "var(--on-glut)" },
  ".cm-selectionMatch": { backgroundColor: "var(--surface-active)" },

  /* Tooltips, Autovervollständigung */
  ".cm-tooltip": {
    backgroundColor: "var(--surface-raised)",
    color: "var(--ink)",
    border: "1px solid var(--line)",
    borderRadius: "var(--radius-md)",
    boxShadow: "var(--shadow-pop)",
    overflow: "hidden",
    fontFamily: "var(--font-sans)",
    fontSize: "12.5px",
  },
  ".cm-tooltip-autocomplete": { minWidth: "260px" },
  ".cm-tooltip-autocomplete > ul": { fontFamily: "var(--font-mono)", maxHeight: "240px", padding: "4px" },
  ".cm-tooltip-autocomplete > ul > li": {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    height: "24px",
    padding: "0 6px",
    borderRadius: "var(--radius-xs)",
    lineHeight: "24px",
  },
  ".cm-tooltip-autocomplete > ul > li[aria-selected]": { backgroundColor: "var(--surface-active)", color: "var(--ink)" },
  ".cm-completionIcon": {
    display: "grid",
    placeItems: "center",
    width: "16px",
    height: "16px",
    margin: "0",
    padding: "0",
    borderRadius: "3px",
    backgroundColor: "var(--surface-active)",
    fontSize: "10px",
    fontWeight: "600",
    opacity: "1",
    flex: "none",
  },
  ".cm-completionIcon-function::after": { content: "'f'", color: "var(--syn-function)" },
  ".cm-completionIcon-method::after": { content: "'f'", color: "var(--syn-function)" },
  ".cm-completionIcon-constant::after": { content: "'c'", color: "var(--syn-constant)" },
  ".cm-completionIcon-class::after": { content: "'t'", color: "var(--syn-type)" },
  ".cm-completionIcon-type::after": { content: "'t'", color: "var(--syn-type)" },
  ".cm-completionIcon-keyword::after": { content: "'k'", color: "var(--syn-keyword)" },
  ".cm-completionIcon-variable::after": { content: "'v'", color: "var(--ink-muted)" },
  ".cm-completionLabel": { flex: "1", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" },
  ".cm-completionMatchedText": { textDecoration: "none", color: "var(--glut)", fontWeight: "600" },
  ".cm-completionDetail": { color: "var(--ink-faint)", fontSize: "11.5px", fontStyle: "normal", marginLeft: "0" },
  "li[aria-selected] .cm-completionDetail": { color: "var(--ink-muted)" },
  ".cm-completionInfo": {
    padding: "8px 10px",
    fontFamily: "var(--font-sans)",
    fontSize: "12px",
    lineHeight: "18px",
    color: "var(--ink-muted)",
  },

  /* Platzhalter {{name}} in Webdateien */
  ".cm-forge-placeholder": { color: "var(--glut)", backgroundColor: "var(--glut-soft)", borderRadius: "3px", padding: "0 1px" },

  /* Suchleiste */
  ".cm-panels": { backgroundColor: "var(--surface-panel)", color: "var(--ink)", borderColor: "var(--line)" },
  ".cm-panels.cm-panels-top": { borderBottom: "1px solid var(--line)" },
  ".cm-search": { padding: "6px 10px", display: "flex", flexWrap: "wrap", alignItems: "center", gap: "6px", fontFamily: "var(--font-sans)", fontSize: "12px" },
  ".cm-search label": { color: "var(--ink-muted)", display: "inline-flex", alignItems: "center", gap: "4px" },
  ".cm-search input[type=text], .cm-textfield": {
    height: "24px",
    boxSizing: "border-box",
    padding: "0 8px",
    background: "var(--surface)",
    color: "var(--ink)",
    border: "1px solid var(--line-control)",
    borderRadius: "var(--radius-sm)",
    font: "inherit",
  },
  ".cm-search input[type=text]:focus": { outline: "1px solid var(--focus)", borderColor: "var(--focus)" },
  ".cm-button": {
    height: "24px",
    padding: "0 10px",
    background: "var(--surface-raised)",
    color: "var(--ink)",
    border: "1px solid var(--line-strong)",
    borderRadius: "var(--radius-sm)",
    backgroundImage: "none",
    font: "inherit",
    cursor: "pointer",
  },
  ".cm-button:hover": { background: "var(--surface-hover)" },
  ".cm-search [name=close]": { color: "var(--ink-muted)", background: "transparent", border: "none", cursor: "pointer", fontSize: "16px" },
});

const hs = HighlightStyle.define([
  { tag: [t.definitionKeyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.keyword, t.standard(t.typeName)], color: "var(--syn-keyword)" },
  { tag: [t.typeName, t.namespace, t.className], color: "var(--syn-type)" },
  { tag: [t.function(t.variableName), t.function(t.definition(t.variableName)), t.function(t.propertyName)], color: "var(--syn-function)" },
  { tag: [t.string, t.character, t.special(t.string)], color: "var(--syn-string)" },
  { tag: [t.number, t.integer, t.float], color: "var(--syn-number)" },
  { tag: [t.null, t.bool, t.escape, t.constant(t.variableName)], color: "var(--syn-constant)" },
  { tag: [t.processingInstruction, t.meta, t.special(t.name)], color: "var(--syn-macro)" },
  { tag: [t.lineComment, t.blockComment, t.comment], color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: [t.operator, t.punctuation, t.separator, t.paren, t.squareBracket, t.brace, t.angleBracket, t.derefOperator], color: "var(--syn-punct)" },
  { tag: t.invalid, color: "var(--error)" },
  // HTML und CSS
  { tag: [t.tagName, t.standard(t.tagName)], color: "var(--syn-keyword)" },
  { tag: [t.attributeName, t.propertyName], color: "var(--syn-type)" },
  { tag: [t.attributeValue], color: "var(--syn-string)" },
  { tag: [t.color, t.unit, t.labelName], color: "var(--syn-constant)" },
  { tag: [t.documentMeta, t.url], color: "var(--syn-macro)" },
]);

export const forgeHighlighting = syntaxHighlighting(hs);
