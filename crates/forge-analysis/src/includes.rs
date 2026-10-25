use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Include {
    pub name: String,
    /// `<...>` statt `"..."`.
    pub system: bool,
    pub line: u32,
}

/// Alle `#include`-Zeilen eines Textes.
pub fn includes(text: &str) -> Vec<Include> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim_start();
        let Some(rest) = line.strip_prefix('#') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("include") else {
            continue;
        };
        let rest = rest.trim_start();
        let (system, close) = match rest.chars().next() {
            Some('<') => (true, '>'),
            Some('"') => (false, '"'),
            _ => continue,
        };
        if let Some(end) = rest[1..].find(close) {
            out.push(Include {
                name: rest[1..1 + end].to_string(),
                system,
                line: i as u32 + 1,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_both_kinds() {
        let inc =
            includes("#include <WiFi.h>\n  # include \"config.h\"\n// #include <x.h>\nint a;");
        assert_eq!(inc.len(), 2, "{inc:?}");
        assert_eq!(
            inc[0],
            Include {
                name: "WiFi.h".into(),
                system: true,
                line: 1
            }
        );
        assert_eq!(
            inc[1],
            Include {
                name: "config.h".into(),
                system: false,
                line: 2
            }
        );
    }
}
