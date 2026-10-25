//! Aufgaben an die KI: Der Nutzer beschreibt, was der Sketch können soll, die KI liefert die Änderungen.
//! Sie arbeitet in einem Zug. Was sie liefert, sind Ersetzungen mit genauem Text und neue Dateien, nie ganze Dateien.

use crate::prompt::position;
use forge_core::TextEdit;
use forge_secrets::Redactor;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

/// Mehr Quelltext als das geht nicht an die KI. Größere Projekte brauchen eine engere Aufgabe.
pub const MAX_CONTEXT_CHARS: usize = 60_000;

/// Dateiendungen, die die KI neu anlegen darf.
const NEW_FILE_EXTENSIONS: &[&str] = &["ino", "cpp", "c", "cc", "h", "hpp"];

pub const TASK_SYSTEM: &str = r#"Du bist ein Programmierassistent in einer IDE für ESP32-Sketches (Arduino-Framework, C++).
Der Nutzer gibt dir eine Aufgabe. Du siehst alle Quelldateien des Projekts. Setze die Aufgabe um, indem du den Code änderst.

Regeln:
- Ändere nur, was die Aufgabe verlangt. Halte Stil, Einrückung und Namen des vorhandenen Codes ein.
- Schreibe kompilierbaren Code für das genannte Board. Benutze nur Bibliotheken, die im Projekt schon vorkommen oder zum Arduino-ESP32-Kern gehören.
- Zugangsdaten gehören nie in den Code. Platzhalter wie ‹Secret NAME› schreibst du nie in neuen Code und änderst sie nicht.
- Ist die Aufgabe unklar oder ohne Rückfrage nicht lösbar, ändere nichts und erkläre in "note", was du wissen musst.

Antworte ausschließlich mit einem JSON-Objekt:
{"note":"<zwei Sätze auf Deutsch: was du gemacht hast>","edits":[{"file":"<Pfad wie im Kontext>","old":"<exakter Text aus der Datei>","new":"<Ersatz>"}],"create":[{"file":"<neuer Pfad, relativ zum Projekt>","text":"<kompletter Inhalt>"}]}
Jeder "old"-Text muss in seiner Datei genau einmal vorkommen. Er darf mehrere Zeilen umfassen und muss lang genug sein, um eindeutig zu sein. Willst du nur etwas einfügen, nimm eine Zeile in der Nähe als "old" und schreibe sie samt Ergänzung in "new". Mehrere Änderungen in einer Datei dürfen sich nicht überschneiden.
"edits" und "create" dürfen leer sein."#;

/// Eine Quelldatei im Kontext. `shown` ist der Pfad, den die KI sieht (relativ zum Projekt).
pub struct SourceText {
    pub shown: String,
    pub text: String,
}

/// Der Text, der an die KI geht. Secrets sind vorher geschwärzt.
pub fn task_prompt(
    board: &str,
    files: &[SourceText],
    active: Option<&str>,
    problems: &str,
    task: &str,
    redactor: &Redactor,
) -> String {
    let mut out = format!("Board: {board}\n");
    if let Some(a) = active {
        out.push_str(&format!("Im Editor ist gerade geöffnet: {a}\n"));
    }
    out.push_str("\nDateien des Projekts:\n");
    for f in files {
        out.push_str(&format!("\n=== {} ===\n{}\n", f.shown, f.text.trim_end()));
    }
    if !problems.trim().is_empty() {
        out.push_str(&format!("\nAktuelle Meldungen:\n{}\n", problems.trim()));
    }
    out.push_str(&format!("\nAufgabe:\n{}", task.trim()));
    redactor.apply(&out)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileFix {
    pub file: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NewFile {
    pub file: String,
    pub text: String,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    note: String,
    #[serde(default)]
    edits: Vec<FileFix>,
    #[serde(default)]
    create: Vec<NewFile>,
}

/// Liest die Antwort. Modelle packen JSON gern in Code-Zäune oder Text.
pub fn parse_task(content: &str) -> Result<(Vec<FileFix>, Vec<NewFile>, String), String> {
    let start = content.find('{').ok_or("kein JSON in der Antwort")?;
    let end = content.rfind('}').ok_or("kein JSON in der Antwort")?;
    let raw: Raw = serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())?;
    Ok((raw.edits, raw.create, raw.note.trim().to_string()))
}

/// Die Änderungen an einer Datei.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    /// Pfad, wie ihn der Aufrufer für die Datei vergeben hat.
    pub file: String,
    pub edits: Vec<TextEdit>,
}

