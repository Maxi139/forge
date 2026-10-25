import { Badge, Button, Checkbox, Dialog, Input } from "@forge/ui";
import { useEffect, useState, type FormEvent } from "react";
import { t } from "../i18n/de";
import { useHardware } from "../lib/hardware";
import { useStore } from "../lib/store";

const NAME = /^[A-Za-z0-9_]{1,15}$/;

/** Secrets eines Projekts: Werte eintragen, die nie im Code stehen. */
export function SecretsDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const secrets = useHardware((s) => s.secrets);
  const refresh = useHardware((s) => s.refreshSecrets);
  const setSecret = useHardware((s) => s.setSecret);
  const removeSecret = useHardware((s) => s.removeSecret);
  const [values, setValues] = useState<Record<string, string>>({});
  const [global, setGlobal] = useState<Record<string, boolean>>({});
  const [pending, setPending] = useState<string[]>([]);
  const [newName, setNewName] = useState("");
  const [nameError, setNameError] = useState<string | undefined>();

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const known = new Set(secrets.map((s) => s.name));
  const names = [...secrets.map((s) => s.name), ...pending.filter((p) => !known.has(p))];

  const add = (e: FormEvent) => {
    e.preventDefault();
    const n = newName.trim().toUpperCase();
    if (!NAME.test(n)) return setNameError(t.secrets.invalidName);
    setNameError(undefined);
    if (!names.includes(n)) setPending((p) => [...p, n]);
    setNewName("");
  };

  const save = async (name: string) => {
    const v = values[name];
    if (!v) return;
    const status = secrets.find((s) => s.name === name);
    const asGlobal = global[name] ?? status?.scope === "global";
    if (await setSecret(name, v, asGlobal)) setValues((x) => ({ ...x, [name]: "" }));
  };

  return (
    <Dialog
      title={t.secrets.title}
      icon="wifi"
      tone="glut"
      width={560}
      onClose={() => setDialog(null)}
      footer={
        <Button variant="primary" onClick={() => setDialog(null)}>
          {t.dialogs.ok}
        </Button>
      }
    >
      <p>{t.secrets.hint}</p>
      <div className="secret-list">
        {names.length === 0 ? <div className="picker-empty">{t.secrets.none}</div> : null}
        {names.map((name) => {
          const st = secrets.find((s) => s.name === name);
          return (
            <div key={name} className="secret-row">
              <div className="secret-head">
                <span className="fg-mono secret-name">{name}</span>
                {st?.set ? <Badge tone="success" icon="check">{t.secrets.set}</Badge> : <Badge tone="warning" icon="warning">{t.secrets.missing}</Badge>}
                {st?.scope === "global" ? <Badge>{t.secrets.allProjectsBadge}</Badge> : null}
                {st && !st.used ? <span className="secret-note">{t.secrets.unused}</span> : null}
              </div>
              <form
                className="secret-edit"
                onSubmit={(e) => {
                  e.preventDefault();
                  void save(name);
                }}
              >
                <Input type="password" size="sm" aria-label={`${t.secrets.value} ${name}`} placeholder={st?.set ? t.secrets.newValue : t.secrets.value} value={values[name] ?? ""} onChange={(e) => setValues((x) => ({ ...x, [name]: e.target.value }))} autoComplete="off" mono />
                <Checkbox label={t.secrets.allProjects} checked={global[name] ?? st?.scope === "global"} onChange={(e) => setGlobal((x) => ({ ...x, [name]: e.target.checked }))} />
                <Button size="sm" type="submit" disabled={!values[name]}>
                  {t.secrets.save}
                </Button>
                {st ? <Button size="sm" variant="ghost" icon="trash" aria-label={`${name} ${t.secrets.remove}`} title={t.secrets.remove} onClick={() => void removeSecret(name)} /> : null}
              </form>
            </div>
          );
        })}
      </div>
      <form className="secret-add" onSubmit={add}>
        <Input size="sm" mono aria-label={t.secrets.name} placeholder={t.secrets.namePlaceholder} value={newName} onChange={(e) => setNewName(e.target.value)} error={nameError} autoComplete="off" spellCheck={false} />
        <Button size="sm" type="submit" icon="plus">
          {t.secrets.add}
        </Button>
      </form>
    </Dialog>
  );
}

/** Ein Klartext-Wert aus dem Code wandert in den Schlüsselbund. */
export function MoveSecretDialog(p: { file: string; line: number; from: number; to: number; value: string; name: string; mode: "call" | "decl" }) {
  const setDialog = useStore((s) => s.setDialog);
  const setSecret = useHardware((s) => s.setSecret);
  const [name, setName] = useState(p.name);
  const [global, setGlobal] = useState(p.name.startsWith("WIFI"));
  const valid = NAME.test(name);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!valid) return;
    if (!(await setSecret(name, p.value, global))) return;
    const store = useStore.getState();
    const text = store.buffers[p.file]?.text ?? "";
    const edits = [{ fromLine: p.line, fromCol: p.from, toLine: p.line, toCol: p.to, text: p.mode === "call" ? `Secret("${name}").c_str()` : '""' }];
    if (p.mode === "call" && !/#include\s*<ForgeSecrets\.h>/.test(text)) edits.push({ fromLine: 1, fromCol: 0, toLine: 1, toCol: 0, text: "#include <ForgeSecrets.h>\n" });
    setDialog(null);
    await store.applyEdits(p.file, edits);
    store.toast({ kind: "success", title: t.secrets.moved(name) });
  };

  return (
    <Dialog
      title={t.secrets.moveTitle}
      icon="wifi"
      tone="glut"
      width={440}
      onClose={() => setDialog(null)}
      footer={
        <>
          <Button onClick={() => setDialog(null)}>{t.dialogs.cancel}</Button>
          <Button variant="primary" type="submit" form="move-secret" disabled={!valid}>
            {t.secrets.moveConfirm}
          </Button>
        </>
      }
    >
      <form id="move-secret" className="form" onSubmit={submit}>
        <Input label={t.secrets.name} value={name} onChange={(e) => setName(e.target.value.toUpperCase())} error={valid ? undefined : t.secrets.invalidName} mono autoFocus autoComplete="off" spellCheck={false} />
        <Checkbox label={t.secrets.allProjects} checked={global} onChange={(e) => setGlobal(e.target.checked)} />
        <p style={{ margin: 0 }}>{(p.mode === "call" ? t.secrets.moveCall : t.secrets.moveDecl).replace("NAME", name)}</p>
      </form>
    </Dialog>
  );
}
