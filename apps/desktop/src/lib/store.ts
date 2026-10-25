import { create } from "zustand";
import { backend, type CreateProjectRequest, type FileNode, type ProjectInfo, type RecentProject } from "./backend";
import type { FunctionFacts } from "./backend";
import { bySeverity, type Diagnostic, type DiagnosticSource, type TextEdit } from "./diagnostics";
import { t } from "../i18n/de";

/* ---------- Typen ---------- */
export interface Buffer {
  text: string;
  saved: string;
  /** Erhöht sich, wenn der Inhalt von außen ersetzt wurde. */
  revision: number;
  /** Zählt jede Änderung im Editor hoch. Live-Prüfungen erkennen daran auch Änderungen gleicher Länge. */
  version: number;
  /** Dateien aus Core und Bibliotheken: nur lesen. */
  readOnly?: boolean;
}

export type SidebarView = "files" | "search" | "library";
export type PaletteMode = "commands" | "files" | "symbols";
export type BottomTab = "monitor" | "plotter" | "problems" | "output";

export interface ToastItem {
  id: number;
  kind: "info" | "success" | "warning" | "error" | "progress";
  title: string;
  message?: string;
  progress?: number;
  actions?: Array<{ label: string; run(): void; primary?: boolean }>;
  /** Millisekunden bis zum automatischen Ausblenden. Ohne Wert bleibt der Toast. */
  ttl?: number;
  /** Zeigt „Abbrechen“ an einem Fortschritts-Toast. Ohne Wert gibt es keinen Knopf. */
  onCancel?: () => void;
}

export type DialogState =
  | { kind: "newProject" }
  | { kind: "boardPicker" }
  | { kind: "settings" }
  | { kind: "secrets" }
  | { kind: "aiTask" }
  | { kind: "moveSecret"; file: string; line: number; from: number; to: number; value: string; name: string; mode: "call" | "decl" }
  | { kind: "toolchain" }
  | { kind: "examples" }
  | { kind: "unsaved"; path: string; resolve(choice: "save" | "discard" | "cancel"): void }
  | { kind: "confirmDelete"; path: string; resolve(ok: boolean): void }
  | { kind: "confirm"; title: string; text: string; confirm: string; tone: "error" | "warning" | "glut"; resolve(ok: boolean): void }
  | { kind: "prompt"; title: string; label: string; initial: string; confirm: string; validate?(v: string): string | null; resolve(v: string | null): void };

export interface OutputLine {
  text: string;
  kind?: "cmd" | "err" | "warn";
}

export interface LayoutState {
  sidebarWidth: number;
  sidebarOpen: boolean;
  bottomHeight: number;
  bottomOpen: boolean;
  deviceWidth: number;
  deviceOpen: boolean;
}

const LAYOUT_KEY = "forge.layout";
const DEFAULT_LAYOUT: LayoutState = { sidebarWidth: 248, sidebarOpen: true, bottomHeight: 248, bottomOpen: true, deviceWidth: 312, deviceOpen: false };

function readLayout(): LayoutState {
  try {
    return { ...DEFAULT_LAYOUT, ...JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? "{}") };
  } catch {
    return DEFAULT_LAYOUT;
  }
}

function writeLayout(l: LayoutState) {
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(l));
  } catch {
    /* Ohne Speicher gilt das Layout nur für diese Sitzung. */
  }
}

export const baseName = (p: string) => p.split("/").pop() ?? p;
export const dirName = (p: string) => p.slice(0, Math.max(p.lastIndexOf("/"), 0));

export function errorText(e: unknown, fallback = t.errors.generic): string {
  return e instanceof Error && e.message ? e.message : typeof e === "string" && e ? e : fallback;
}

/** Ob ein Fehler nur den Abbruch durch den Nutzer meldet (Backend und Oberfläche schreiben ihn verschieden). */
export function isCancelled(e: unknown): boolean {
  return /^abgebrochen\.?$/i.test(errorText(e, "").trim());
}

const MAX_TOASTS = 3;
const RECOVER_KEY = "forge.recover";

