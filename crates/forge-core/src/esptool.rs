//! esptool 4 und 5 schreiben Befehle und Werte unterschiedlich: v4 nutzt Unterstriche
//! (`write_flash`, `default_reset`), ab v5 gelten Bindestriche (`write-flash`, `default-reset`).
//! Die Version wird einmal ermittelt, danach liefert `EsptoolStyle` die passende Schreibweise.

use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EsptoolStyle {
    /// esptool bis 4.x: `write_flash`, `default_reset`.
    V4,
    /// esptool ab 5: `write-flash`, `default-reset`.
    #[default]
    V5,
}

impl EsptoolStyle {
    /// Liest die Hauptversion aus der Ausgabe von `esptool version` / `--version`
    /// („esptool.py v4.7.0“, „esptool v5.3.1“, „esptool.py 4.8.1“). Unbekanntes gilt als v5.
    pub fn from_version_output(text: &str) -> Self {
        for line in text.lines() {
            let lower = line.to_lowercase();
            let Some(pos) = lower.find("esptool") else {
                continue;
            };
            let rest = &lower[pos + "esptool".len()..];
            let digits: String = rest
                .trim_start_matches(|c: char| !c.is_ascii_digit())
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if let Ok(major) = digits.parse::<u32>() {
                return if major < 5 { Self::V4 } else { Self::V5 };
            }
        }
        Self::V5
    }

    /// Fragt das Programm nach seiner Version. Blockiert kurz, Aufrufer in async-Code nutzen `spawn_blocking`.
    pub fn detect(esptool: &Path) -> Self {
        for arg in ["version", "--version"] {
            let Ok(out) = Command::new(esptool).arg(arg).stdin(Stdio::null()).output() else {
                return Self::default();
            };
            let text = format!(
                "{}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if text.to_lowercase().contains("esptool") && text.chars().any(|c| c.is_ascii_digit()) {
                return Self::from_version_output(&text);
            }
        }
        Self::default()
    }

    /// Wandelt einen Befehls- oder Wertnamen in die Schreibweise der Version: `write-flash` / `write_flash`.
    pub fn word(self, hyphenated: &str) -> String {
        match self {
            Self::V5 => hyphenated.to_string(),
            Self::V4 => hyphenated.replace('-', "_"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_selects_style() {
        assert_eq!(
            EsptoolStyle::from_version_output("esptool.py v4.7.0"),
            EsptoolStyle::V4
        );
        assert_eq!(
            EsptoolStyle::from_version_output("esptool.py 4.8.1\n"),
            EsptoolStyle::V4
        );
        assert_eq!(
            EsptoolStyle::from_version_output("esptool v5.3.1"),
            EsptoolStyle::V5
        );
        assert_eq!(EsptoolStyle::from_version_output("???"), EsptoolStyle::V5);
    }

    #[test]
    fn words_follow_style() {
        assert_eq!(EsptoolStyle::V4.word("write-flash"), "write_flash");
        assert_eq!(EsptoolStyle::V5.word("write-flash"), "write-flash");
        assert_eq!(EsptoolStyle::V4.word("hard-reset"), "hard_reset");
    }
}
