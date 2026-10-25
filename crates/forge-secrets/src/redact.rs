//! Schwärzt Secrets, bevor Text das Gerät verlässt (KI-Anfragen, MCP-Antworten).

use crate::scan::literals;
use regex::Regex;
use std::sync::OnceLock;

/// Bekannte Formen von Zugangsdaten, unabhängig davon, ob sie im Schlüsselbund liegen.
fn token_patterns() -> &'static [Regex] {
    static R: OnceLock<Vec<Regex>> = OnceLock::new();
    R.get_or_init(|| {
        [
            r"sk-[A-Za-z0-9_-]{20,}",
            r"gh[pousr]_[A-Za-z0-9]{30,}",
            r"xox[baprs]-[A-Za-z0-9-]{10,}",
            r"AIza[0-9A-Za-z_-]{30,}",
            r"AKIA[0-9A-Z]{16}",
            r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        ]
        .iter()
        .map(|p| Regex::new(p).expect("gültiges Muster"))
        .collect()
    })
}

#[derive(Debug, Clone, Default)]
pub struct Redactor {
    /// (Name, Wert), längste Werte zuerst.
    items: Vec<(String, String)>,
}

impl Redactor {
    pub fn new(secrets: impl IntoIterator<Item = (String, String)>) -> Self {
        let mut items: Vec<(String, String)> = secrets
            .into_iter()
            .filter(|(_, v)| v.chars().count() >= 3)
            .collect();
        items.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        Redactor { items }
    }

    /// Ersetzt gespeicherte Werte, bekannte Token-Formen und im Code fest eingetragene Zugangsdaten.
    pub fn apply(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (name, value) in &self.items {
            if out.contains(value.as_str()) {
                out = out.replace(value.as_str(), &format!("‹Secret {name}›"));
            }
        }
        // Fest eingetragene Werte, auch wenn sie noch nicht im Schlüsselbund liegen.
        let mut lits = literals(&out);
        lits.sort_by_key(|l| std::cmp::Reverse((l.row, l.start)));
        let lines: Vec<String> = out.split('\n').map(str::to_string).collect();
        let mut lines = lines;
        for l in lits {
            if let Some(line) = lines.get_mut(l.row) {
                if line.is_char_boundary(l.start) && line.is_char_boundary(l.end) {
                    line.replace_range(l.start..l.end, &format!("‹Secret {}›", l.suggested));
                }
            }
        }
        out = lines.join("\n");
        for re in token_patterns() {
            out = re.replace_all(&out, "‹Secret›").into_owned();
        }
        out
    }

    /// Namen der Secrets, deren Wert in `text` vorkommt. Für Prüfungen und Tests.
    pub fn leaks(&self, text: &str) -> Vec<&str> {
        self.items
            .iter()
            .filter(|(_, v)| text.contains(v.as_str()))
            .map(|(n, _)| n.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r() -> Redactor {
        Redactor::new([
            ("WIFI_PASS".to_string(), "correct horse".to_string()),
            ("WIFI_SSID".to_string(), "Heimnetz".to_string()),
            ("X".to_string(), "ab".to_string()),
        ])
    }

    #[test]
    fn known_values_disappear_everywhere() {
        let t = "WiFi.begin(\"Heimnetz\", \"correct horse\"); // Netz Heimnetz, pw correct horse\nSerial.println(\"ab\");";
        let out = r().apply(t);
        assert!(r().leaks(&out).is_empty(), "{out}");
        assert!(out.contains("‹Secret WIFI_PASS›") && out.contains("‹Secret WIFI_SSID›"));
        // Zu kurze Werte werden nicht ersetzt, sonst wäre jeder Text kaputt.
        assert!(out.contains("\"ab\""));
    }

    #[test]
    fn hardcoded_literals_are_masked_even_when_unknown() {
        let out = Redactor::default().apply(
            "const char* password = \"geheim123\";\nWiFi.begin(\"Buero\", \"hunter2hunter2\");",
        );
        assert!(
            !out.contains("geheim123") && !out.contains("hunter2hunter2") && !out.contains("Buero"),
            "{out}"
        );
        assert!(out.contains("const char* password = \"‹Secret PASSWORD›\""));
    }

    #[test]
    fn token_shapes_are_masked() {
        let t = "key=sk-0123456789abcdefghijklmnop ghp_0123456789abcdefghijklmnopqrstuvwxyz AKIAABCDEFGHIJKLMNOP eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.abcdefghijklmnop";
        let out = Redactor::default().apply(t);
        for leaked in ["sk-0123", "ghp_0123", "AKIAABC", "eyJhbGci"] {
            assert!(!out.contains(leaked), "{leaked} in {out}");
        }
    }

    #[test]
    fn code_without_secrets_is_untouched() {
        let code = "void setup() { Serial.begin(115200); pinMode(4, OUTPUT); }\n// Passwort wird aus dem Schlüsselbund gelesen\nString p = Secret(\"WIFI_PASS\");\n";
        assert_eq!(Redactor::default().apply(code), code);
    }
}
