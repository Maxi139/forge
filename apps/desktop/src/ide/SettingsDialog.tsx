import { Badge, Button, Checkbox, Dialog, Input, Select } from "@forge/ui";
import { useEffect, useState } from "react";
import { EDITOR_FONT_SIZES, TAB_WIDTHS, useEditorSettings, type Autosave } from "../editor/settings";
import { useFormat } from "../editor/format";
import { t } from "../i18n/de";
import { backend, type FbuildStatus, type McpStatus } from "../lib/backend";
import { useAi } from "../lib/ai";
import { errorText, useStore } from "../lib/store";
import { setThemePreference, useTheme, type ThemePreference } from "../lib/theme";

function tokens(n: number): string {
  return n.toLocaleString("de-DE");
}

function AiSection() {
  const state = useAi((s) => s.state);
  const models = useAi((s) => s.models);
  const project = useStore((s) => s.project);
  const [key, setKey] = useState("");
  const [model, setModel] = useState("");
  const [budget, setBudget] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    void useAi.getState().refresh();
  }, []);
  useEffect(() => {
    if (!state) return;
    setModel(state.modelIsDefault ? "" : state.model);
    setBudget(String(state.dailyTokens));
  }, [state?.model, state?.modelIsDefault, state?.dailyTokens]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!state) return null;
  const ai = useAi.getState();
  const budgetNumber = Number.parseInt(budget.replace(/\D/g, ""), 10);

  return (
    <section className="settings-group" aria-label={t.ai.title}>
      <h3 className="settings-heading">{t.ai.title}</h3>
      <div className="form">
        {state.hasKey ? (
          <div className="settings-key">
            <span className="settings-key-label">{t.ai.keySet}</span>
            <Button variant="ghost" size="sm" onClick={() => void ai.configure({ key: "" })}>
              {t.ai.keyRemove}
            </Button>
          </div>
        ) : (
          <Input
            label={t.ai.key}
            type="password"
            mono
            autoComplete="off"
            spellCheck={false}
            placeholder={t.ai.keyPlaceholder}
            value={key}
            onChange={(e) => setKey(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && key.trim()) {
                void ai.configure({ key });
                setKey("");
              }
            }}
            trailing={
              <Button
                size="sm"
                variant="secondary"
                disabled={!key.trim()}
                onClick={() => {
                  void ai.configure({ key });
                  setKey("");
                }}
              >
                {t.ai.keySave}
              </Button>
            }
          />
        )}

        <Input
          label={t.ai.model}
          mono
          list="ai-models"
          spellCheck={false}
          placeholder={state.modelIsDefault ? state.model : t.ai.modelAuto}
          value={model}
          onChange={(e) => setModel(e.target.value)}
          onBlur={() => {
            if (model.trim() !== (state.modelIsDefault ? "" : state.model)) void ai.configure({ model });
          }}
          trailing={
            <Button
              size="sm"
              variant="secondary"
              busy={loading}
              onClick={async () => {
                setLoading(true);
                await ai.loadModels();
                setLoading(false);
                const pick = useAi.getState().suggested;
                if (pick) {
                  setModel(pick);
                  void ai.configure({ model: pick });
                }
              }}
            >
              {loading ? t.ai.modelLoading : t.ai.modelPick}
            </Button>
          }
        />
        <datalist id="ai-models">
          {models.slice(0, 80).map((m) => (
            <option key={m.id} value={m.id}>
              {m.name}
            </option>
          ))}
        </datalist>

        <Input
          label={t.ai.budget}
          mono
          inputMode="numeric"
          value={budget}
          onChange={(e) => setBudget(e.target.value)}
          onBlur={() => {
            if (Number.isFinite(budgetNumber) && budgetNumber > 0 && budgetNumber !== state.dailyTokens) void ai.configure({ dailyTokens: budgetNumber });
            else setBudget(String(state.dailyTokens));
          }}
          hint={t.ai.used(tokens(state.usedToday))}
        />

        {project ? (
          <Checkbox label={t.ai.projectOn} checked={state.projectEnabled} onChange={(e) => void ai.setProjectEnabled(e.target.checked)} />
        ) : null}
      </div>
    </section>
  );
}

function BuildSection() {
  const toast = useStore((st) => st.toast);
  const [status, setStatus] = useState<FbuildStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [line, setLine] = useState("");

  const load = () => backend().fbuildStatus().then(setStatus, () => setStatus(null));
  useEffect(() => {
    void load();
  }, []);

  if (!status) return null;
  const install = async () => {
    setBusy(true);
    const off = await backend().onToolchainProgress((p) => setLine(p.line));
    try {
      await backend().fbuildInstall();
      await load();
    } catch (e) {
      toast({ kind: "error", title: t.build.installFailed, message: errorText(e) });
    } finally {
      off();
      setBusy(false);
      setLine("");
    }
  };

  return (
    <section className="settings-group" aria-label={t.build.title}>
      <h3 className="settings-heading">{t.build.title}</h3>
      <div className="form">
        {status.installed ? (
          <>
            <Checkbox
              label={t.build.fbuild}
              checked={status.enabled}
              onChange={(e) => void backend().fbuildSetEnabled(e.target.checked).then(load)}
            />
            <div className="settings-note">
              {status.version ? `${t.build.version(status.version)}. ` : ""}
              {t.build.boards(status.boards.join(", "))}
            </div>
          </>
        ) : (
          <div className="settings-actions">
            <Button variant="secondary" size="sm" busy={busy} onClick={() => void install()}>
              {busy ? t.build.installing : t.build.install}
            </Button>
            <span className="settings-note">{busy ? line.slice(0, 60) : t.build.size}</span>
          </div>
        )}
      </div>
    </section>
  );
}

