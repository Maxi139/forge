//! Gemeinsame Typen aller Forge-Crates. Sie werden als JSON (camelCase) an die Oberfläche gereicht.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSource {
    Syntax,
    Compiler,
    Clangd,
    Analysis,
    Ai,
}

/// Ersetzt den Text zwischen zwei Positionen (Zeilen 1-basiert, Spalten 0-basiert in UTF-16-Einheiten).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub from_line: u32,
    pub from_col: u32,
    pub to_line: u32,
    pub to_col: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuickFix {
    pub label: String,
    pub edits: Vec<TextEdit>,
}

/// Eine Aktion, die kein Texteingriff ist, etwa „Bibliothek suchen“.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticAction {
    pub kind: String,
    pub arg: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub id: String,
    pub file: String,
    /// 1-basiert.
    pub line: u32,
    /// Spalten, 0-basiert in UTF-16-Einheiten, `to` ist exklusiv. So zählt auch der Editor.
    pub from: u32,
    pub to: u32,
    pub severity: Severity,
    pub message: String,
    pub source: DiagnosticSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<QuickFix>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub action: Option<DiagnosticAction>,
}

/// Entfernt ANSI-Escape-Sequenzen (Farben). Werkzeuge färben ihre Ausgabe auch dann, wenn sie in eine Pipe schreibt.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
        } else if c != '\0' {
            out.push(c);
        }
    }
    out
}

/// Rechnet eine Byte-Spalte innerhalb einer Zeile in UTF-16-Einheiten um.
pub fn utf16_col(line: &str, byte_col: usize) -> u32 {
    let end = byte_col.min(line.len());
    let end = (0..=end)
        .rev()
        .find(|&i| line.is_char_boundary(i))
        .unwrap_or(0);
    line[..end].encode_utf16().count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_colors_are_removed() {
        assert_eq!(
            strip_ansi("\u{1b}[92mUsed library\u{1b}[0m  \u{1b}[90m/x\u{1b}[0m"),
            "Used library  /x"
        );
        assert_eq!(strip_ansi("plain"), "plain");
    }

    #[test]
    fn utf16_columns_follow_the_editor() {
        assert_eq!(utf16_col("äb", 2), 1);
        assert_eq!(utf16_col("a😀b", 5), 3);
        assert_eq!(utf16_col("abc", 99), 3);
    }

    #[test]
    fn diagnostic_serializes_camel_case() {
        let d = Diagnostic {
            id: "1".into(),
            file: "a.ino".into(),
            line: 2,
            from: 1,
            to: 2,
            severity: Severity::Error,
            message: "x".into(),
            source: DiagnosticSource::Syntax,
            fix: None,
            action: None,
        };
        let j = serde_json::to_string(&d).unwrap();
        assert!(j.contains("\"severity\":\"error\"") && j.contains("\"source\":\"syntax\""));
        assert!(!j.contains("fix"));
    }
}
