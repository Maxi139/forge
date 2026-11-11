import { useEffect, useRef } from "react";
import { backend } from "../lib/backend";
import type { Diagnostic } from "../lib/diagnostics";
import { useAi } from "../lib/ai";
import { useHardware } from "../lib/hardware";
import { useLsp } from "../lib/lsp";
import { isCancelled, useStore } from "../lib/store";
import { checkWeb } from "../lib/web/check";
import { webKind } from "../lib/web/embed";
import { flattenTree, toRel } from "../lib/web/sync";

const SYNTAX_DELAY = 250;
const CHECK_DELAY = 800;
const ANALYSIS_DELAY = 1100;
const CODE_FILE = /\.(ino|cpp|c|cc|h|hpp)$/i;

/**
 * Ändert sich, sobald sich Quelltext des Sketches ändert. Webdateien zählen nicht: Compiler und Analyse
 * kennen sie nicht. Mit `withWeb` zählt eine gespeicherte Webdatei, eine geänderte nicht.
 */
function codeSignature(buffers: Record<string, { version: number; text: string; saved: string; readOnly?: boolean }>, withWeb = false, skipReadOnly = false): string {
  return Object.entries(buffers)
    .filter(([p, b]) => (CODE_FILE.test(p) || (withWeb && webKind(p))) && !(skipReadOnly && b.readOnly))
    .map(([p, b]) => (webKind(p) ? `${p}:${b.text === b.saved ? b.version : "d"}` : `${p}:${b.version}:${b.text === b.saved ? "s" : "d"}`))
    .join("|");
}

/**
 * Prüft den Quelltext, während getippt wird: erst die schnelle Strukturprüfung, kurz danach der Compiler.
 * Ältere Läufe werden verworfen, sobald ein neuer beginnt.
 */
export function useLiveChecks() {
  const project = useStore((s) => s.project);
  const active = useStore((s) => s.active);
  const fqbn = project?.config.board.fqbn;
  const options = project?.config.board.options;
  const ready = useHardware((s) => !!s.toolchain?.esp32Core);
  const activity = useHardware((s) => s.activity);
  const libraries = useHardware((s) => s.libraries.length);
  const seq = useRef(0);

  // Strukturprüfung der aktiven Datei.
  // Nur der Text der aktiven Datei: ein Abo auf alle Puffer würde die ganze Oberfläche bei jedem Tastendruck neu zeichnen.
  const activeText = useStore((s) => (s.active ? s.buffers[s.active]?.text : undefined));
  useEffect(() => {
    if (!active || activeText == null || !CODE_FILE.test(active)) return;
    const id = setTimeout(() => {
      backend()
        .syntax(active, activeText)
        .then((list) => {
          const s = useStore.getState();
          // Inzwischen weitergetippt oder die Datei geschlossen: das Ergebnis ist veraltet.
          if (s.buffers[active]?.text !== activeText) return;
          // Hat der Compiler für diese Datei Fehler gemeldet, ist er die bessere Quelle.
          const compilerHasErrors = (s.diagnostics.compiler?.[active] ?? []).some((d) => d.severity === "error");
          s.setDiagnostics("syntax", active, compilerHasErrors ? [] : list);
        })
        .catch(() => undefined);
    }, SYNTAX_DELAY);
    return () => clearTimeout(id);
  }, [active, activeText]);

  // Compilerprüfung des ganzen Sketches, mit ungespeichertem Text.
  const signature = useStore((s) => codeSignature(s.buffers));
  useEffect(() => {
    if (!project || !fqbn || !ready || activity !== "idle") return;
    const run = ++seq.current;
    const id = setTimeout(async () => {
      const s = useStore.getState();
      const overrides: Record<string, string> = {};
      for (const [path, b] of Object.entries(s.buffers)) if (b.text !== b.saved) overrides[path] = b.text;
      try {
        const res = await backend().check({ fqbn, options: options ?? {} }, overrides);
        if (run !== seq.current || !res.ran) return;
        const st = useStore.getState();
        const byFile = new Map<string, Diagnostic[]>();
        for (const d of res.diagnostics) byFile.set(d.file, [...(byFile.get(d.file) ?? []), d]);
        // Dateien, die vorher Meldungen hatten und jetzt keine mehr, werden geleert.
        const before = Object.keys(st.diagnostics.compiler ?? {});
        for (const f of new Set([...before, ...byFile.keys()])) {
          const list = byFile.get(f) ?? [];
          st.setDiagnostics("compiler", f, list);
          if (list.some((d) => d.severity === "error")) st.setDiagnostics("syntax", f, []);
        }
      } catch (e) {
        // Abgebrochene Läufe sind normal. Alles andere zeigt höchstens der Ausgabe-Bereich.
        if (!isCancelled(e)) console.debug("Prüfung fehlgeschlagen", e);
      }
    }, CHECK_DELAY);
    return () => {
      // Ein laufender Aufruf darf nach Projektwechsel oder Build-Start nichts mehr eintragen.
      seq.current++;
      clearTimeout(id);
    };
  }, [project, fqbn, options, ready, activity, signature, libraries]);
}

