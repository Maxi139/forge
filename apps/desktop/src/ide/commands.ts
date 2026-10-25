import type { Command } from "@forge/ui";
import { t } from "../i18n/de";
import { toggleTheme } from "../lib/theme";
import { useHardware } from "../lib/hardware";
import { useStore } from "../lib/store";
import { findReferences, formatActive, renameSymbol } from "../editor/actions";
import { useFormat } from "../editor/format";
import { useAppUpdate } from "../lib/appUpdate";

export interface RunnableCommand extends Command {
  run(): void;
}

/** Alle Befehle der Palette und der Menüleiste an einer Stelle. */
export function buildCommands(): RunnableCommand[] {
  const s = useStore.getState();
  const hasProject = !!s.project;
  const g = t.commands.groups;

  const list: Array<RunnableCommand | false> = [
    { id: "file.newSketch", group: g.file, label: t.commands.newSketch, icon: "file-code", shortcut: ["⌘", "N"], run: () => void s.createSketch() },
    { id: "file.newProject", group: g.file, label: t.commands.newProject, icon: "plus", shortcut: ["⇧", "⌘", "N"], run: () => s.setDialog({ kind: "newProject" }) },
    { id: "file.open", group: g.file, label: t.commands.openFile, icon: "folder-open", shortcut: ["⌘", "O"], run: () => void s.pickAndOpenFile() },
    { id: "file.openFolder", group: g.file, label: t.commands.openFolder, icon: "folder", run: () => void s.pickAndOpen() },
    hasProject && {
      id: "file.renameProject",
      group: g.file,
      label: t.commands.renameProject,
      icon: "file-code",
      run: () => (document.getElementById("project-name-input") as HTMLInputElement | null)?.focus(),
    },
    hasProject && !s.project?.package && { id: "file.convertToPackage", group: g.file, label: t.commands.convertToPackage, icon: "files", run: () => void s.convertToProject() },
    s.project?.kind === "sketch" && { id: "file.convertToProject", group: g.file, label: t.commands.convertToProject, icon: "plus", run: () => void s.convertToProject() },
    hasProject && { id: "file.saveAs", group: g.file, label: t.commands.saveAs, icon: "files", shortcut: ["⇧", "⌘", "S"], run: () => void s.saveAs() },
    hasProject && !!s.project?.package && { id: "file.exportFolder", group: g.file, label: t.commands.exportFolder, icon: "folder", run: () => void s.exportFolder() },
    hasProject && { id: "file.save", group: g.file, label: t.commands.save, icon: "check", shortcut: ["⌘", "S"], run: () => void s.save() },
    hasProject && { id: "file.saveAll", group: g.file, label: t.commands.saveAll, icon: "files", shortcut: ["⌥", "⌘", "S"], run: () => void s.saveAll() },
    hasProject && !!s.active && { id: "file.close", group: g.file, label: t.commands.closeTab, icon: "close", shortcut: ["⌘", "W"], run: () => s.active && void s.closeTab(s.active) },
    hasProject && { id: "build.compile", group: g.build, label: t.commands.build, icon: "build", shortcut: ["⌘", "B"], run: () => void useHardware.getState().compile() },
    hasProject && { id: "build.flash", group: g.build, label: t.commands.flash, icon: "flash", shortcut: ["⌘", "U"], run: () => void useHardware.getState().flash() },
    hasProject && { id: "build.force", group: g.build, label: t.commands.rebuild, icon: "refresh", run: () => void useHardware.getState().compile({ force: true }) },
    hasProject && { id: "ai.task", group: g.build, label: t.ai.task, icon: "spark", shortcut: ["⇧", "⌘", "I"], run: () => s.setDialog({ kind: "aiTask" }) },
    hasProject && { id: "board.pick", group: g.board, label: t.commands.pickBoard, icon: "chip", run: () => s.setDialog({ kind: "boardPicker" }) },
    { id: "device.rescan", group: g.board, label: t.commands.rescan, icon: "usb", run: () => void useHardware.getState().refreshDevices() },
    hasProject && { id: "secrets.manage", group: g.board, label: t.commands.secrets, icon: "wifi", run: () => s.setDialog({ kind: "secrets" }) },
    { id: "toolchain.setup", group: g.board, label: t.commands.setup, icon: "library", run: () => s.setDialog({ kind: "toolchain" }) },
    hasProject && { id: "view.search", group: g.view, label: t.commands.search, icon: "search", shortcut: ["⇧", "⌘", "F"], run: () => { s.setSidebarViewOpen("search"); s.patchLayout({ sidebarOpen: true }); window.dispatchEvent(new Event("forge:focus-search")); } },
    { id: "view.libraries", group: g.view, label: t.commands.openLibraries, icon: "library", run: () => s.setSidebarView("library") },
    { id: "app.settings", group: g.view, label: t.commands.settings, icon: "settings", shortcut: ["⌘", ","], run: () => s.setDialog({ kind: "settings" }) },
    { id: "view.theme", group: g.view, label: t.commands.toggleTheme, icon: "settings", run: toggleTheme },
    { id: "view.panel", group: g.view, label: t.commands.togglePanel, icon: "split", shortcut: ["⌘", "J"], run: () => s.patchLayout({ bottomOpen: !s.layout.bottomOpen }) },
    { id: "view.sidebar", group: g.view, label: t.commands.toggleSidebar, icon: "files", shortcut: ["⌘", "\\"], run: () => s.patchLayout({ sidebarOpen: !s.layout.sidebarOpen }) },
    { id: "view.palette", group: g.view, label: t.commands.palette, icon: "command", shortcut: ["⌘", "K"], run: () => s.togglePalette(true) },
    { id: "view.device", group: g.view, label: t.commands.toggleDevice, icon: "device", shortcut: ["⌥", "⌘", "D"], run: () => s.patchLayout({ deviceOpen: !s.layout.deviceOpen }) },
    { id: "view.problems", group: g.view, label: t.commands.openProblems, icon: "warning", run: () => s.setBottomTab("problems") },
    { id: "view.monitor", group: g.view, label: t.commands.openMonitor, icon: "serial", shortcut: ["⇧", "⌘", "M"], run: () => s.setBottomTab("monitor") },
    hasProject && { id: "file.quickOpen", group: g.file, label: t.commands.quickOpen, icon: "search", shortcut: ["⌘", "P"], run: () => s.togglePalette(true, "files") },
    hasProject && !!s.active && { id: "editor.symbols", group: g.file, label: t.commands.gotoSymbol, icon: "file-code", shortcut: ["⇧", "⌘", "O"], run: () => s.togglePalette(true, "symbols") },
    hasProject && !!s.active && {
      id: "editor.format",
      group: g.file,
      label: t.commands.formatDocument,
      icon: "check",
      shortcut: ["⇧", "⌥", "F"],
      hint: useFormat.getState().available === false ? t.editorTools.formatMissing : undefined,
      run: formatActive,
    },
    hasProject && !!s.active && { id: "editor.rename", group: g.file, label: t.commands.renameSymbol, icon: "file-code", shortcut: ["F2"], run: () => void renameSymbol() },
    hasProject && !!s.active && { id: "editor.references", group: g.file, label: t.commands.findReferences, icon: "search", shortcut: ["⇧", "F12"], run: () => void findReferences() },
    { id: "example.open", group: g.file, label: t.examples.command, icon: "file-code", run: () => s.setDialog({ kind: "examples" }) },
    { id: "library.installZip", group: g.view, label: t.updates.installZip, icon: "library", run: () => void useHardware.getState().installLibraryZip() },
    { id: "app.checkUpdate", group: g.view, label: t.appUpdate.command, icon: "refresh", run: () => void useAppUpdate.getState().check() },
  ];
  return list.filter((c): c is RunnableCommand => c !== false);
}
