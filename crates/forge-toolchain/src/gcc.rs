use crate::translate::translate;
use forge_core::{
    utf16_col, Diagnostic, DiagnosticAction, DiagnosticSource, QuickFix, Severity, TextEdit,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Pos {
    file: String,
    line: u32,
    #[serde(rename = "byte-column", default)]
    byte_column: u32,
    #[serde(default)]
    column: u32,
}

impl Pos {
    fn col(&self) -> u32 {
        if self.byte_column > 0 {
            self.byte_column
        } else {
            self.column.max(1)
        }
    }
}

#[derive(Debug, Deserialize)]
struct Loc {
    caret: Pos,
    #[serde(default)]
    finish: Option<Pos>,
}

#[derive(Debug, Deserialize)]
struct Fixit {
    start: Pos,
    next: Pos,
    string: String,
}

#[derive(Debug, Deserialize)]
struct Raw {
    kind: String,
    message: String,
    #[serde(default)]
    locations: Vec<Loc>,
    #[serde(default)]
    fixits: Vec<Fixit>,
}

/// Liest die JSON-Ausgabe von `-fdiagnostics-format=json`. GCC schreibt sie als ein Array auf stderr.
fn parse_raw(stderr: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    for value in serde_json::Deserializer::from_str(stderr)
        .into_iter::<Vec<Raw>>()
        .flatten()
    {
        out.extend(value);
    }
    if out.is_empty() {
        // Manche Läufe mischen Text und JSON. Dann das Array ab der ersten Klammer versuchen.
        if let Some(start) = stderr.find("[\n {").or_else(|| stderr.find("[{")) {
            if let Ok(v) = serde_json::from_str::<Vec<Raw>>(&stderr[start..]) {
                out = v;
            }
        }
    }
    out
}

fn inside(root: &Path, file: &str) -> bool {
    let p = Path::new(file);
    p.starts_with(root)
        || p.canonicalize()
            .ok()
            .zip(root.canonicalize().ok())
            .is_some_and(|(a, b)| a.starts_with(b))
}

fn ident_end(line: &str, byte_col0: usize) -> usize {
    let b = line.as_bytes();
    let mut i = byte_col0.min(b.len());
    if i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
            i += 1;
        }
    } else if i < b.len() {
        i += line[i..].chars().next().map_or(1, char::len_utf8);
    }
    i
}

/// Wandelt GCC-Diagnosen in Forge-Diagnosen um. Nur Meldungen aus Dateien unter `root` bleiben übrig.
/// `read` liefert den Text einer Datei (für die Umrechnung der Byte-Spalten in Editor-Spalten).
pub fn to_diagnostics(
    stderr: &str,
    root: &Path,
    read: &mut dyn FnMut(&str) -> Option<String>,
) -> Vec<Diagnostic> {
    let mut cache: HashMap<String, Vec<String>> = HashMap::new();
    let mut lines_of = |file: &str, read: &mut dyn FnMut(&str) -> Option<String>| -> Vec<String> {
        cache
            .entry(file.to_string())
            .or_insert_with(|| {
                read(file)
                    .map(|t| t.split('\n').map(str::to_string).collect())
                    .unwrap_or_default()
            })
            .clone()
    };

    let mut out: Vec<Diagnostic> = Vec::new();
    for d in parse_raw(stderr) {
        let severity = match d.kind.as_str() {
            "error" | "fatal error" => Severity::Error,
            "warning" => Severity::Warning,
            _ => continue,
        };
        let Some(loc) = d.locations.first() else {
            continue;
        };
        let caret = &loc.caret;
        if !inside(root, &caret.file) {
            continue;
        }
        let lines = lines_of(&caret.file, read);
        let text = lines
            .get(caret.line as usize - 1)
            .map(String::as_str)
            .unwrap_or("");
        let from_b = caret.col() as usize - 1;
        let to_b = match &loc.finish {
            Some(f) if f.file == caret.file && f.line == caret.line => f.col() as usize,
            _ => ident_end(text, from_b),
        };
        let from = utf16_col(text, from_b);
        let to = utf16_col(text, to_b.max(from_b + 1)).max(from + 1);

        let fix = if d.fixits.is_empty() {
            None
        } else {
            let edits: Vec<TextEdit> = d
                .fixits
                .iter()
                .filter(|f| inside(root, &f.start.file))
                .map(|f| {
                    let sl = lines_of(&f.start.file, read);
                    let nl = lines_of(&f.next.file, read);
                    let st = sl
                        .get(f.start.line as usize - 1)
                        .map(String::as_str)
                        .unwrap_or("");
                    let nt = nl
                        .get(f.next.line as usize - 1)
                        .map(String::as_str)
                        .unwrap_or("");
                    TextEdit {
                        from_line: f.start.line,
                        from_col: utf16_col(st, f.start.col() as usize - 1),
                        to_line: f.next.line,
                        to_col: utf16_col(nt, f.next.col() as usize - 1),
                        text: f.string.clone(),
                    }
                })
                .collect();
            let label = match d.fixits.as_slice() {
                [one] if one.start.line == one.next.line && one.start.col() == one.next.col() => {
                    format!("„{}“ einfügen", one.string.trim())
                }
                [one] if one.string.is_empty() => "Entfernen".to_string(),
                [one] => format!("Ersetzen durch „{}“", one.string),
                _ => "Vorschlag übernehmen".to_string(),
            };
            Some(QuickFix { label, edits })
        };

        let mut message = translate(&d.message);
        // Fehlt ein Header in spitzen Klammern, liegt meist eine Bibliothek nicht vor.
        let mut action = None;
        if let Some(header) = d.message.strip_suffix(": No such file or directory") {
            if text.contains(&format!("<{header}>")) {
                let stem = header.rsplit('/').next().unwrap_or(header);
                let stem = stem
                    .trim_end_matches(".h")
                    .trim_end_matches(".hpp")
                    .to_string();
                message.push_str(". Installiere die passende Bibliothek.");
                action = Some(DiagnosticAction {
                    kind: "searchLibrary".into(),
                    arg: stem,
                    label: "Bibliothek suchen".into(),
                });
            }
        }
        out.push(Diagnostic {
            id: format!("compiler:{}:{}:{}", caret.file, caret.line, from),
            file: caret.file.clone(),
            line: caret.line,
            from,
            to,
            severity,
            message,
            source: DiagnosticSource::Compiler,
            fix,
            action,
        });
    }
    out.dedup_by(|a, b| a.id == b.id && a.message == b.message);
    out
}