function ClaudeSection() {
  const project = useStore((s) => s.project);
  const toast = useStore((s) => s.toast);
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => backend().mcpStatus().then(setStatus, () => setStatus(null));
  useEffect(() => {
    void load();
  }, [project?.path]);

  if (!status) return null;
  const fail = (e: unknown) => toast({ kind: "error", title: t.errors.generic, message: errorText(e) });

  return (
    <section className="settings-group" aria-label={t.claude.title}>
      <h3 className="settings-heading">{t.claude.title}</h3>
      <div className="form">
        <Checkbox
          label={t.claude.allow}
          checked={status.enabled}
          onChange={(e) => void backend().mcpSetEnabled(e.target.checked).then(load, fail)}
        />
        <div className="settings-actions">
          {status.connected ? <Badge tone="success" icon="check">{t.claude.connected}</Badge> : null}
          <Button
            variant="secondary"
            size="sm"
            busy={busy}
            disabled={!project}
            onClick={async () => {
              setBusy(true);
              try {
                await backend().mcpConnect();
                toast({ kind: "success", title: t.claude.connectedToast, message: t.claude.connectedText, ttl: 5000 });
                await load();
              } catch (e) {
                fail(e);
              } finally {
                setBusy(false);
              }
            }}
          >
            {status.connected ? t.claude.reconnect : t.claude.connect}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={!project}
            onClick={() => void backend().mcpOpenInClaude().catch((e) => toast({ kind: "error", title: t.claude.openFailed, message: errorText(e) }))}
          >
            {t.claude.open}
          </Button>
        </div>
      </div>
    </section>
  );
}

function EditorSection() {
  const settings = useEditorSettings((st) => st.settings);
  const patch = useEditorSettings((st) => st.patch);
  const formatReady = useFormat((st) => st.available);
  const et = t.editorTools;
  useEffect(() => {
    void useFormat.getState().check();
  }, []);
  return (
    <section className="settings-group" aria-label={et.section}>
      <h3 className="settings-heading">{et.section}</h3>
      <div className="form">
        <Select
          label={et.fontSize}
          value={String(settings.fontSize)}
          onChange={(e) => patch({ fontSize: Number(e.target.value) })}
          options={[...new Set([...EDITOR_FONT_SIZES, settings.fontSize])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: et.pt(n) }))}
        />
        <Select
          label={et.tabWidth}
          value={String(settings.tabWidth)}
          onChange={(e) => patch({ tabWidth: Number(e.target.value) })}
          options={[...new Set([...TAB_WIDTHS, settings.tabWidth])].sort((a, b) => a - b).map((n) => ({ value: String(n), label: et.spaces(n) }))}
        />
        <Checkbox label={et.wrap} checked={settings.wrap} onChange={(e) => patch({ wrap: e.target.checked })} />
        <Select
          label={et.autosave}
          value={settings.autosave}
          onChange={(e) => patch({ autosave: e.target.value as Autosave })}
          options={[
            { value: "off", label: et.autosaveOff },
            { value: "delay", label: et.autosaveDelay },
            { value: "blur", label: et.autosaveBlur },
          ]}
        />
        <Checkbox label={et.formatOnSave} checked={settings.formatOnSave} disabled={formatReady === false} onChange={(e) => patch({ formatOnSave: e.target.checked })} />
        {formatReady === false ? <div className="settings-note">{`${et.formatMissing}. ${et.formatMissingText}`}</div> : null}
      </div>
    </section>
  );
}

export function SettingsDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const { preference } = useTheme();
  return (
    <Dialog
      title={t.settings.title}
      icon="settings"
      width={500}
      onClose={() => setDialog(null)}
      footer={
        <Button variant="primary" onClick={() => setDialog(null)}>
          {t.dialogs.ok}
        </Button>
      }
    >
      <div className="settings">
        <div className="form">
          <Select
            label={t.settings.theme}
            value={preference}
            onChange={(e) => setThemePreference(e.target.value as ThemePreference)}
            options={[
              { value: "system", label: t.settings.themeSystem },
              { value: "dark", label: t.settings.themeDark },
              { value: "light", label: t.settings.themeLight },
            ]}
          />
        </div>
        <EditorSection />
        <AiSection />
        <BuildSection />
        <ClaudeSection />
      </div>
    </Dialog>
  );
}
