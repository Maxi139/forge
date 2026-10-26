import { Button, Dialog, Input, Select } from "@forge/ui";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { t } from "../i18n/de";
import { backend, type TemplateId } from "../lib/backend";
import { FALLBACK_BOARDS } from "../lib/boards";
import { baseName, errorText, isCancelled, useStore } from "../lib/store";
import { AiTaskDialog } from "./AiTaskDialog";
import { WebEmbedDialog } from "./WebEmbedDialog";
import { BoardPicker } from "./BoardPicker";
import { ExamplesDialog } from "./ExamplesDialog";
import { SecretsDialog, MoveSecretDialog } from "./SecretsDialog";
import { SettingsDialog } from "./SettingsDialog";
import { ToolchainDialog } from "./ToolchainDialog";

export function Dialogs() {
  const dialog = useStore((s) => s.dialog);
  if (!dialog) return null;
  switch (dialog.kind) {
    case "newProject":
      return <NewProjectDialog />;
    case "boardPicker":
      return <BoardPicker />;
    case "toolchain":
      return <ToolchainDialog />;
    case "examples":
      return <ExamplesDialog />;
    case "settings":
      return <SettingsDialog />;
    case "secrets":
      return <SecretsDialog />;
    case "aiTask":
      return <AiTaskDialog />;
    case "webEmbed":
      return <WebEmbedDialog source={dialog.source} target={dialog.target} />;
    case "moveSecret":
      return <MoveSecretDialog {...dialog} />;
    case "unsaved":
      return <UnsavedDialog path={dialog.path} resolve={dialog.resolve} />;
    case "confirmDelete":
      return <DeleteDialog path={dialog.path} resolve={dialog.resolve} />;
    case "confirm":
      return <ConfirmDialog {...dialog} />;
    case "prompt":
      return <PromptDialog {...dialog} />;
  }
}

/* ---------- Ungespeicherte Änderungen ---------- */
function UnsavedDialog({ path, resolve }: { path: string; resolve(c: "save" | "discard" | "cancel"): void }) {
  const name = baseName(path);
  return (
    <Dialog
      title={t.dialogs.unsavedTitle(name)}
      icon="warning"
      tone="warning"
      onClose={() => resolve("cancel")}
      footer={
        <>
          <Button variant="ghost" onClick={() => resolve("discard")}>
            {t.dialogs.discard}
          </Button>
          <span className="fg-spacer" />
          <Button onClick={() => resolve("cancel")}>{t.dialogs.cancel}</Button>
          <Button variant="primary" onClick={() => resolve("save")} autoFocus>
            {t.dialogs.save}
          </Button>
        </>
      }
    >
      <p>{t.dialogs.unsavedText}</p>
    </Dialog>
  );
}

/* ---------- Löschen ---------- */
function DeleteDialog({ path, resolve }: { path: string; resolve(ok: boolean): void }) {
  return (
    <Dialog
      title={t.dialogs.deleteTitle(baseName(path))}
      icon="trash"
      tone="error"
      onClose={() => resolve(false)}
      footer={
        <>
          <Button onClick={() => resolve(false)}>{t.dialogs.cancel}</Button>
          <Button variant="danger" onClick={() => resolve(true)} autoFocus>
            {t.dialogs.delete}
          </Button>
        </>
      }
    >
      <p>{t.dialogs.deleteText}</p>
    </Dialog>
  );
}

/* ---------- Allgemeine Rückfrage ---------- */
function ConfirmDialog({ title, text, confirm, tone, resolve }: { title: string; text: string; confirm: string; tone: "error" | "warning" | "glut"; resolve(ok: boolean): void }) {
  return (
    <Dialog
      title={title}
      icon={tone === "error" ? "erase" : "warning"}
      tone={tone}
      onClose={() => resolve(false)}
      footer={
        <>
          <Button onClick={() => resolve(false)}>{t.dialogs.cancel}</Button>
          <Button variant={tone === "error" ? "danger" : "primary"} onClick={() => resolve(true)} autoFocus>
            {confirm}
          </Button>
        </>
      }
    >
      <p>{text}</p>
    </Dialog>
  );
}

