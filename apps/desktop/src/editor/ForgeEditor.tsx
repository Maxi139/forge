import { acceptCompletion, autocompletion, closeBrackets, closeBracketsKeymap, completionKeymap } from "@codemirror/autocomplete";
import { addCursorAbove, addCursorBelow, defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { cpp } from "@codemirror/lang-cpp";
import { bracketMatching, foldGutter, foldKeymap, indentOnInput } from "@codemirror/language";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, type Extension, EditorState } from "@codemirror/state";
import {
  crosshairCursor,
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  rectangularSelection,
} from "@codemirror/view";
import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { germanPhrases } from "./phrases";
import { editorSettingsExtension, useEditorSettings } from "./settings";
import type { LspLocation } from "../lib/backend";
import type { Diagnostic } from "../lib/diagnostics";
import { forgeLsp, lspCompletions } from "./lsp";
import { type AiOk, forgeAiMarks, setAiOk } from "./aiMarks";
import { forgeDiagnostics, setDiagnostics } from "./diagnostics";
import { forgeHighlighting, forgeTheme } from "./forgeTheme";
import { semanticLite } from "./semanticLite";

/**
 * Die App läuft mit strenger CSP. Tauri hängt den Nonce an seine eigenen Tags, und CodeMirror braucht ihn
 * für die Stile, die es zur Laufzeit einfügt. Im Browser und im Entwicklungsmodus gibt es keinen.
 */
function cspNonce(): string {
  const el = document.querySelector<HTMLElement>("style[nonce], script[nonce], link[nonce]");
  return el?.nonce || el?.getAttribute("nonce") || "";
}

export interface CursorInfo {
  line: number;
  col: number;
  selected: number;
}

export interface Reveal {
  line: number;
  col?: number;
  /** Ändert sich bei jedem Sprung, damit derselbe Sprung zweimal möglich ist. */
  nonce: number;
}

export interface ForgeEditorHandle {
  focus(): void;
  view(): EditorView | null;
}

export interface ForgeEditorProps {
  /** Eindeutiger Schlüssel der Datei. Pro Schlüssel bleiben Undo-Verlauf und Scrollposition erhalten. */
  docKey: string;
  text: string;
  /** Erhöht sich, wenn die Datei von außen neu geladen wurde. */
  revision: number;
  diagnostics?: Diagnostic[];
  /** Nur lesen, z. B. Dateien aus Bibliotheken. */
  readOnly?: boolean;
  /** Sprung zu einer Definition. */
  onGoto?(loc: LspLocation): void;
  /** Funktionen, die die KI-Prüfung ohne Auffälligkeit durchlaufen haben. */
  aiOk?: AiOk[];
  extensions?: Extension;
  reveal?: Reveal | null;
  autoFocus?: boolean;
  onChange?(text: string): void;
  onSave?(): void;
  onCursor?(info: CursorInfo): void;
}

interface Cached {
  state: EditorState;
  revision: number;
  scrollTop: number;
}

