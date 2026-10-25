export type Severity = "error" | "warning" | "info";
export type DiagnosticSource = "syntax" | "compiler" | "clangd" | "analysis" | "ai";

/** Ersetzt den Text zwischen zwei Positionen (Zeilen 1-basiert, Spalten 0-basiert). */
export interface TextEdit {
  fromLine: number;
  fromCol: number;
  toLine: number;
  toCol: number;
  text: string;
}

export interface QuickFix {
  label: string;
  edits: TextEdit[];
}

/** Eine Aktion, die kein Texteingriff ist, etwa „Bibliothek suchen“. */
export interface DiagnosticAction {
  kind: string;
  arg: string;
  label: string;
}

export interface Diagnostic {
  id: string;
  file: string;
  /** 1-basiert. */
  line: number;
  /** Spalten, 0-basiert, `to` ist exklusiv. */
  from: number;
  to: number;
  severity: Severity;
  message: string;
  source: DiagnosticSource;
  fix?: QuickFix;
  action?: DiagnosticAction;
}

const RANK: Record<Severity, number> = { error: 0, warning: 1, info: 2 };

export function bySeverity(a: Diagnostic, b: Diagnostic): number {
  return RANK[a.severity] - RANK[b.severity] || a.line - b.line || a.from - b.from;
}

export function countBySeverity(list: Diagnostic[]): Record<Severity, number> {
  const out: Record<Severity, number> = { error: 0, warning: 0, info: 0 };
  for (const d of list) out[d.severity]++;
  return out;
}
