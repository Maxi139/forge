//! Findet Secrets im Quelltext: benutzte Namen (`Secret("WIFI_PASS")`) und fest eingetragene Werte.

use forge_core::{utf16_col, Diagnostic, DiagnosticAction, DiagnosticSource, Severity};
use regex::Regex;
use std::collections::BTreeSet;
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("gültiges Muster"))
}

/// Namen, die der Code über `Secret("...")` oder `HasSecret("...")` liest.
pub fn used_names(text: &str) -> BTreeSet<String> {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r#"\b(?:Has)?Secret\(\s*"([A-Za-z0-9_.-]{1,15})"\s*\)"#)
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect()
}

/// Ein fest eingetragener Wert im Quelltext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Literal {
    /// Zeile (0-basiert) und Byte-Spalten des Werts ohne Anführungszeichen.
    pub row: usize,
    pub start: usize,
    pub end: usize,
    pub value: String,
    /// Vorschlag für den Namen im Schlüsselbund.
    pub suggested: String,
    /// `true` für Passwörter, Tokens und Schlüssel, `false` für den WLAN-Namen.
    pub secret: bool,
    /// Der Wert steht direkt in einem Funktionsaufruf, nicht in einer Deklaration.
    pub in_call: bool,
}

fn suggest(var: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for ch in var.chars() {
        if ch.is_ascii_uppercase() && prev_lower {
            out.push('_');
        }
        prev_lower = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        out.push(ch.to_ascii_uppercase());
    }
    let out: String = out
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    out.chars().take(15).collect()
}

const PLACEHOLDERS: &[&str] = &[
    "password",
    "passwort",
    "yourpassword",
    "your_password",
    "changeme",
    "xxxx",
    "secret",
    "token",
    "apikey",
];

fn plausible(value: &str) -> bool {
    value.chars().count() >= 6
        && !PLACEHOLDERS.contains(&value.to_lowercase().as_str())
        && !value.starts_with("http")
        && !value.contains("%s")
        && !value.contains("%d")
}