/** Sagt dem Rest der Oberfläche, dass ein anderes Projekt gilt und Anzeigen des alten wegmüssen. */
function announceProjectSwitch() {
  window.dispatchEvent(new Event("forge:project-switched"));
}

/** Sagt dem Editor, dass er sich den Zustand dieser Datei (Verlauf, Scrollstand) nicht mehr merken soll. */
function forgetDocs(paths: string[]) {
  for (const key of paths) window.dispatchEvent(new CustomEvent("forge:forget-doc", { detail: { key } }));
}

let beforeSave: ((path: string) => Promise<void>) | null = null;
/** Vor dem ausdrücklichen Speichern der aktiven Datei läuft dieser Hook (Formatieren beim Speichern). */
export function setBeforeSave(fn: ((path: string) => Promise<void>) | null) {
  beforeSave = fn;
}

export interface RecoverySnapshot {
  project: string;
  files: Record<string, string>;
}

/** Hält ungespeicherte Texte fest, bevor die Oberfläche neu lädt. */
export function stashUnsaved() {
  try {
    const s = useStore.getState();
    if (!s.project) return;
    const files: Record<string, string> = {};
    for (const [path, b] of Object.entries(s.buffers)) if (!b.readOnly && b.text !== b.saved) files[path] = b.text;
    if (!Object.keys(files).length) return;
    const snapshot: RecoverySnapshot = { project: s.project.package ?? s.project.path, files };
    sessionStorage.setItem(RECOVER_KEY, JSON.stringify(snapshot));
  } catch {
    /* Ohne Speicher gibt es nichts zu retten. */
  }
}

function takeRecovery(): RecoverySnapshot | null {
  try {
    const raw = sessionStorage.getItem(RECOVER_KEY);
    if (!raw) return null;
    sessionStorage.removeItem(RECOVER_KEY);
    const snap = JSON.parse(raw) as RecoverySnapshot;
    return snap && typeof snap.project === "string" && snap.files && Object.keys(snap.files).length ? snap : null;
  } catch {
    return null;
  }
}

/* ---------- Store ---------- */
interface Store {
  /* Projekt */
  ready: boolean;
  project: ProjectInfo | null;
  tree: FileNode[];
  recents: RecentProject[];
  tabs: string[];
  active: string | null;
  buffers: Record<string, Buffer>;
  /** Diagnosen je Quelle und Datei. */
  diagnostics: Partial<Record<DiagnosticSource, Record<string, Diagnostic[]>>>;
  cursor: { line: number; col: number; selected: number };
  output: OutputLine[];
  /** Fakten der statischen Analyse zu allen Funktionen, Kontext für die KI-Prüfung. */
  facts: FunctionFacts[];

  /* Oberfläche */
  sidebarView: SidebarView;
  bottomTab: BottomTab;
  layout: LayoutState;
  paletteOpen: boolean;
  paletteMode: PaletteMode;
  dialog: DialogState | null;
  toasts: ToastItem[];

  /* Abläufe */
  init(): Promise<void>;
  openProject(path: string): Promise<boolean>;
  pickAndOpen(): Promise<void>;
  /** Öffnet eine .forgeproj-Datei, die der Nutzer auswählt. */
  pickAndOpenFile(): Promise<void>;
  /** Speichert das Projekt unter neuem Namen oder Ort als .forgeproj und arbeitet danach dort weiter. */
  saveAs(): Promise<void>;
  /** Entpackt das Projekt in einen normalen Ordner. */
  exportFolder(): Promise<void>;
  /** Prüft nach dem Öffnen einer .forgeproj, ob Bibliotheken oder der Core fehlen, und bietet die Installation an. */
  checkPackageNeeds(): Promise<void>;
  createProject(req: CreateProjectRequest): Promise<ProjectInfo>;
  createSketch(): Promise<void>;
  /** Öffnet eine Kopie eines Beispiels aus Core oder Bibliothek als neuen Sketch. */
  openExample(path: string): Promise<boolean>;
  /** Benennt das Projekt um. Gibt zurück, ob es geklappt hat. */
  renameProject(name: string): Promise<boolean>;
  /** Macht den geöffneten Sketch zum Projekt, mit forge.toml für Board, Secrets und KI-Prüfung. */
  convertToProject(): Promise<void>;
  /** Übernimmt ein Projekt, das an einen anderen Ort gezogen ist: Tabs, Puffer und Zuletzt-Liste folgen. */
  adoptProject(before: ProjectInfo, project: ProjectInfo): Promise<void>;
  /** Ein normaler Ordner ist nur der Altweg. Forge bietet an, ihn als Projektdatei zu speichern. */
  offerProjectFile(): void;
  closeProject(): Promise<boolean>;
  refreshTree(): Promise<void>;

