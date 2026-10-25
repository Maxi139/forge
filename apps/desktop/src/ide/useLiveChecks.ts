import { useEffect, useRef } from "react";
import { backend } from "../lib/backend";
import type { Diagnostic } from "../lib/diagnostics";
import { useAi } from "../lib/ai";
import { useHardware } from "../lib/hardware";
import { useLsp } from "../lib/lsp";
import { isCancelled, useStore } from "../lib/store";

const SYNTAX_DELAY = 250;
const CHECK_DELAY = 800;
const ANALYSIS_DELAY = 1100;
const CODE_FILE = /\.(ino|cpp|c|cc|h|hpp)$/i;

/**
 * Prüft den Quelltext, während getippt wird: erst die schnelle Strukturprüfung, kurz danach der Compiler.
 * Ältere Läufe werden verworfen, sobald ein neuer beginnt.
 */
export function useLiveChecks() {
  const project = useStore((s) => s.project);
  const active = useStore((s) => s.active);
  const buffers = useStore((s) => s.buffers);
  const fqbn = project?.config.board.fqbn;
  const options = project?.config.board.options;
  const ready = useHardware((s) => !!s.toolchain?.esp32Core);
  const activity = useHardware((s) => s.activity);
  const libraries = useHardware((s) => s.libraries.length);
  const seq = useRef(0);

  // Strukturprüfung der aktiven Datei.
  const activeText = active ? buffers[active]?.text : undefined;
  useEffect(() => {
    if (!active || activeText == null || !CODE_FILE.test(active)) return;
    const id = setTimeout(() => {
      backend()
        .syntax(active, activeText)
        .then((list) => {
          const s = useStore.getState();
          // Hat der Compiler für diese Datei Fehler gemeldet, ist er die bessere Quelle.
          const compilerHasErrors = (s.diagnostics.compiler?.[active] ?? []).some((d) => d.severity === "error");
          s.setDiagnostics("syntax", active, compilerHasErrors ? [] : list);
        })
        .catch(() => undefined);
    }, SYNTAX_DELAY);
    return () => clearTimeout(id);
  }, [active, activeText]);

  // Compilerprüfung des ganzen Sketches, mit ungespeichertem Text.
  const signature = useStore((s) => Object.entries(s.buffers).map(([p, b]) => `${p}:${b.version}:${b.text === b.saved ? "s" : "d"}`).join("|"));
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
    return () => clearTimeout(id);
  }, [project, fqbn, options, ready, activity, signature, libraries]);
}

/** Regeln ohne Compiler laufen etwas später und unabhängig vom Compiler. */
export function useLiveAnalysis() {
  const project = useStore((s) => s.project);
  const fqbn = project?.config.board.fqbn;
  const options = project?.config.board.options;
  const signature = useStore((s) => Object.entries(s.buffers).map(([p, b]) => `${p}:${b.version}:${b.text === b.saved ? "s" : "d"}`).join("|"));
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
    return () => clearTimeout(id);
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
  const signature = useStore((s) => Object.entries(s.buffers).map(([p, b]) => `${p}:${b.version}:${b.text === b.saved ? "s" : "d"}`).join("|"));
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
  const signature = useStore((s) => Object.entries(s.buffers).filter(([, b]) => !b.readOnly).map(([p, b]) => `${p}:${b.version}:${b.text === b.saved ? "s" : "d"}`).join("|"));
  useEffect(() => {
    if (!project || !ready) return;
    const id = setTimeout(() => void useLsp.getState().sync(), LSP_DELAY);
    return () => clearTimeout(id);
  }, [project, ready, fqbn, options, libraries, signature]);
}