/// Eine neue Datei mit vollem Pfad.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedFile {
    pub file: String,
    pub text: String,
}

/// Was die KI für eine Aufgabe vorschlägt.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskOutcome {
    pub changes: Vec<FileChange>,
    pub created: Vec<CreatedFile>,
    pub note: String,
    pub tokens: u64,
}

const PLACEHOLDER: &str = "‹Secret";

/// Setzt die Ersetzungen auf die Dateien. `files` sind Paare aus gezeigtem Pfad und vollem Pfad samt Text.
/// Schlägt eine einzige Ersetzung fehl, bleibt alles unberührt.
pub fn plan_changes(
    files: &[(String, String, String)],
    fixes: &[FileFix],
) -> Result<Vec<FileChange>, String> {
    let mut out: Vec<FileChange> = Vec::new();
    for (shown, full, text) in files {
        let mine: Vec<&FileFix> = fixes
            .iter()
            .filter(|f| &f.file == shown && !f.old.is_empty() && f.old != f.new)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let mut spans: Vec<(usize, usize, &str)> = Vec::new();
        for f in mine {
            if f.new.contains(PLACEHOLDER) {
                return Err("Die KI wollte einen geschützten Wert in den Code schreiben. Das passiert nicht.".into());
            }
            let mut hits = text.match_indices(f.old.as_str());
            let (Some((at, _)), None) = (hits.next(), hits.next()) else {
                let first = f.old.lines().next().unwrap_or("").trim();
                return Err(format!(
                    "Die KI hat die Stelle „{first}“ in {shown} nicht eindeutig getroffen. Versuch es noch einmal."
                ));
            };
            spans.push((at, at + f.old.len(), f.new.as_str()));
        }
        spans.sort_by_key(|s| s.0);
        if spans.windows(2).any(|w| w[0].1 > w[1].0) {
            return Err(format!(
                "Die Änderungen der KI in {shown} überschneiden sich."
            ));
        }
        let edits = spans
            .into_iter()
            .map(|(from, to, new)| {
                let (from_line, from_col) = position(text, from);
                let (to_line, to_col) = position(text, to);
                TextEdit {
                    from_line,
                    from_col,
                    to_line,
                    to_col,
                    text: new.to_string(),
                }
            })
            .collect();
        out.push(FileChange {
            file: full.clone(),
            edits,
        });
    }
    // Änderungen an Dateien, die die KI nie gesehen hat, sind Erfindungen.
    if let Some(bad) = fixes
        .iter()
        .find(|f| !f.old.is_empty() && !files.iter().any(|(s, _, _)| s == &f.file))
    {
        return Err(format!(
            "Die KI wollte eine Datei ändern, die es nicht gibt: {}.",
            bad.file
        ));
    }
    Ok(out)
}

