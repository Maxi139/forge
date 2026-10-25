import { Button, EmptyState } from "@forge/ui";
import { Component, type ReactNode } from "react";
import { t } from "../i18n/de";
import { reportError } from "../lib/log";
import { stashUnsaved } from "../lib/store";

interface Props {
  children: ReactNode;
  /** Ein Bereich der Oberfläche: Der Fehler bleibt dort, der Rest läuft weiter, und nichts lädt neu. */
  region?: boolean;
}

interface State {
  error: Error | null;
}

/** Ein Fehler beim Zeichnen darf nicht das ganze Fenster leeren. */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  componentDidCatch(error: Error) {
    reportError("Oberfläche abgestürzt", error);
  }

  render() {
    if (!this.state.error) return this.props.children;
    const { region } = this.props;
    return (
      <div style={{ height: "100%", minWidth: 0 }}>
        <EmptyState
          icon="error"
          title={t.errors.crashTitle}
          text={this.state.error.message}
          actions={
            region ? (
              <Button variant="primary" onClick={() => this.setState({ error: null })}>
                {t.errors.reloadRegion}
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={() => {
                  // Ungespeicherte Texte überstehen das Neuladen und lassen sich danach zurückholen.
                  stashUnsaved();
                  window.location.reload();
                }}
              >
                {t.errors.reload}
              </Button>
            )
          }
        />
      </div>
    );
  }
}
