import { create } from "zustand";
import { t } from "../i18n/de";
import { backend, type AiBlockResult, type AiState, type AiThread, type ModelInfo } from "./backend";
import type { Diagnostic } from "./diagnostics";
import { errorText, useStore } from "./store";
import { webKind } from "./web/embed";

interface Ai {
  state: AiState | null;
  /** Ergebnisse je Datei und Funktion. */
  blocks: Record<string, AiBlockResult[]>;
  running: boolean;
  /** Funktionen, die nicht geprüft werden konnten, mit dem Grund. */
  failed: Record<string, Array<{ name: string; message: string }>>;
  /** Zuletzt gemeldeter Grund, warum die Prüfung stoppt (Guthaben, Limit ...). */
  stopped: string | null;
  models: ModelInfo[];
  suggested: string | null;
  /** Gespräche über Hinweise, nach Kennung. */
  threads: Record<string, AiThread>;
  /** Gespräche, in denen die KI gerade antwortet. */
  sending: Record<string, boolean>;
  /** Das Gespräch, das im Problem-Panel aufgeklappt ist. */
  openThread: string | null;
  /** Hinweise, die die KI gerade behebt, nach Kennung. */
  fixing: Record<string, boolean>;
  /** Die KI arbeitet an einer Aufgabe. */
  tasking: boolean;
  /** Fehler der letzten Aufgabe, für den Dialog. */
  taskError: string | null;

  init(): Promise<void>;
  refresh(): Promise<void>;
  configure(opts: { key?: string; model?: string; dailyTokens?: number }): Promise<void>;
  setProjectEnabled(on: boolean): Promise<void>;
  loadModels(): Promise<void>;
  /** Läuft die Prüfung? Dann braucht sie Schlüssel und Freigabe im Projekt. */
  enabled(): boolean;
  run(): Promise<void>;
  loadThreads(): Promise<void>;
  setOpenThread(id: string | null): void;
  /** Antwortet auf einen Hinweis. Die KI antwortet zurück und schließt das Gespräch, wenn sie überzeugt ist. */
  reply(id: string, text: string): Promise<void>;
  /** Schließt einen Hinweis, auch ohne Zustimmung der KI. */
  closeThread(id: string): Promise<void>;
  reopenThread(id: string): Promise<void>;
  /** Behebt einen Hinweis. Mit fertigem Vorschlag wird der übernommen, sonst arbeitet die KI die Änderung aus. */
  fix(d: Diagnostic): Promise<void>;
  /** Gibt der KI eine Aufgabe. Ob sie umgesetzt wurde, sagt der Rückgabewert. */
  task(text: string): Promise<boolean>;
  /** Eine Meldung der KI nach angenommener Behebung entfernen. */
  dismiss(file: string, id: string): void;
  /** Nimmt den Hinweis eines Gesprächs aus Editor und Problem-Liste. */
  hideDiagnostic(id: string): void;
  reset(): void;
}

const HINT_KEY = "forge.ai-hint";

function publish(file: string, list: AiBlockResult[]) {
  const diags: Diagnostic[] = list.flatMap((b) => b.diagnostics);
  useStore.getState().setDiagnostics("ai", file, diags);
}