/// Prüft die neuen Dateien: relativer Pfad im Projekt, bekannte Endung, noch nicht vorhanden.
pub fn plan_created(
    root: &Path,
    exists: impl Fn(&Path) -> bool,
    created: &[NewFile],
) -> Result<Vec<CreatedFile>, String> {
    let mut out = Vec::new();
    for n in created {
        let rel = Path::new(&n.file);
        let clean = !n.file.trim().is_empty()
            && rel.components().all(|c| matches!(c, Component::Normal(_)));
        let ext_ok = rel
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| NEW_FILE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
            .unwrap_or(false);
        if !clean || !ext_ok {
            return Err(format!(
                "Die KI wollte die Datei „{}“ anlegen. Das ist hier nicht erlaubt.",
                n.file
            ));
        }
        if n.text.contains(PLACEHOLDER) {
            return Err(
                "Die KI wollte einen geschützten Wert in den Code schreiben. Das passiert nicht."
                    .into(),
            );
        }
        let full = root.join(rel);
        if exists(&full) {
            return Err(format!("Die Datei „{}“ gibt es schon.", n.file));
        }
        out.push(CreatedFile {
            file: full.to_string_lossy().to_string(),
            text: n.text.clone(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<(String, String, String)> {
        vec![
            (
                "a.ino".into(),
                "/p/a.ino".into(),
                "void setup() {\n  pinMode(2, OUTPUT);\n}\nvoid loop() {\n  delay(10);\n}\n".into(),
            ),
            ("b.h".into(), "/p/b.h".into(), "#pragma once\n".into()),
        ]
    }

    fn fix(file: &str, old: &str, new: &str) -> FileFix {
        FileFix {
            file: file.into(),
            old: old.into(),
            new: new.into(),
        }
    }

    #[test]
    fn replies_come_in_many_shapes() {
        let (e, c, note) = parse_task("```json\n{\"note\":\"Fertig.\",\"edits\":[{\"file\":\"a.ino\",\"old\":\"x\",\"new\":\"y\"}],\"create\":[{\"file\":\"c.h\",\"text\":\"//\"}]}\n```").unwrap();
        assert_eq!((e.len(), c.len(), note.as_str()), (1, 1, "Fertig."));
        let (e, c, _) = parse_task("{\"note\":\"Frage\"}").unwrap();
        assert!(e.is_empty() && c.is_empty());
        assert!(parse_task("nein").is_err());
    }

    #[test]
    fn edits_land_in_the_right_file_with_exact_positions() {
        let plan = plan_changes(
            &files(),
            &[
                fix("a.ino", "delay(10);", "delay(10);\n  Serial.println(1);"),
                fix("a.ino", "pinMode(2, OUTPUT);", "pinMode(4, OUTPUT);"),
            ],
        )
        .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].file, "/p/a.ino");
        // Nach Position sortiert, in Originalkoordinaten.
        assert_eq!(
            (
                plan[0].edits[0].from_line,
                plan[0].edits[0].from_col,
                plan[0].edits[0].to_col
            ),
            (2, 2, 21)
        );
        assert_eq!(plan[0].edits[1].from_line, 5);
    }

    #[test]
    fn anything_unclear_changes_nothing() {
        let f = files();
        assert!(plan_changes(&f, &[fix("a.ino", "gibt es nicht", "x")]).is_err());
        assert!(
            plan_changes(&f, &[fix("a.ino", "(", "[")]).is_err(),
            "nicht eindeutig"
        );
        assert!(
            plan_changes(&f, &[fix("zzz.ino", "x", "y")]).is_err(),
            "unbekannte Datei"
        );
        assert!(plan_changes(&f, &[fix("a.ino", "delay(10);", "‹Secret WIFI›")]).is_err());
        let overlap = [
            fix("a.ino", "delay(10);\n}", "x"),
            fix("a.ino", "(10);", "y"),
        ];
        assert!(plan_changes(&f, &overlap).is_err());
    }

    #[test]
    fn new_files_stay_inside_the_project() {
        let root = Path::new("/p");
        let ok = |p: &str| NewFile {
            file: p.into(),
            text: "// neu".into(),
        };
        let made = plan_created(root, |_| false, &[ok("led.h"), ok("src/led.cpp")]).unwrap();
        assert_eq!(made[0].file, "/p/led.h");
        assert!(plan_created(root, |_| false, &[ok("../x.h")]).is_err());
        assert!(plan_created(root, |_| false, &[ok("/etc/x.h")]).is_err());
        assert!(plan_created(root, |_| false, &[ok("forge.toml")]).is_err());
        assert!(plan_created(root, |_| true, &[ok("led.h")]).is_err());
    }

    #[test]
    fn the_prompt_never_contains_secrets() {
        let src = SourceText {
            shown: "a.ino".into(),
            text: "const char* token = \"mein-token-ABCDEF\";\nvoid setup() {}\n".into(),
        };
        let red = Redactor::new([("API".to_string(), "mein-token-ABCDEF".to_string())]);
        let p = task_prompt(
            "ESP32-S3",
            &[src],
            Some("a.ino"),
            "",
            "Blinke mit der LED",
            &red,
        );
        assert!(!p.contains("mein-token-ABCDEF"));
        assert!(p.contains("Blinke mit der LED") && p.contains("=== a.ino ==="));
    }
}
