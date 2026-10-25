import { Button, Dialog } from "@forge/ui";
import { useEffect, useRef, useState } from "react";
import { t } from "../i18n/de";
import { useAi } from "../lib/ai";
import { useStore } from "../lib/store";
import { AiScene, Elapsed } from "./AiSignal";

/** Eine Aufgabe an die KI: Sie liest das Projekt und schreibt den Code. Das Ergebnis erscheint im Editor. */
export function AiTaskDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const hasKey = useAi((s) => s.state?.hasKey ?? false);
  const tasking = useAi((s) => s.tasking);
  const error = useAi((s) => s.taskError);
  const [text, setText] = useState("");
  const input = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    useAi.setState({ taskError: null });
    input.current?.focus();
  }, []);

  const run = async () => {
    if (!text.trim() || tasking) return;
    if (await useAi.getState().task(text)) setDialog(null);
  };

  return (
    <Dialog
      title={t.ai.task}
      icon="spark"
      tone="glut"
      width={600}
      onClose={() => !tasking && setDialog(null)}
      footer={
        <>
          <Button variant="ghost" disabled={tasking} onClick={() => setDialog(null)}>
            {t.dialogs.cancel}
          </Button>
          <Button variant="primary" icon="spark" disabled={!hasKey || !text.trim() || tasking} onClick={() => void run()}>
            {tasking ? t.ai.taskRunning : t.ai.taskRun}
          </Button>
        </>
      }
    >
      <p>{t.ai.taskHint}</p>
      {tasking ? (
        <div className="ai-work" role="status" aria-live="polite">
          <p className="ai-work-task">{text.trim()}</p>
          <AiScene label={t.ai.taskSceneLabel} />
          <div className="ai-work-status">
            <span>{t.ai.taskRunning}</span>
            <Elapsed />
          </div>
        </div>
      ) : (
      <textarea
        ref={input}
        className="ai-task-input"
        value={text}
        rows={6}
        disabled={tasking}
        placeholder={t.ai.taskPlaceholder}
        aria-label={t.ai.task}
        spellCheck
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            void run();
          }
          e.stopPropagation();
        }}
      />
      )}
      {tasking ? null : !hasKey ? (
        <p className="ai-task-note">
          {t.ai.taskNoKey}{" "}
          <Button size="sm" onClick={() => setDialog({ kind: "settings" })}>
            {t.ai.hintAction}
          </Button>
        </p>
      ) : error ? (
        <p className="ai-task-note is-error" role="alert">
          {error}
        </p>
      ) : (
        <p className="ai-task-note">{t.ai.taskSend}</p>
      )}
    </Dialog>
  );
}