  openFile(path: string): Promise<void>;
  /** Springt zu einer Stelle, auch in Dateien außerhalb des Projekts. */
  gotoLocation(loc: { file: string; line: number; col: number }): Promise<void>;
  closeTab(path: string): Promise<boolean>;
  setActive(path: string): void;
  updateBuffer(path: string, text: string): void;
  save(path?: string): Promise<boolean>;
  saveAll(): Promise<boolean>;
  reloadFromDisk(path: string): Promise<void>;
  isDirty(path: string): boolean;

  createEntry(dir: string, isDir: boolean): Promise<void>;
  renameEntry(path: string): Promise<void>;
  deleteEntry(path: string): Promise<void>;

  setDiagnostics(source: DiagnosticSource, file: string, list: Diagnostic[]): void;
  /** Nimmt eine einzelne Meldung heraus, etwa nach einer angenommenen Behebung. */
  dismissDiagnostic(d: Diagnostic): void;
  clearDiagnostics(source: DiagnosticSource): void;
  setCursor(c: { line: number; col: number; selected: number }): void;
  appendOutput(lines: OutputLine[]): void;
  clearOutput(): void;

  setSidebarView(v: SidebarView): void;
  /** Zeigt eine Ansicht, ohne sie bei erneutem Aufruf wieder zu schließen. */
  setSidebarViewOpen(v: SidebarView): void;
  setBottomTab(t: BottomTab): void;
  patchLayout(p: Partial<LayoutState>): void;
  togglePalette(open?: boolean, mode?: PaletteMode): void;
  setDialog(d: DialogState | null): void;
  /** Wendet Textänderungen auf eine Datei an, auch wenn sie nicht offen ist. */
  applyEdits(file: string, edits: TextEdit[]): Promise<void>;
  confirm(o: { title: string; text: string; confirm: string; tone?: "error" | "warning" | "glut" }): Promise<boolean>;
  prompt(o: { title: string; label: string; initial?: string; confirm: string; validate?(v: string): string | null }): Promise<string | null>;

  toast(item: Omit<ToastItem, "id">): number;
  updateToast(id: number, patch: Partial<Omit<ToastItem, "id">>): void;
  dismissToast(id: number): void;
}

let toastId = 1;
const timers = new Map<number, ReturnType<typeof setTimeout>>();

