import "@forge/ui";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ErrorBoundary } from "./ide/ErrorBoundary";
import { useAi } from "./lib/ai";
import { useHardware } from "./lib/hardware";
import { installErrorHandlers } from "./lib/log";
import { useStore } from "./lib/store";
import "./app.css";

installErrorHandlers();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </StrictMode>,
);

if (import.meta.env.DEV) {
  Object.assign(window, { __forge: useStore, __hw: useHardware, __ai: useAi });
}
