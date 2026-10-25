import { Button, EmptyState, Icon } from "@forge/ui";
import { t } from "../i18n/de";
import { useStore } from "../lib/store";

function shortPath(p: string): string {
  const parts = p.split("/").filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}

export function Welcome() {
  const recents = useStore((s) => s.recents);
  const openProject = useStore((s) => s.openProject);
  const pickAndOpenFile = useStore((s) => s.pickAndOpenFile);
  const createSketch = useStore((s) => s.createSketch);
  const setDialog = useStore((s) => s.setDialog);

  return (
    <EmptyState
      icon="chip"
      title={t.welcome.title}
      text={t.welcome.text}
      lit={3}
      actions={
        <>
          <Button variant="primary" icon="plus" onClick={() => setDialog({ kind: "newProject" })}>
            {t.welcome.newProject}
          </Button>
          <Button icon="file-code" onClick={() => void createSketch()}>
            {t.welcome.newSketch}
          </Button>
          <Button icon="folder-open" onClick={() => void pickAndOpenFile()}>
            {t.welcome.openFolder}
          </Button>
          <Button icon="library" onClick={() => setDialog({ kind: "examples" })}>
            {t.examples.welcome}
          </Button>
        </>
      }
      hints={[
        { keys: ["⌘", "N"], label: t.welcome.hintSketch },
        { keys: ["⌘", "O"], label: t.welcome.hintOpen },
        { keys: ["⌘", "K"], label: t.welcome.hintCommands },
      ]}
    >
      {recents.length ? (
        <div className="welcome-recent">
          <h4>{t.welcome.recent}</h4>
          {recents.slice(0, 5).map((r) => (
            <button key={r.path} className="welcome-item" onClick={() => void openProject(r.path)} title={r.path}>
              <Icon name={r.kind === "project" ? "folder" : "file-code"} size={14} />
              <span className="name">{r.name}</span>
              <span className="where">{shortPath(r.path.slice(0, r.path.lastIndexOf("/")))}</span>
            </button>
          ))}
        </div>
      ) : null}
    </EmptyState>
  );
}