export const useStore = create<Store>((set, get) => ({
  ready: false,
  project: null,
  tree: [],
  recents: [],
  tabs: [],
  active: null,
  buffers: {},
  diagnostics: {},
  cursor: { line: 1, col: 1, selected: 0 },
  output: [],
  facts: [],

  sidebarView: "files",
  bottomTab: "problems",
  layout: readLayout(),
  paletteOpen: false,
  paletteMode: "commands",
  dialog: null,
  toasts: [],

  async init() {
    try {
      set({ recents: await backend().recentProjects() });
    } catch {
      set({ recents: [] });
    }
    set({ ready: true });
    const snap = takeRecovery();
    if (snap) {
      const n = Object.keys(snap.files).length;
      get().toast({
        kind: "warning",
        title: t.errors.recoverTitle,
        message: t.errors.recoverText(n),
        actions: [
          {
            label: t.errors.recover,
            primary: true,
            run: () =>
              void (async () => {
                if (!(await get().openProject(snap.project))) return;
                for (const [path, text] of Object.entries(snap.files)) {
                  await get().openFile(path);
                  get().updateBuffer(path, text);
                  // Der Editor baut seinen Zustand nur bei neuer Revision neu auf.
                  useStore.setState((s) => (s.buffers[path] ? { buffers: { ...s.buffers, [path]: { ...s.buffers[path], revision: s.buffers[path].revision + 1 } } } : s));
                }
              })(),
          },
          { label: t.errors.discard, run: () => undefined },
        ],
      });
    }
  },

  async openProject(path) {
    if (get().project && !(await get().closeProject())) return false;
    try {
      const project = await backend().openProject(path);
      set({ project, tabs: [], active: null, buffers: {}, diagnostics: {} });
      announceProjectSwitch();
      await get().refreshTree();
      await get().openFile(project.mainFile);
      await backend().pushRecent(project).catch(() => undefined);
      set({ recents: await backend().recentProjects().catch(() => get().recents) });
      if (project.package) void get().checkPackageNeeds();
      else get().offerProjectFile();
      return true;
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.openFailed, message: errorText(e) });
      return false;
    }
  },

  async pickAndOpen() {
    const dir = await backend().pickFolder();
    if (dir) await get().openProject(dir);
  },

  async pickAndOpenFile() {
    const file = await backend().pickProjectFile();
    if (file) await get().openProject(file);
  },

  async saveAs() {
    const before = get().project;
    if (!before) return;
    try {
      // Offene Änderungen sollen in der neuen Datei stehen.
      if (!(await get().saveAll())) return;
      const dest = await backend().pickSavePath(`${before.name}.forgeproj`);
      if (!dest) return;
      const project = await backend().packageSaveAs(dest);
      await get().adoptProject(before, project);
      get().toast({ kind: "success", title: t.package.savedAs(baseName(project.package ?? dest)), ttl: 4000 });
    } catch (e) {
      get().toast({ kind: "error", title: t.package.saveAsFailed, message: errorText(e) });
    }
  },

  async exportFolder() {
    if (!get().project?.package) return;
    try {
      await get().saveAll();
      const parent = await backend().pickFolder();
      if (!parent) return;
      const dest = await backend().packageExportFolder(parent);
      get().toast({ kind: "success", title: t.package.unpacked(baseName(dest)), ttl: 4000 });
    } catch (e) {
      get().toast({ kind: "error", title: t.package.exportFailed, message: errorText(e) });
    }
  },

  async checkPackageNeeds() {
    const needs = await backend().packageNeeds().catch(() => null);
    if (!needs) return;
    const missing = needs.libraries.filter((l) => l.installed === null);
    const different = needs.libraries.filter((l) => l.installed !== null);
    const coreNote = needs.core ? t.package.coreNote(needs.core.id, needs.core.wanted, needs.core.installed) : "";
    const list = (l: { name: string; version: string }[]) => l.map((x) => `${x.name} ${x.version}`).join(", ");

    if (missing.length) {
      const id = get().toast({
        kind: "warning",
        title: t.package.missingTitle(missing.length),
        message: [list(missing), different.length ? t.package.differentNote(list(different)) : "", coreNote].filter(Boolean).join(" "),
        actions: [
          {
            label: t.package.install,
            primary: true,
            run: () => {
              get().dismissToast(id);
              const progress = get().toast({ kind: "progress", title: t.package.installing });
              backend()
                .packageInstallNeeds()
                .then(() => get().toast({ kind: "success", title: t.package.installed, ttl: 4000 }))
                .catch((e) => get().toast({ kind: "error", title: t.package.installFailed, message: errorText(e) }))
                .finally(() => get().dismissToast(progress));
            },
          },
          { label: t.package.later, run: () => undefined },
        ],
      });
    } else if (different.length || coreNote) {
      get().toast({
        kind: "info",
        title: t.package.differsTitle,
        message: [different.length ? t.package.differentNote(list(different)) : "", coreNote].filter(Boolean).join(" "),
        ttl: 10000,
      });
    }
  },

  async createProject(req) {
    if (get().project && !(await get().closeProject())) throw new Error("abgebrochen");
    const project = await backend().createProject(req);
    set({ project, tabs: [], active: null, buffers: {}, diagnostics: {} });
    announceProjectSwitch();
    await get().refreshTree();
    await get().openFile(project.mainFile);
    await backend().pushRecent(project).catch(() => undefined);
    set({ recents: await backend().recentProjects().catch(() => get().recents) });
    return project;
  },

  async createSketch() {
    if (get().project && !(await get().closeProject())) return;
    try {
      const project = await backend().createSketch();
      set({ project, tabs: [], active: null, buffers: {}, diagnostics: {} });
      announceProjectSwitch();
      await get().refreshTree();
      await get().openFile(project.mainFile);
      await backend().pushRecent(project).catch(() => undefined);
      set({ recents: await backend().recentProjects().catch(() => get().recents) });
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async openExample(path) {
    if (get().project && !(await get().closeProject())) return false;
    try {
      const project = await backend().openExample(path);
      set({ project, tabs: [], active: null, buffers: {}, diagnostics: {} });
      announceProjectSwitch();
      await get().refreshTree();
      await get().openFile(project.mainFile);
      await backend().pushRecent(project).catch(() => undefined);
      set({ recents: await backend().recentProjects().catch(() => get().recents) });
      get().toast({ kind: "success", title: t.examples.opened(project.name), ttl: 4000 });
      return true;
    } catch (e) {
      get().toast({ kind: "error", title: t.examples.failed, message: errorText(e) });
      return false;
    }
  },

  async renameProject(name) {
    const before = get().project;
    const next = name.trim();
    if (!before || !next || next === before.name) return false;
    const invalid = await backend().validateName(next);
    if (invalid) {
      get().toast({ kind: "error", title: t.package.renameFailed, message: invalid });
      return false;
    }
    try {
      // Der Stand im Editor soll auf der Platte stehen, bevor der Ordner umzieht.
      if (!(await get().saveAll())) return false;
      const project = await backend().renameProject(next);
      const remap = (p: string) =>
        p === before.mainFile ? project.mainFile : p.startsWith(`${before.path}/`) ? project.path + p.slice(before.path.length) : p;
      set((s) => ({
        project,
        tabs: s.tabs.map(remap),
        active: s.active ? remap(s.active) : null,
        buffers: Object.fromEntries(Object.entries(s.buffers).map(([k, v]) => [remap(k), v])),
        diagnostics: {},
      }));
      await get().refreshTree();
      await backend().removeRecent(before.package ?? before.path).catch(() => undefined);
      await backend().pushRecent(project).catch(() => undefined);
      set({ recents: await backend().recentProjects().catch(() => get().recents) });
      return true;
    } catch (e) {
      get().toast({ kind: "error", title: t.package.renameFailed, message: errorText(e) });
      return false;
    }
  },

  async convertToProject() {
    const before = get().project;
    // Sketches werden zum Projekt, Ordner zur Projektdatei. Eine Projektdatei mit Projekt ist schon beides.
    if (!before || (before.kind !== "sketch" && before.package)) return;
    try {
      // Der Stand im Editor soll in der Datei stehen, bevor Forge sie umbaut.
      if (!(await get().saveAll())) return;
      const project = await backend().convertToProject();
      await get().adoptProject(before, project);
      get().toast(
        !before.package && project.package
          ? { kind: "success", title: t.package.convertedToFile(baseName(project.package)), message: t.package.folderStays, ttl: 8000 }
          : { kind: "success", title: t.package.converted(project.name), ttl: 4000 },
      );
    } catch (e) {
      get().toast({ kind: "error", title: t.package.convertFailed, message: errorText(e) });
    }
  },

  offerProjectFile() {
    const id = get().toast({
      kind: "info",
      title: t.package.folderTitle,
      message: t.package.folderText,
      actions: [
        {
          label: t.package.folderAction,
          primary: true,
          run: () => {
            get().dismissToast(id);
            void get().convertToProject();
          },
        },
        { label: t.package.later, run: () => undefined },
      ],
      ttl: 14000,
    });
  },

  async adoptProject(before, project) {
    // Tabs und Puffer zeigen auf den alten Arbeitsordner. Zieht das Projekt um, ziehen sie mit.
    const remap = (p: string) => (p.startsWith(`${before.path}/`) ? project.path + p.slice(before.path.length) : p);
    set((s) => ({
      project,
      tabs: s.tabs.map(remap),
      active: s.active ? remap(s.active) : null,
      buffers: Object.fromEntries(Object.entries(s.buffers).map(([k, v]) => [remap(k), v])),
      diagnostics: {},
    }));
    await get().refreshTree();
    // Ein Ordner geht in seiner Projektdatei auf. Bei „Speichern unter“ bleibt die alte Datei als Eintrag erhalten.
    if (!before.package || before.package === project.package) await backend().removeRecent(before.package ?? before.path).catch(() => undefined);
    await backend().pushRecent(project).catch(() => undefined);
    set({ recents: await backend().recentProjects().catch(() => get().recents) });
  },

  async closeProject() {
    for (const path of [...get().tabs]) if (get().isDirty(path) && !(await get().closeTab(path))) return false;
    set({ project: null, tree: [], tabs: [], active: null, buffers: {}, diagnostics: {} });
    announceProjectSwitch();
    await backend().closeProject().catch(() => undefined);
    return true;
  },

  async refreshTree() {
    if (!get().project) return;
    try {
      set({ tree: await backend().projectTree() });
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async openFile(path) {
    if (!get().buffers[path]) {
      try {
        const project = get().project;
        const external = !!project && path !== project.path && !path.startsWith(`${project.path}/`);
        const text = external ? await backend().readExternal(path) : await backend().readFile(path);
        set((s) => ({ buffers: { ...s.buffers, [path]: { text, saved: text, revision: 0, version: 0, readOnly: external || undefined } } }));
      } catch (e) {
        get().toast({ kind: "error", title: t.errors.readFailed(baseName(path)), message: errorText(e) });
        return;
      }
    }
    set((s) => ({ tabs: s.tabs.includes(path) ? s.tabs : [...s.tabs, path], active: path }));
  },

  async gotoLocation(loc) {
    await get().openFile(loc.file);
    if (!get().buffers[loc.file]) return;
    window.dispatchEvent(new CustomEvent("forge:reveal", { detail: { file: loc.file, line: loc.line, from: loc.col } }));
  },

  setActive: (path) => set({ active: path }),

  isDirty(path) {
    const b = get().buffers[path];
    return !!b && b.text !== b.saved;
  },

  async closeTab(path) {
    if (get().isDirty(path)) {
      const choice = await new Promise<"save" | "discard" | "cancel">((resolve) => set({ dialog: { kind: "unsaved", path, resolve } }));
      set({ dialog: null });
      if (choice === "cancel") return false;
      if (choice === "save" && !(await get().save(path))) return false;
    }
    set((s) => {
      const tabs = s.tabs.filter((p) => p !== path);
      const idx = s.tabs.indexOf(path);
      const active = s.active === path ? (tabs[Math.min(idx, tabs.length - 1)] ?? null) : s.active;
      const { [path]: _gone, ...buffers } = s.buffers;
      return { tabs, active, buffers };
    });
    forgetDocs([path]);
    return true;
  },

  updateBuffer(path, text) {
    const b = get().buffers[path];
    if (!b || b.text === text) return;
    set((s) => ({ buffers: { ...s.buffers, [path]: { ...b, text, version: b.version + 1 } } }));
  },

  async save(path) {
    const p = path ?? get().active;
    if (!p) return true;
    // Ohne Pfad speichert der Nutzer die aktive Datei selbst. Nur dann formatiert der Hook.
    if (path === undefined && beforeSave && get().buffers[p]) await beforeSave(p).catch(() => undefined);
    const b = get().buffers[p];
    if (!b || b.text === b.saved) return true;
    try {
      await backend().writeFile(p, b.text);
      set((s) => ({ buffers: { ...s.buffers, [p]: { ...s.buffers[p], saved: b.text } } }));
      return true;
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.saveFailed(baseName(p)), message: errorText(e), actions: [{ label: t.errors.retry, run: () => void get().save(p) }] });
      return false;
    }
  },

  async saveAll() {
    let ok = true;
    for (const p of get().tabs) ok = (await get().save(p)) && ok;
    return ok;
  },

  async reloadFromDisk(path) {
    const b = get().buffers[path];
    if (!b) return;
    try {
      const text = await backend().readFile(path);
      if (text === b.text) {
        set((s) => ({ buffers: { ...s.buffers, [path]: { ...b, saved: text } } }));
        return;
      }
      set((s) => ({ buffers: { ...s.buffers, [path]: { text, saved: text, revision: b.revision + 1, version: b.version + 1 } } }));
    } catch {
      /* Datei ist weg oder gesperrt. Der Puffer bleibt, wie er ist. */
    }
  },

  async createEntry(dir, isDir) {
    const name = await get().prompt({
      title: isDir ? t.files.newFolderTitle : t.files.newFileTitle,
      label: isDir ? t.files.newFolderPrompt : t.files.newFilePrompt,
      confirm: t.files.create,
      validate: (v) => (v.trim() === "" ? t.files.newFilePrompt : /[/\\]/.test(v) ? "Der Name darf kein / oder \\ enthalten." : null),
    });
    if (!name) return;
    const path = `${dir}/${name.trim()}`;
    try {
      await backend().createFile(path, isDir);
      await get().refreshTree();
      if (!isDir) await get().openFile(path);
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async renameEntry(path) {
    const name = await get().prompt({
      title: t.files.renameTitle,
      label: t.files.newFilePrompt,
      initial: baseName(path),
      confirm: t.files.rename,
      validate: (v) => (v.trim() === "" ? t.files.newFilePrompt : /[/\\]/.test(v) ? "Der Name darf kein / oder \\ enthalten." : null),
    });
    if (!name || name === baseName(path)) return;
    const to = `${dirName(path)}/${name.trim()}`;
    try {
      await get().save(path);
      await backend().renameFile(path, to);
      set((s) => {
        const tabs = s.tabs.map((p) => (p === path ? to : p));
        const { [path]: buf, ...rest } = s.buffers;
        return { tabs, active: s.active === path ? to : s.active, buffers: buf ? { ...rest, [to]: buf } : rest };
      });
      forgetDocs([path]);
      await get().refreshTree();
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  async deleteEntry(path) {
    const ok = await new Promise<boolean>((resolve) => set({ dialog: { kind: "confirmDelete", path, resolve } }));
    set({ dialog: null });
    if (!ok) return;
    try {
      await backend().trashFile(path);
      forgetDocs(Object.keys(get().buffers).filter((p) => p === path || p.startsWith(`${path}/`)));
      set((s) => {
        const gone = (p: string) => p === path || p.startsWith(`${path}/`);
        const tabs = s.tabs.filter((p) => !gone(p));
        const buffers = Object.fromEntries(Object.entries(s.buffers).filter(([p]) => !gone(p)));
        return { tabs, buffers, active: s.active && gone(s.active) ? (tabs[0] ?? null) : s.active };
      });
      await get().refreshTree();
    } catch (e) {
      get().toast({ kind: "error", title: t.errors.generic, message: errorText(e) });
    }
  },

  dismissDiagnostic(d) {
    set((s) => {
      const perSource = s.diagnostics[d.source];
      if (!perSource?.[d.file]) return {};
      return { diagnostics: { ...s.diagnostics, [d.source]: { ...perSource, [d.file]: perSource[d.file].filter((x) => x.id !== d.id) } } };
    });
  },

  setDiagnostics(source, file, list) {
    set((s) => ({ diagnostics: { ...s.diagnostics, [source]: { ...(s.diagnostics[source] ?? {}), [file]: list } } }));
  },
  clearDiagnostics(source) {
    set((s) => ({ diagnostics: { ...s.diagnostics, [source]: {} } }));
  },
  setCursor: (cursor) => set({ cursor }),
  appendOutput: (lines) => set((s) => ({ output: [...s.output, ...lines].slice(-5000) })),
  clearOutput: () => set({ output: [] }),

  setSidebarView(v) {
    const { layout, sidebarView } = get();
    if (v === sidebarView && layout.sidebarOpen) return get().patchLayout({ sidebarOpen: false });
    set({ sidebarView: v });
    if (!layout.sidebarOpen) get().patchLayout({ sidebarOpen: true });
  },
  setSidebarViewOpen(v) {
    set({ sidebarView: v });
    if (!get().layout.sidebarOpen) get().patchLayout({ sidebarOpen: true });
  },
  setBottomTab(bottomTab) {
    set({ bottomTab });
    if (!get().layout.bottomOpen) get().patchLayout({ bottomOpen: true });
  },
  patchLayout(p) {
    const layout = { ...get().layout, ...p };
    writeLayout(layout);
    set({ layout });
  },
  togglePalette: (open, mode) =>
    set((s) => {
      const paletteOpen = open ?? !s.paletteOpen;
      return { paletteOpen, paletteMode: paletteOpen ? (mode ?? "commands") : s.paletteMode };
    }),
  setDialog: (dialog) => set({ dialog }),
  async applyEdits(file, edits) {
    await get().openFile(file);
    // Der Editor wechselt asynchron zur Datei. Der Befehl kommt danach an.
    setTimeout(() => window.dispatchEvent(new CustomEvent("forge:apply-edits", { detail: { file, edits } })), 60);
  },
  confirm(o) {
    return new Promise<boolean>((resolve) => set({ dialog: { kind: "confirm", title: o.title, text: o.text, confirm: o.confirm, tone: o.tone ?? "warning", resolve } })).then((ok) => {
      set({ dialog: null });
      return ok;
    });
  },
  prompt(o) {
    return new Promise((resolve) =>
      set({ dialog: { kind: "prompt", title: o.title, label: o.label, initial: o.initial ?? "", confirm: o.confirm, validate: o.validate, resolve } }),
    ).then((v) => {
      set({ dialog: null });
      return v as string | null;
    });
  },

  toast(item) {
    const id = toastId++;
    const ttl = item.ttl ?? (item.kind === "success" || item.kind === "info" ? 4500 : undefined);
    set((s) => {
      const list = [...s.toasts.filter((x) => x.kind !== "progress" || item.kind !== "progress"), { ...item, id }];
      // Zu viele Toasts: zuerst weichen die ältesten, die keine Fehler sind. Fehler bleiben, bis sie geschlossen werden.
      while (list.length > MAX_TOASTS) {
        const at = list.findIndex((x) => x.kind !== "error" && x.id !== id);
        const [gone] = list.splice(at >= 0 ? at : 0, 1);
        const timer = timers.get(gone.id);
        if (timer) clearTimeout(timer);
        timers.delete(gone.id);
      }
      return { toasts: list };
    });
    if (ttl) timers.set(id, setTimeout(() => get().dismissToast(id), ttl));
    return id;
  },
  updateToast(id, patch) {
    set((s) => ({ toasts: s.toasts.map((x) => (x.id === id ? { ...x, ...patch } : x)) }));
  },
  dismissToast(id) {
    const timer = timers.get(id);
    if (timer) clearTimeout(timer);
    timers.delete(id);
    set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) }));
  },
}));

/** Alle Diagnosen einer Datei, nach Schwere sortiert. */
export function diagnosticsForFile(state: Pick<Store, "diagnostics">, file: string): Diagnostic[] {
  const out: Diagnostic[] = [];
  for (const perFile of Object.values(state.diagnostics)) if (perFile?.[file]) out.push(...perFile[file]);
  return out.sort(bySeverity);
}
