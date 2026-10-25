import { Button, ContextMenu, FileTree, Panel, type MenuItem, type TreeNode } from "@forge/ui";
import { useCallback, useMemo, useState } from "react";
import { t } from "../i18n/de";
import type { FileNode } from "../lib/backend";
import { baseName, useStore } from "../lib/store";

const CODE_EXT = /\.(ino|cpp|c|h|hpp|cc)$/i;

function iconFor(name: string): TreeNode["icon"] {
  if (CODE_EXT.test(name)) return "file-code";
  if (name === "forge.toml") return "settings";
  if (/^partitions.*\.csv$/i.test(name)) return "partition";
  return "file";
}

interface Menu {
  x: number;
  y: number;
  path: string;
  isDir: boolean;
}

export function Explorer() {
  const project = useStore((s) => s.project)!;
  const tree = useStore((s) => s.tree);
  const active = useStore((s) => s.active);
  const buffers = useStore((s) => s.buffers);
  const diagnostics = useStore((s) => s.diagnostics);
  const openFile = useStore((s) => s.openFile);
  const refreshTree = useStore((s) => s.refreshTree);
  const createEntry = useStore((s) => s.createEntry);
  const renameEntry = useStore((s) => s.renameEntry);
  const deleteEntry = useStore((s) => s.deleteEntry);
  const [menu, setMenu] = useState<Menu | null>(null);

  const errorFiles = useMemo(() => {
    const set = new Set<string>();
    for (const perFile of Object.values(diagnostics)) {
      for (const [file, list] of Object.entries(perFile ?? {})) if (list.some((d) => d.severity === "error")) set.add(file);
    }
    return set;
  }, [diagnostics]);

  const nodes = useMemo(() => {
    const convert = (n: FileNode): TreeNode => {
      if (n.isDir) return { name: n.name, children: (n.children ?? []).map(convert) };
      const b = buffers[n.path];
      const dirty = !!b && b.text !== b.saved;
      return {
        name: n.name,
        icon: iconFor(n.name),
        status: errorFiles.has(n.path) ? "error" : dirty ? "modified" : undefined,
        badge: errorFiles.has(n.path) ? "!" : dirty ? "M" : undefined,
      };
    };
    return tree.map(convert);
  }, [tree, buffers, errorFiles]);

  const pathOf = useCallback((treePath: string) => `${project.path}${treePath}`, [project.path]);

  const items = (m: Menu): MenuItem[] => [
    ...(m.isDir
      ? ([
          { id: "new-file", label: t.sidebar.newFile, icon: "plus" },
          { id: "new-folder", label: t.sidebar.newFolder, icon: "folder" },
        ] as MenuItem[])
      : []),
    { id: "rename", label: t.files.rename, icon: "file", separatorBefore: m.isDir },
    { id: "delete", label: t.files.remove, icon: "trash", danger: true },
  ];

  const run = (id: string, m: Menu) => {
    setMenu(null);
    if (id === "new-file") void createEntry(m.path, false);
    else if (id === "new-folder") void createEntry(m.path, true);
    else if (id === "rename") void renameEntry(m.path);
    else if (id === "delete") void deleteEntry(m.path);
  };

  return (
    <Panel
      title={t.sidebar.files}
      actions={
        <div className="sb-actions">
          <Button variant="ghost" size="sm" icon="plus" aria-label={t.sidebar.newFile} title={t.sidebar.newFile} onClick={() => void createEntry(project.path, false)} />
          <Button variant="ghost" size="sm" icon="folder" aria-label={t.sidebar.newFolder} title={t.sidebar.newFolder} onClick={() => void createEntry(project.path, true)} />
          <Button variant="ghost" size="sm" icon="refresh" aria-label={t.sidebar.refresh} title={t.sidebar.refresh} onClick={() => void refreshTree()} />
        </div>
      }
    >
      <FileTree
        key={project.path}
        nodes={nodes}
        selected={active ? active.slice(project.path.length) : undefined}
        onSelect={(p, n) => !n.children && void openFile(pathOf(p))}
        onContextMenu={(p, n, e) => {
          e.preventDefault();
          setMenu({ x: e.clientX, y: e.clientY, path: pathOf(p), isDir: !!n.children });
        }}
      />
      {menu ? <ContextMenu x={menu.x} y={menu.y} items={items(menu)} onSelect={(id) => run(id, menu)} onClose={() => setMenu(null)} /> : null}
    </Panel>
  );
}

export function projectRelative(root: string, path: string): string {
  return path.startsWith(`${root}/`) ? path.slice(root.length + 1) : baseName(path);
}
