import { Button, Dialog } from "@forge/ui";
import { t } from "../i18n/de";
import { useHardware } from "../lib/hardware";
import { useStore } from "../lib/store";

/** Richtet den ESP32-Core ein, wenn er fehlt. */
export function ToolchainDialog() {
  const setDialog = useStore((s) => s.setDialog);
  const status = useHardware((s) => s.toolchain);
  const setup = useHardware((s) => s.setup);
  const runSetup = useHardware((s) => s.runSetup);
  const noCli = status && !status.arduinoCli;

  return (
    <Dialog
      title={noCli ? t.toolchain.missingCli : t.toolchain.title}
      icon="chip"
      tone={noCli ? "warning" : "glut"}
      onClose={() => !setup.running && setDialog(null)}
      footer={
        noCli ? (
          <Button variant="primary" onClick={() => setDialog(null)}>
            {t.dialogs.ok}
          </Button>
        ) : (
          <>
            <Button onClick={() => setDialog(null)} disabled={setup.running}>
              {t.toolchain.later}
            </Button>
            <Button
              variant="primary"
              busy={setup.running}
              onClick={async () => {
                if (await runSetup()) setDialog(null);
              }}
            >
              {setup.running ? t.toolchain.running : t.toolchain.start}
            </Button>
          </>
        )
      }
    >
      {noCli ? (
        <p>{t.toolchain.missingCliText}</p>
      ) : (
        <>
          <p>{t.toolchain.text}</p>
          {setup.running ? (
            <div className="setup-progress" role="progressbar" aria-valuenow={setup.fraction == null ? undefined : Math.round(setup.fraction * 100)} aria-label={t.toolchain.running}>
              <div className="bar">
                <i style={setup.fraction == null ? undefined : { width: `${Math.round(setup.fraction * 100)}%` }} className={setup.fraction == null ? "is-indeterminate" : undefined} />
              </div>
              <span>{setup.line}</span>
            </div>
          ) : null}
        </>
      )}
    </Dialog>
  );
}