export const ForgeEditor = forwardRef<ForgeEditorHandle, ForgeEditorProps>(function ForgeEditor(props, ref) {
  const host = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const cache = useRef(new Map<string, Cached>());
  const current = useRef<string | null>(null);
  const extras = useRef(new Compartment());
  const prefs = useRef(new Compartment());
  const settings = useEditorSettings((st) => st.settings);
  const cb = useRef(props);
  cb.current = props;

  useImperativeHandle(ref, () => ({ focus: () => viewRef.current?.focus(), view: () => viewRef.current }), []);

  const makeState = (doc: string) =>
    EditorState.create({
      doc,
      extensions: [
        forgeDiagnostics(),
        EditorView.cspNonce.of(cspNonce()),
        forgeAiMarks(),
        lineNumbers(),
        highlightActiveLineGutter(),
        foldGutter({
          markerDOM(open) {
            const el = document.createElement("span");
            el.textContent = open ? "⌄" : "›";
            el.title = open ? "Einklappen" : "Ausklappen";
            return el;
          },
        }),
        history(),
        drawSelection(),
        EditorState.allowMultipleSelections.of(true),
        rectangularSelection(),
        crosshairCursor(),
        germanPhrases,
        // Text zu ziehen braucht hier niemand. Ein Klick in eine Markierung startet sonst einen Ziehvorgang,
        // den macOS nicht sauber beendet: Die Maus gilt als gedrückt und die Ansicht scrollt von selbst.
        EditorView.domEventHandlers({
          mousedown(e, view) {
            if (e.button !== 0 || e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return false;
            const sel = view.state.selection.main;
            if (sel.empty) return false;
            const pos = view.posAtCoords({ x: e.clientX, y: e.clientY });
            if (pos != null && pos >= sel.from && pos <= sel.to) view.dispatch({ selection: { anchor: pos } });
            return false;
          },
          dragstart(e) {
            e.preventDefault();
            return true;
          },
        }),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        autocompletion({ override: [lspCompletions(() => cb.current.docKey)], icons: true, maxRenderedOptions: 60 }),
        forgeLsp({ file: () => cb.current.docKey, goto: (l) => cb.current.onGoto?.(l) }),
        ...(cb.current.readOnly ? [EditorState.readOnly.of(true), EditorView.editable.of(false)] : []),
        prefs.current.of(editorSettingsExtension(useEditorSettings.getState().settings)),
        cpp(),
        forgeTheme,
        forgeHighlighting,
        semanticLite,
        extras.current.of([]),
        EditorView.contentAttributes.of({ "aria-label": "Quelltext", spellcheck: "false", autocorrect: "off", autocapitalize: "off" }),
        keymap.of([
          { key: "Mod-s", preventDefault: true, run: () => (cb.current.onSave?.(), true) },
          // ⌘U flasht (Menü). Ohne diese Zeile macht CodeMirror daraus zusätzlich „Auswahl rückgängig“.
          { key: "Mod-u", run: () => true, preventDefault: true },
          { key: "Mod-Alt-ArrowUp", run: addCursorAbove, preventDefault: true },
          { key: "Mod-Alt-ArrowDown", run: addCursorBelow, preventDefault: true },
          // Diese Befehle gehören der Oberfläche (Palette, Sprachserver), der Editor reicht sie nur weiter.
          ...(["F2:editor.rename", "Shift-F12:editor.references", "Shift-Alt-f:editor.format"] as const).map((entry) => {
            const [key, id] = entry.split(":");
            return { key, preventDefault: true, run: () => (window.dispatchEvent(new CustomEvent("forge:run-command", { detail: id })), true) };
          }),
          { key: "Tab", run: acceptCompletion },
          ...closeBracketsKeymap,
          ...completionKeymap,
          ...searchKeymap,
          ...foldKeymap,
          ...historyKeymap,
          ...defaultKeymap,
          indentWithTab,
        ]),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) cb.current.onChange?.(u.state.doc.toString());
          if (u.selectionSet || u.docChanged) {
            const sel = u.state.selection.main;
            const line = u.state.doc.lineAt(sel.head);
            cb.current.onCursor?.({ line: line.number, col: sel.head - line.from + 1, selected: Math.abs(sel.to - sel.from) });
          }
        }),
      ],
    });

  // Ansicht einmal aufbauen.
  useEffect(() => {
    const view = new EditorView({ parent: host.current!, state: makeState("") });
    viewRef.current = view;
    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Datei wechseln oder neu laden.
  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    if (current.current && current.current !== props.docKey) {
      cache.current.set(current.current, {
        state: view.state,
        revision: cache.current.get(current.current)?.revision ?? 0,
        scrollTop: view.scrollDOM.scrollTop,
      });
    }
    const hit = cache.current.get(props.docKey);
    const reuse = hit && hit.revision === props.revision;
    const state = reuse ? hit.state : makeState(props.text);
    view.setState(state);
    cache.current.set(props.docKey, { state, revision: props.revision, scrollTop: reuse ? hit.scrollTop : 0 });
    current.current = props.docKey;
    requestAnimationFrame(() => {
      view.scrollDOM.scrollTop = cache.current.get(props.docKey)?.scrollTop ?? 0;
    });
    view.dispatch({
      effects: [
        setDiagnostics.of(props.diagnostics ?? []),
        setAiOk.of(props.aiOk ?? []),
        // Ein zwischengespeicherter Zustand kennt die Einstellungen von damals.
        prefs.current.reconfigure(editorSettingsExtension(useEditorSettings.getState().settings)),
      ],
    });
    const sel = view.state.selection.main;
    const line = view.state.doc.lineAt(sel.head);
    cb.current.onCursor?.({ line: line.number, col: sel.head - line.from + 1, selected: Math.abs(sel.to - sel.from) });
    if (props.autoFocus) view.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [props.docKey, props.revision]);

  // Verworfene, umbenannte oder gelöschte Dateien behalten keinen Verlauf. Sonst käme alter Text beim Öffnen zurück.
  useEffect(() => {
    const onForget = (e: Event) => {
      const key = (e as CustomEvent<{ key: string }>).detail?.key;
      if (!key) return;
      cache.current.delete(key);
      // Beim Wechsel zu einer anderen Datei würde der Zustand der aktuellen sonst wieder in den Cache wandern.
      if (current.current === key) current.current = null;
    };
    window.addEventListener("forge:forget-doc", onForget);
    return () => window.removeEventListener("forge:forget-doc", onForget);
  }, []);

  // Diagnosen aktualisieren.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: setDiagnostics.of(props.diagnostics ?? []) });
  }, [props.diagnostics]);

  // Haken der KI-Prüfung.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: setAiOk.of(props.aiOk ?? []) });
  }, [props.aiOk]);

  // Zusätzliche Erweiterungen (z. B. KI-Markierungen) austauschen.
  useEffect(() => {
    viewRef.current?.dispatch({ effects: extras.current.reconfigure(props.extensions ?? []) });
  }, [props.extensions]);

  // Einstellungen (Schrift, Tabbreite, Umbruch).
  useEffect(() => {
    viewRef.current?.dispatch({ effects: prefs.current.reconfigure(editorSettingsExtension(settings)) });
  }, [settings]);

  // Sprung zu einer Zeile.
  useEffect(() => {
    const view = viewRef.current;
    const r = props.reveal;
    if (!view || !r) return;
    const line = view.state.doc.line(Math.min(Math.max(r.line, 1), view.state.doc.lines));
    const pos = Math.min(line.from + Math.max((r.col ?? 1) - 1, 0), line.to);
    view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
    view.focus();
  }, [props.reveal]);

  return <div ref={host} className="forge-editor" />;
});