/// Sucht Passwörter, Tokens und Schlüssel, die als Text im Code stehen.
pub fn literals(text: &str) -> Vec<Literal> {
    static DECL: OnceLock<Regex> = OnceLock::new();
    static BEGIN: OnceLock<Regex> = OnceLock::new();
    static BEARER: OnceLock<Regex> = OnceLock::new();
    let decl = re(
        &DECL,
        r#"(?i)\b(\w*?(?:pass(?:word|wd)?|pwd|secret|token|api_?key|auth(?:key)?))\s*(?:\[\s*\]\s*)?=\s*"((?:[^"\\]|\\.)*)""#,
    );
    let begin = re(
        &BEGIN,
        r#"WiFi\.(?:begin|softAP)\s*\(\s*"((?:[^"\\]|\\.)*)"\s*(?:,\s*"((?:[^"\\]|\\.)*)")?"#,
    );
    let bearer = re(&BEARER, r#"(?i)"(Bearer\s+[A-Za-z0-9._~+/=-]{12,})""#);

    let mut out = Vec::new();
    for (row, line) in text.split('\n').enumerate() {
        let code = line.split("//").next().unwrap_or(line);
        for c in decl.captures_iter(code) {
            let v = c.get(2).unwrap();
            if plausible(v.as_str()) {
                out.push(Literal {
                    row,
                    start: v.start(),
                    end: v.end(),
                    value: v.as_str().to_string(),
                    suggested: suggest(&c[1]),
                    secret: true,
                    in_call: false,
                });
            }
        }
        for c in begin.captures_iter(code) {
            if let Some(ssid) = c.get(1).filter(|m| !m.as_str().is_empty()) {
                out.push(Literal {
                    row,
                    start: ssid.start(),
                    end: ssid.end(),
                    value: ssid.as_str().to_string(),
                    suggested: "WIFI_SSID".into(),
                    secret: false,
                    in_call: true,
                });
            }
            if let Some(pw) = c.get(2).filter(|m| !m.as_str().is_empty()) {
                out.push(Literal {
                    row,
                    start: pw.start(),
                    end: pw.end(),
                    value: pw.as_str().to_string(),
                    suggested: "WIFI_PASS".into(),
                    secret: true,
                    in_call: true,
                });
            }
        }
        for c in bearer.captures_iter(code) {
            let v = c.get(1).unwrap();
            out.push(Literal {
                row,
                start: v.start(),
                end: v.end(),
                value: v.as_str().to_string(),
                suggested: "API_TOKEN".into(),
                secret: true,
                in_call: true,
            });
        }
    }
    // Doppelte Treffer derselben Stelle (Deklaration und Aufruf) zusammenfassen.
    out.sort_by_key(|l| (l.row, l.start));
    out.dedup_by_key(|l| (l.row, l.start));
    out
}

/// Hinweise für den Editor. Der Wert selbst steht nie in der Meldung.
pub fn diagnostics(file: &str, text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.split('\n').collect();
    literals(text)
        .into_iter()
        .filter(|l| l.secret)
        .map(|l| {
            let line = lines.get(l.row).copied().unwrap_or("");
            let from = utf16_col(line, l.start.saturating_sub(1));
            let to = utf16_col(line, l.end + 1);
            Diagnostic {
                id: format!("analysis:secret:{file}:{}:{from}", l.row + 1),
                file: file.into(),
                line: l.row as u32 + 1,
                from,
                to,
                severity: Severity::Warning,
                message: "Dieser Wert gehört in den Schlüsselbund, nicht in den Code.".into(),
                source: DiagnosticSource::Analysis,
                fix: None,
                action: Some(DiagnosticAction {
                    kind: "moveToKeychain".into(),
                    arg: format!(
                        "{}|{}",
                        l.suggested,
                        if l.in_call { "call" } else { "decl" }
                    ),
                    label: "In Schlüsselbund legen".into(),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_used_names() {
        let t = r#"WiFi.begin(Secret("WIFI_SSID").c_str(), Secret( "WIFI_PASS" ).c_str()); if (HasSecret("API_TOKEN")) {} Secret("viel_zu_langer_name_hier")"#;
        assert_eq!(
            used_names(t).into_iter().collect::<Vec<_>>(),
            ["API_TOKEN", "WIFI_PASS", "WIFI_SSID"]
        );
    }

    #[test]
    fn finds_hardcoded_credentials_but_not_noise() {
        let t = "const char* ssid = \"Heimnetz\";\nconst char* password = \"geheim123\";\nString apiKey = \"abcdef123456\";\nconst char* hostName = \"esp32\";\nconst char* pwd = \"changeme\"; // Platzhalter\nWiFi.begin(\"Buero\", \"hunter2hunter2\");\n";
        let l = literals(t);
        let found: Vec<(usize, &str, &str, bool)> = l
            .iter()
            .map(|x| (x.row, x.value.as_str(), x.suggested.as_str(), x.secret))
            .collect();
        assert_eq!(
            found,
            [
                (1, "geheim123", "PASSWORD", true),
                (2, "abcdef123456", "API_KEY", true),
                (5, "Buero", "WIFI_SSID", false),
                (5, "hunter2hunter2", "WIFI_PASS", true)
            ]
        );
    }

    #[test]
    fn diagnostics_never_contain_the_value() {
        let d = diagnostics("a.ino", "const char* password = \"geheim123\";\n");
        assert_eq!(d.len(), 1);
        assert!(!serde_json_like(&d[0]).contains("geheim123"));
        assert_eq!(d[0].action.as_ref().unwrap().arg, "PASSWORD|decl");
        assert_eq!((d[0].line, d[0].from), (1, 23));
    }

    fn serde_json_like(d: &Diagnostic) -> String {
        format!("{d:?}")
    }
}