pub fn project_file(root: &Path, file: &str) -> Option<PathBuf> {
    inside(root, file).then(|| PathBuf::from(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"[{"kind":"error","message":"expected ';' before 'digitalWrite'","children":[],"column-origin":1,
      "locations":[{"caret":{"file":"/p/blink.ino","line":2,"display-column":58,"byte-column":58,"column":58}},
                   {"caret":{"file":"/p/blink.ino","line":2,"byte-column":59,"column":59},"finish":{"file":"/p/blink.ino","line":2,"byte-column":70,"column":70}}],
      "fixits":[{"start":{"file":"/p/blink.ino","line":2,"byte-column":58,"column":58},"next":{"file":"/p/blink.ino","line":2,"byte-column":58,"column":58},"string":";"}]},
     {"kind":"warning","message":"unused variable 'x'","option":"-Wunused-variable","locations":[{"caret":{"file":"/p/blink.ino","line":1,"byte-column":10,"column":10}}]},
     {"kind":"error","message":"boom","locations":[{"caret":{"file":"/opt/core/Arduino.h","line":5,"byte-column":1,"column":1}}]},
     {"kind":"note","message":"declared here","locations":[{"caret":{"file":"/p/blink.ino","line":1,"byte-column":1,"column":1}}]}]"#;

    fn run() -> Vec<Diagnostic> {
        let src = "void f() { int x = 0; }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(300); digitalWrite(LED_BUILTIN, LOW); }\n";
        to_diagnostics(JSON, Path::new("/p"), &mut |_| Some(src.to_string()))
    }

    #[test]
    fn keeps_project_errors_and_warnings_only() {
        let d = run();
        assert_eq!(d.len(), 2, "{d:#?}");
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[1].severity, Severity::Warning);
        assert_eq!(d[1].message, "Variable „x“ wird nicht verwendet");
    }

    #[test]
    fn missing_semicolon_gets_a_precise_fix() {
        let d = &run()[0];
        assert_eq!(d.message, "Semikolon erwartet vor „digitalWrite“");
        assert_eq!((d.line, d.from), (2, 57));
        let fix = d.fix.as_ref().unwrap();
        assert_eq!(fix.label, "„;“ einfügen");
        assert_eq!(
            (
                fix.edits[0].from_line,
                fix.edits[0].from_col,
                fix.edits[0].text.as_str()
            ),
            (2, 57, ";")
        );
    }

    #[test]
    fn missing_library_header_offers_a_search() {
        let json = r#"[{"kind":"fatal error","message":"Adafruit_NeoPixel.h: No such file or directory","locations":[{"caret":{"file":"/p/a.ino","line":1,"byte-column":10,"column":10}}]}]"#;
        let d = to_diagnostics(json, Path::new("/p"), &mut |_| {
            Some("#include <Adafruit_NeoPixel.h>\n".to_string())
        });
        assert_eq!(
            d[0].message,
            "Datei „Adafruit_NeoPixel.h“ nicht gefunden. Installiere die passende Bibliothek."
        );
        assert_eq!(d[0].action.as_ref().unwrap().arg, "Adafruit_NeoPixel");
        // Eigene Header (in Anführungszeichen) sind keine Bibliothek.
        let own = to_diagnostics(
            &json.replace("Adafruit_NeoPixel.h", "config.h"),
            Path::new("/p"),
            &mut |_| Some("#include \"config.h\"\n".to_string()),
        );
        assert!(own[0].action.is_none());
    }

    #[test]
    fn garbage_input_yields_nothing() {
        assert!(to_diagnostics("keine json ausgabe", Path::new("/p"), &mut |_| None).is_empty());
    }
}