/** Regeln ohne Compiler laufen etwas später und unabhängig vom Compiler. */
export function useLiveAnalysis() {
  const project = useStore((s) => s.project);
  const fqbn = project?.config.board.fqbn;
  const options = project?.config.board.options;
  const signature = useStore((s) => codeSignature(s.buffers));
  const seq = useRef(0);

  useEffect(() => {
    if (!project || !fqbn) return;
    const run = ++seq.current;
    const id = setTimeout(async () => {
      const s = useStore.getState();
      const overrides: Record<string, string> = {};
      for (const [path, b] of Object.entries(s.buffers)) if (b.text !== b.saved) overrides[path] = b.text;
      try {
        const res = await backend().analyze({ fqbn, options: options ?? {} }, overrides);
        if (run !== seq.current) return;
        const st = useStore.getState();
        const byFile = new Map<string, Diagnostic[]>();
        for (const d of res.diagnostics) byFile.set(d.file, [...(byFile.get(d.file) ?? []), d]);
        for (const f of new Set([...Object.keys(st.diagnostics.analysis ?? {}), ...byFile.keys()])) st.setDiagnostics("analysis", f, byFile.get(f) ?? []);
        useStore.setState({ facts: res.facts });
      } catch (e) {
        console.debug("Analyse fehlgeschlagen", e);
      }
    }, ANALYSIS_DELAY);
    return () => {
      seq.current++;
      clearTimeout(id);
    };
  }, [project, fqbn, options, signature]);
}

const AI_DELAY = 1500;

/**
 * Die KI-Prüfung läuft erst, wenn kurz nicht getippt wird und Syntax und Compiler keine Fehler melden.
 * Unveränderte Funktionen kommen aus dem Zwischenspeicher, das Backend ruft nur Geänderte ab.
 */
export function useLiveAi() {
  const project = useStore((s) => s.project);
  const ready = useHardware((s) => !!s.toolchain?.esp32Core);
  const activity = useHardware((s) => s.activity);
  const enabled = useAi((s) => !!s.state && s.state.hasKey && s.state.projectEnabled);
  // Webdateien prüft die KI als ganze Datei. Sie läuft erst, wenn die Datei gespeichert ist, nicht bei jedem Tastendruck.
  const signature = useStore((s) => codeSignature(s.buffers, true));
  const errors = useStore((s) => {
    for (const src of ["syntax", "compiler"] as const) {
      for (const list of Object.values(s.diagnostics[src] ?? {})) if (list.some((d) => d.severity === "error")) return true;
    }
    return false;
  });

  const projectPath = project?.path;
  useEffect(() => {
    useAi.getState().reset();
    void useAi.getState().refresh();
  }, [projectPath]);

  useEffect(() => {
    if (!project || !ready || activity !== "idle" || errors) return;
    const id = setTimeout(() => void useAi.getState().run(), AI_DELAY);
    return () => clearTimeout(id);
  }, [project, ready, activity, errors, enabled, signature]);
}

const LSP_DELAY = 500;

/** Hält clangd auf dem Stand des Editors, damit Vorschläge und Hover zu dem passen, was gerade dasteht. */
export function useLiveLsp() {
  const project = useStore((s) => s.project);
  const ready = useHardware((s) => !!s.toolchain?.esp32Core);
  const fqbn = project?.config.board.fqbn;
  const options = project?.config.board.options;
  const libraries = useHardware((s) => s.libraries.length);
  const signature = useStore((s) => codeSignature(s.buffers, false, true));
  useEffect(() => {
    if (!project || !ready) return;
    const id = setTimeout(() => void useLsp.getState().sync(), LSP_DELAY);
    return () => clearTimeout(id);
  }, [project, ready, fqbn, options, libraries, signature]);
}

const WEB_DELAY = 250;

/** Prüft die geöffnete Webdatei beim Tippen: Strukturfehler, fehlende und externe Dateien. */
export function useLiveWebChecks() {
  const active = useStore((s) => s.active);
  const text = useStore((s) => (s.active ? s.buffers[s.active]?.text : undefined));
  const tree = useStore((s) => s.tree);
  useEffect(() => {
    if (!active || text == null || !webKind(active)) return;
    const id = setTimeout(() => {
      const known = new Set(flattenTree(tree).map(toRel));
      useStore.getState().setDiagnostics("web", active, checkWeb(toRel(active), text, { exists: (p) => known.has(p) }).map((d) => ({ ...d, file: active })));
    }, WEB_DELAY);
    return () => clearTimeout(id);
  }, [active, text, tree]);
}