export const useAi = create<Ai>((set, get) => ({
  state: null,
  blocks: {},
  running: false,
  failed: {},
  stopped: null,
  models: [],
  suggested: null,
  threads: {},
  sending: {},
  openThread: null,
  fixing: {},
  tasking: false,
  taskError: null,

  async init() {
    await get().refresh();
    await backend().onAiBlock((r) => {
      set((s) => {
        const others = (s.blocks[r.file] ?? []).filter((b) => b.name !== r.name);
        const bad = r.state === "failed" || r.state === "skipped";
        const next = bad ? others : [...others, r];
        publish(r.file, next);
        const failedHere = (s.failed[r.file] ?? []).filter((f) => f.name !== r.name);
        if (bad && r.message) failedHere.push({ name: r.name, message: r.message });
        return {
          blocks: { ...s.blocks, [r.file]: next },
          failed: { ...s.failed, [r.file]: failedHere },
          stopped: bad ? (r.message ?? s.stopped) : s.stopped,
        };
      });
    });
  },

  async refresh() {
    try {
      set({ state: await backend().aiState() });
    } catch {
      set({ state: null });
    }
  },

  async configure(opts) {
    try {
      await backend().aiConfigure(opts);
      set({ stopped: null });
      await get().refresh();
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async setProjectEnabled(on) {
    try {
      await backend().aiSetProjectEnabled(on);
      if (!on) get().reset();
      await get().refresh();
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async loadModels() {
    try {
      const list = await backend().aiModels();
      set({ models: list.models, suggested: list.suggested });
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.ai.modelsFailed, message: errorText(e) });
    }
  },

  enabled() {
    const s = get().state;
    return !!s && s.hasKey && s.projectEnabled;
  },

  async run() {
    const store = useStore.getState();
    const p = store.project;
    if (!p || get().running) return;
    const st = get().state;
    if (!st || !st.projectEnabled) return;
    if (!st.hasKey) {
      hintOnce();
      return;
    }
    // Mit Fehlern im Code rät die KI nur. Der Compiler ist dann die bessere Quelle und kostet nichts.
    for (const src of ["syntax", "compiler"] as const) {
      for (const list of Object.values(store.diagnostics[src] ?? {})) if (list.some((d) => d.severity === "error")) return;
    }
    // Webdateien prüft die KI in ihrem gespeicherten Stand. Strukturfehler darin schließen die Datei aus.
    const overrides: Record<string, string> = {};
    for (const [path, b] of Object.entries(store.buffers)) if (b.text !== b.saved && !webKind(path)) overrides[path] = b.text;
    const skip = Object.entries(store.diagnostics.web ?? {})
      .filter(([, list]) => list.some((d) => d.severity === "error"))
      .map(([file]) => file);
    set({ running: true });
    try {
      const res = await backend().aiCheck({ fqbn: p.config.board.fqbn, options: p.config.board.options }, overrides, skip);
      if (res.skipped) {
        await get().refresh();
        return;
      }
      // Funktionen, die es nicht mehr gibt, verlieren ihr Ergebnis.
      const alive = new Set(res.blocks.map(([f, n]) => `${f}\u0000${n}`));
      set((s) => {
        const blocks: Record<string, AiBlockResult[]> = {};
        const failed: Record<string, Array<{ name: string; message: string }>> = {};
        for (const [file, list] of Object.entries(s.blocks)) {
          const keep = list.filter((b) => alive.has(`${file}\u0000${b.name}`));
          blocks[file] = keep;
          publish(file, keep);
        }
        for (const [file, list] of Object.entries(s.failed)) failed[file] = list.filter((f) => alive.has(`${file}\u0000${f.name}`));
        return { blocks, failed };
      });
      if (res.summary.stopped) set({ stopped: res.summary.stopped });
      void get().refresh();
    } catch (e) {
      console.debug("KI-Prüfung fehlgeschlagen", errorText(e));
    } finally {
      set({ running: false });
      void get().loadThreads();
    }
  },

  async loadThreads() {
    try {
      const list = await backend().aiThreads();
      set({ threads: Object.fromEntries(list.map((t) => [t.id, t])) });
    } catch {
      // Ohne Projekt gibt es keine Gespräche.
      set({ threads: {} });
    }
  },

  setOpenThread(id) {
    set({ openThread: id });
  },

  async reply(id, text) {
    const store = useStore.getState();
    const project = store.project;
    const body = text.trim();
    if (!project || !body || get().sending[id]) return;
    const now = Math.floor(Date.now() / 1000);
    // Die Nachricht erscheint sofort, die Antwort der KI folgt.
    set((s) => {
      const t = s.threads[id];
      if (!t) return {};
      return {
        threads: { ...s.threads, [id]: { ...t, messages: [...t.messages, { role: "user", text: body, at: now }] } },
        sending: { ...s.sending, [id]: true },
      };
    });
    try {
      const overrides: Record<string, string> = {};
      for (const [path, b] of Object.entries(store.buffers)) if (b.text !== b.saved) overrides[path] = b.text;
      const { thread } = await backend().aiThreadReply({ fqbn: project.config.board.fqbn, options: project.config.board.options }, overrides, id, body);
      set((s) => ({ threads: { ...s.threads, [thread.id]: thread } }));
      if (thread.status === "closed") get().hideDiagnostic(id);
      void get().refresh();
    } catch (e) {
      store.toast({ kind: "error", title: t.ai.replyFailed, message: errorText(e) });
      // Die Nachricht des Nutzers ist gespeichert, auch wenn die Antwort ausblieb.
      await get().loadThreads();
    } finally {
      set((s) => ({ sending: { ...s.sending, [id]: false } }));
    }
  },

  async fix(d) {
    const store = useStore.getState();
    const project = store.project;
    if (!project || get().fixing[d.id]) return;
    const applied = () => {
      window.dispatchEvent(new CustomEvent("forge:fix-applied", { detail: d }));
    };
    if (d.fix) {
      await store.applyEdits(d.file, d.fix.edits);
      applied();
      return;
    }
    const overrides: Record<string, string> = {};
    for (const [path, b] of Object.entries(store.buffers)) if (b.text !== b.saved) overrides[path] = b.text;
    const before = store.buffers[d.file]?.text;
    set((s) => ({ fixing: { ...s.fixing, [d.id]: true } }));
    try {
      const res = await backend().aiFix({ fqbn: project.config.board.fqbn, options: project.config.board.options }, overrides, d.file, d.line, d.message);
      if (useStore.getState().buffers[d.file]?.text !== before) {
        store.toast({ kind: "warning", title: t.ai.fixFailed, message: t.ai.fixStale, ttl: 6000 });
        return;
      }
      if (!res.edits.length) {
        store.toast({ kind: "info", title: t.ai.fixNone, message: res.note || undefined, ttl: 8000 });
        return;
      }
      await store.applyEdits(d.file, res.edits);
      applied();
      store.toast({ kind: "success", title: t.ai.fixDone, message: res.note || undefined, ttl: 5000 });
    } catch (e) {
      store.toast({ kind: "error", title: t.ai.fixFailed, message: errorText(e) });
    } finally {
      set((s) => ({ fixing: { ...s.fixing, [d.id]: false } }));
    }
  },

  async task(text) {
    const store = useStore.getState();
    const project = store.project;
    const body = text.trim();
    if (!project || !body || get().tasking) return false;
    const overrides: Record<string, string> = {};
    for (const [path, b] of Object.entries(store.buffers)) if (b.text !== b.saved) overrides[path] = b.text;
    const before = Object.fromEntries(Object.entries(store.buffers).map(([p, b]) => [p, b.text]));
    // Die Meldungen, die der Nutzer gerade sieht, helfen bei „behebe die Fehler“.
    const problems = Object.entries(store.diagnostics)
      .filter(([source]) => source !== "ai")
      .flatMap(([, perFile]) => Object.values(perFile ?? {}).flat())
      .filter((d) => d.severity !== "info")
      .slice(0, 20)
      .map((d) => `${d.file.startsWith(project.path) ? d.file.slice(project.path.length).replace(/^\//, "") : d.file}:${d.line}: ${d.message}`)
      .join("\n");
    set({ tasking: true, taskError: null });
    try {
      const res = await backend().aiTask({ fqbn: project.config.board.fqbn, options: project.config.board.options }, overrides, body, store.active, problems);
      const now = useStore.getState().buffers;
      if (res.changes.some((c) => c.file in before && now[c.file]?.text !== before[c.file])) {
        set({ taskError: t.ai.taskStale });
        return false;
      }
      if (!res.changes.length && !res.created.length) {
        set({ taskError: res.note || t.ai.taskNothing });
        return false;
      }
      for (const c of res.changes) {
        await useStore.getState().applyEdits(c.file, c.edits);
        // Der Editor wechselt asynchron zur Datei, bevor die Änderung ankommt.
        await new Promise((r) => setTimeout(r, 180));
      }
      for (const f of res.created) {
        await backend().createFile(f.file, false);
        await backend().writeFile(f.file, f.text);
      }
      if (res.created.length) {
        await useStore.getState().refreshTree();
        await useStore.getState().openFile(res.created[0].file);
      }
      const parts = [res.changes.length ? t.ai.taskFiles(res.changes.length) : "", res.created.length ? t.ai.taskCreated(res.created.length) : ""].filter(Boolean);
      store.toast({ kind: "success", title: t.ai.taskDone, message: [res.note, parts.join(", ")].filter(Boolean).join(" "), ttl: 9000 });
      return true;
    } catch (e) {
      set({ taskError: errorText(e) });
      return false;
    } finally {
      set({ tasking: false });
    }
  },

  async closeThread(id) {
    try {
      const thread = await backend().aiThreadClose(id);
      set((s) => ({ threads: { ...s.threads, [id]: thread } }));
      get().hideDiagnostic(id);
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async reopenThread(id) {
    try {
      const thread = await backend().aiThreadReopen(id);
      set((s) => ({ threads: { ...s.threads, [id]: thread }, openThread: id }));
      // Aus dem Zwischenspeicher, ohne neuen Aufruf.
      await get().run();
    } catch (e) {
      useStore.getState().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  hideDiagnostic(id) {
    for (const [file, list] of Object.entries(get().blocks)) {
      if (list.some((b) => b.diagnostics.some((d) => d.id === `ai:${id}`))) get().dismiss(file, `ai:${id}`);
    }
  },

  dismiss(file, id) {
    set((s) => {
      const next = (s.blocks[file] ?? []).map((b) => ({ ...b, diagnostics: b.diagnostics.filter((d) => d.id !== id) }));
      publish(file, next);
      return { blocks: { ...s.blocks, [file]: next } };
    });
  },

  reset() {
    const s = useStore.getState();
    for (const file of Object.keys(get().blocks)) s.setDiagnostics("ai", file, []);
    set({ blocks: {}, failed: {}, stopped: null, threads: {}, openThread: null });
  },
}));

/** Ein einziger Hinweis, dass die KI-Prüfung nur mit Schlüssel läuft. Danach schweigt Forge. */
function hintOnce() {
  try {
    if (localStorage.getItem(HINT_KEY)) return;
    localStorage.setItem(HINT_KEY, "1");
  } catch {
    return;
  }
  useStore.getState().toast({
    kind: "info",
    title: t.ai.hintTitle,
    message: t.ai.hintText,
    actions: [{ label: t.ai.hintAction, run: () => useStore.getState().setDialog({ kind: "settings" }), primary: true }],
    ttl: 14000,
  });
}