/* ---------- Eingabe eines Namens ---------- */
function PromptDialog(p: { title: string; label: string; initial: string; confirm: string; validate?(v: string): string | null; resolve(v: string | null): void }) {
  const [value, setValue] = useState(p.initial);
  const [touched, setTouched] = useState(false);
  const ref = useRef<HTMLInputElement>(null);
  const error = p.validate?.(value) ?? null;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    // Beim Umbenennen den Namen ohne Endung markieren.
    const dot = p.initial.lastIndexOf(".");
    el.setSelectionRange(0, dot > 0 ? dot : p.initial.length);
  }, [p.initial]);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    if (!error) p.resolve(value.trim());
  };

  return (
    <Dialog
      title={p.title}
      width={400}
      onClose={() => p.resolve(null)}
      footer={
        <>
          <Button onClick={() => p.resolve(null)}>{t.dialogs.cancel}</Button>
          <Button variant="primary" type="submit" form="prompt-form" disabled={!!error && touched}>
            {p.confirm}
          </Button>
        </>
      }
    >
      <form id="prompt-form" onSubmit={submit}>
        <Input ref={ref} label={p.label} value={value} onChange={(e) => setValue(e.target.value)} error={touched ? (error ?? undefined) : undefined} spellCheck={false} autoComplete="off" />
      </form>
    </Dialog>
  );
}

/* ---------- Neues Projekt ---------- */
const TEMPLATE_OPTIONS: Array<{ value: TemplateId; label: string }> = [
  { value: "blank", label: t.newProject.templates.blank },
  { value: "blink", label: t.newProject.templates.blink },
  { value: "wifi", label: t.newProject.templates.wifi },
  { value: "webserver", label: t.newProject.templates.webserver },
];

function NewProjectDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const createProject = useStore((s) => s.createProject);
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [fqbn, setFqbn] = useState(FALLBACK_BOARDS[0].fqbn);
  const [template, setTemplate] = useState<TemplateId>("blink");
  const [nameError, setNameError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [touched, setTouched] = useState(false);

  useEffect(() => {
    void backend().defaultProjectsDir().then(setParent);
  }, []);

  useEffect(() => {
    if (!name) return setNameError(null);
    let alive = true;
    void backend()
      .validateName(name)
      .then((e) => alive && setNameError(e));
    return () => {
      alive = false;
    };
  }, [name]);

  const close = () => !busy && setDialog(null);

  const choose = async () => {
    const dir = await backend().pickFolder();
    if (dir) setParent(dir);
  };

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setTouched(true);
    setFormError(null);
    if (!name || nameError || !parent) return;
    setBusy(true);
    try {
      await createProject({ name, parent, fqbn, template, git: false });
      setDialog(null);
    } catch (err) {
      const message = errorText(err);
      if (!isCancelled(err)) setFormError(message);
      setBusy(false);
    }
  };

  const shownNameError = nameError ?? (touched && !name ? "Gib einen Namen ein." : null);

  return (
    <Dialog
      title={t.newProject.title}
      icon="plus"
      tone="glut"
      width={480}
      onClose={close}
      footer={
        <>
          <Button onClick={close} disabled={busy}>
            {t.newProject.cancel}
          </Button>
          <Button variant="primary" type="submit" form="new-project-form" busy={busy}>
            {busy ? t.newProject.creating : t.newProject.create}
          </Button>
        </>
      }
    >
      <form id="new-project-form" className="form" onSubmit={submit} noValidate>
        <Input label={t.newProject.name} value={name} onChange={(e) => setName(e.target.value)} placeholder={t.newProject.namePlaceholder} error={formError ?? shownNameError ?? undefined} autoFocus spellCheck={false} autoComplete="off" mono />
        <div className="folder-field">
          <Input label={t.newProject.location} value={parent} readOnly mono title={parent} />
          <Button onClick={() => void choose()} type="button" disabled={busy}>
            {t.newProject.choose}
          </Button>
        </div>
        <div className="form-row">
          <Select label={t.newProject.board} value={fqbn} onChange={(e) => setFqbn(e.target.value)} options={FALLBACK_BOARDS.map((b) => ({ value: b.fqbn, label: b.name }))} />
          <Select label={t.newProject.template} value={template} onChange={(e) => setTemplate(e.target.value as TemplateId)} options={TEMPLATE_OPTIONS} />
        </div>
        {name && !nameError ? <p className="form-hint">{t.newProject.fileNote(name)}</p> : null}
      </form>
    </Dialog>
  );
}
