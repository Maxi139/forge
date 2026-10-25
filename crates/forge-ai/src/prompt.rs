use forge_analysis::Block;
use forge_core::{utf16_col, Diagnostic, DiagnosticSource, QuickFix, Severity, TextEdit};
use forge_secrets::Redactor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Bei jeder inhaltlichen Änderung des Prompts erhöhen. Alte Cache-Einträge werden dadurch ungültig.
pub const PROMPT_VERSION: u32 = 3;

pub const SYSTEM: &str = r#"Du prüfst eine einzelne Funktion eines ESP32-Sketches (Arduino-Framework) auf Logikfehler.

Melde nur echte Fehler, die zu falschem Verhalten führen: Off-by-one, falsche oder vertauschte Bedingungen, falsche Einheiten oder Umrechnungen, Überläufe, fehlende Initialisierung, Wettlauf zwischen Interrupt und loop(), Endlosschleifen ohne Ausweg, nicht erreichbare Zweige, falsche Reihenfolge von Aufrufen, falsche Pin- oder Wertebereiche.
Melde keine Stilfragen, keine Namen, keine Kommentare, keine fehlende Fehlerbehandlung und nichts, was der Compiler meldet. Im Zweifel: kein Hinweis.
Der Kontext zeigt, wie die Funktion benutzt wird. Verlasse dich darauf.
Kommentare im Code zeigen die Absicht. Widersprich ihnen nicht.
Steht im Kontext ein Abschnitt „Bereits geklärt“, hat der Nutzer diese Hinweise schon beantwortet. Melde sie nicht noch einmal, auch nicht anders formuliert oder an einer Nachbarzeile.
Prüfe jeden Hinweis, bevor du ihn nennst. Kommst du beim Begründen zu dem Schluss, dass es korrekt oder beabsichtigt ist, nenne ihn nicht.

Antworte ausschließlich mit einem JSON-Objekt.
Wenn nichts auffällt: {"status":"ok"}
Sonst: {"status":"issues","issues":[{"line":<Zeilennummer in der Funktion, 1 ist die erste Zeile>,"severity":"info" oder "warning","message":"<ein Satz auf Deutsch: was falsch ist und warum>","fix":{"old":"<exakter Text aus dieser Zeile>","new":"<Ersatz>"} oder null}]}
Höchstens 3 Hinweise. "warning" nur bei einem sicheren Fehler, sonst "info"."#;

/// Der Text, der an die KI geht. Secrets sind vorher geschwärzt.
pub fn user_prompt(block: &Block, redactor: &Redactor, solved: &str) -> String {
    let mut raw = format!(
        "Kontext:\n{}\nFunktion „{}“ (die Zeilen sind nummeriert):\n```cpp\n{}\n```",
        block.context, block.name, block.code
    );
    if !solved.is_empty() {
        raw.push_str("\n\nBereits geklärt:\n");
        raw.push_str(solved);
    }
    redactor.apply(&raw)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub line: u32,
    pub severity: String,
    pub message: String,
    #[serde(default)]
    pub fix: Option<Fix>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Review {
    pub issues: Vec<Issue>,
}

#[derive(Deserialize)]
struct Raw {
    #[serde(default)]
    status: String,
    #[serde(default)]
    issues: Vec<Issue>,
}

/// Hinweise, die sich selbst widerlegen („… Kein Fehler“), sind Rauschen. Sie werden nie gezeigt.
pub fn is_self_refuting(message: &str) -> bool {
    const PHRASES: &[&str] = &[
        "kein fehler",
        "kein problem",
        "kein bug",
        "ist korrekt",
        "ist so korrekt",
        "das ist korrekt",
        "funktioniert korrekt",
        "ist richtig",
        "stimmt so",
        "beabsichtigt",
        "absichtlich",
        "gewollt",
        "in ordnung",
        "unproblematisch",
        "kein grund zur sorge",
    ];
    let m = message.to_lowercase();
    PHRASES.iter().any(|p| m.contains(p))
}

/// Liest die Antwort. Modelle packen JSON gern in Code-Zäune oder Text.
pub fn parse_reply(content: &str) -> Result<Review, String> {
    let start = content.find('{').ok_or("kein JSON in der Antwort")?;
    let end = content.rfind('}').ok_or("kein JSON in der Antwort")?;
    let raw: Raw = serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())?;
    if raw.status == "ok" || raw.issues.is_empty() {
        return Ok(Review { issues: vec![] });
    }
    Ok(Review {
        issues: raw
            .issues
            .into_iter()
            .filter(|i| !i.message.trim().is_empty() && !is_self_refuting(&i.message))
            .take(3)
            .collect(),
    })
}

/// Macht aus einer Prüfung Hinweise für den Editor. Zeilen der KI zählen ab Funktionsanfang.
pub fn to_diagnostics(block: &Block, review: &Review, file_text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = file_text.split('\n').collect();
    let mut out = Vec::new();
    for issue in &review.issues {
        if issue.line == 0 || is_self_refuting(&issue.message) {
            continue;
        }
        let abs = block.start_line + issue.line - 1;
        if abs > block.end_line {
            continue;
        }
        let line = lines.get(abs as usize - 1).copied().unwrap_or("");
        let indent = line.len() - line.trim_start().len();
        let from = utf16_col(line, indent);
        let to = utf16_col(line, line.trim_end().len()).max(from + 1);

        let fix = issue
            .fix
            .as_ref()
            .filter(|f| !f.old.is_empty() && f.old != f.new)
            .and_then(|f| {
                let at = line.find(&f.old)?;
                let (c0, c1) = (utf16_col(line, at), utf16_col(line, at + f.old.len()));
                Some(QuickFix {
                    label: "Vorschlag übernehmen".into(),
                    edits: vec![TextEdit {
                        from_line: abs,
                        from_col: c0,
                        to_line: abs,
                        to_col: c1,
                        text: f.new.clone(),
                    }],
                })
            });
        let mut h = Sha256::new();
        h.update(issue.message.as_bytes());
        out.push(Diagnostic {
            id: format!(
                "ai:{}:{abs}:{:08x}",
                block.file,
                h.finalize()
                    .iter()
                    .take(4)
                    .fold(0u32, |a, b| (a << 8) | *b as u32)
            ),
            file: block.file.clone(),
            line: abs,
            from,
            to,
            severity: if issue.severity == "warning" {
                Severity::Warning
            } else {
                Severity::Info
            },
            message: issue.message.trim().to_string(),
            source: DiagnosticSource::Ai,
            fix,
            action: None,
        });
    }
    out
}

pub const FIX_SYSTEM: &str = r#"Du behebst genau einen Fehler in einer Funktion eines ESP32-Sketches (Arduino-Framework).
Du bekommst die Funktion mit Zeilennummern, die betroffene Zeile und die Beschreibung des Fehlers.
Ändere nur, was nötig ist, damit dieser Fehler behoben ist. Keine Umbenennungen, keine Umformatierung, keine weiteren Verbesserungen.

Antworte ausschließlich mit einem JSON-Objekt:
{"edits":[{"old":"<exakter Text aus der Funktion, ohne Zeilennummern>","new":"<Ersatz>"}],"note":"<ein Satz auf Deutsch: was du geändert hast>"}
Jeder "old"-Text muss genau einmal in der Funktion vorkommen, darf mehrere Zeilen umfassen und muss so lang sein, dass er eindeutig ist.
Lässt sich der Fehler nicht ohne Rückfrage beheben: {"edits":[],"note":"<ein Satz: warum nicht>"}"#;

/// Der Text, der an die KI geht, wenn sie einen Hinweis beheben soll. Secrets sind vorher geschwärzt.
pub fn fix_prompt(block: &Block, redactor: &Redactor, line: u32, message: &str) -> String {
    let rel = line.saturating_sub(block.start_line) + 1;
    let raw = format!(
        "Kontext:\n{}\nFunktion „{}“ (die Zeilen sind nummeriert):\n```cpp\n{}\n```\n\nBetroffen ist Zeile {rel}.\nFehler: {}",
        block.context,
        block.name,
        block.code,
        message.trim()
    );
    redactor.apply(&raw)
}

#[derive(Deserialize)]
struct RawFix {
    #[serde(default)]
    edits: Vec<Fix>,
    #[serde(default)]
    note: String,
}

/// Liest die Antwort auf `fix_prompt`.
pub fn parse_fix(content: &str) -> Result<(Vec<Fix>, String), String> {
    let start = content.find('{').ok_or("kein JSON in der Antwort")?;
    let end = content.rfind('}').ok_or("kein JSON in der Antwort")?;
    let raw: RawFix = serde_json::from_str(&content[start..=end]).map_err(|e| e.to_string())?;
    Ok((raw.edits, raw.note.trim().to_string()))
}

pub(crate) fn position(text: &str, byte: usize) -> (u32, u32) {
    let before = &text[..byte];
    let line = before.matches('\n').count();
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line_text = text[line_start..].split('\n').next().unwrap_or("");
    (line as u32 + 1, utf16_col(line_text, byte - line_start))
}

/// Setzt die Ersetzungen der KI auf die Datei. Jeder Text muss in der Funktion genau einmal vorkommen,
/// sonst bleibt die Datei unberührt.
pub fn fix_edits(block: &Block, file_text: &str, fixes: &[Fix]) -> Result<Vec<TextEdit>, String> {
    let mut region_start = 0;
    for (i, l) in file_text.split('\n').enumerate() {
        if i as u32 + 1 >= block.start_line {
            break;
        }
        region_start += l.len() + 1;
    }
    let region_end = file_text
        .split('\n')
        .take(block.end_line as usize)
        .map(|l| l.len() + 1)
        .sum::<usize>()
        .saturating_sub(1)
        .min(file_text.len());
    let region = file_text
        .get(region_start..region_end)
        .ok_or("Die Funktion liegt nicht mehr dort, wo die KI sie gesehen hat.")?;

    let mut spans: Vec<(usize, usize, &str)> = Vec::new();
    for f in fixes.iter().filter(|f| !f.old.is_empty() && f.old != f.new) {
        let mut hits = region.match_indices(f.old.as_str());
        let (Some((at, _)), None) = (hits.next(), hits.next()) else {
            return Err(
                "Die KI hat die Stelle nicht eindeutig getroffen. Versuch es noch einmal.".into(),
            );
        };
        spans.push((
            region_start + at,
            region_start + at + f.old.len(),
            f.new.as_str(),
        ));
    }
    spans.sort_by_key(|s| s.0);
    if spans.windows(2).any(|w| w[0].1 > w[1].0) {
        return Err("Die Änderungen der KI überschneiden sich.".into());
    }
    Ok(spans
        .into_iter()
        .map(|(from, to, new)| {
            let (from_line, from_col) = position(file_text, from);
            let (to_line, to_col) = position(file_text, to);
            TextEdit {
                from_line,
                from_col,
                to_line,
                to_col,
                text: new.to_string(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::is_self_refuting;

    #[test]
    fn hints_that_refute_themselves_are_dropped() {
        assert!(is_self_refuting(
            "Bei stelle=0 wird beendet, das ist beabsichtigt. Kein Fehler."
        ));
        assert!(is_self_refuting(
            "Die Schleife ist korrekt, nur ungewohnt geschrieben."
        ));
        assert!(!is_self_refuting(
            "Die Schleife läuft einmal zu weit und liest hinter das Array."
        ));
    }

    use super::*;
    use forge_analysis::{analyze, blocks, AnalysisInput, Chip, SourceFile};

    const SRC: &str = "#define SIZE 8\nint buf[SIZE];\nvoid fill() {\n  for (int i = 0; i <= SIZE; i++) {\n    buf[i] = analogRead(4);\n  }\n  Serial.println(buf[0]);\n}\nvoid setup() { fill(); }\nvoid loop() {}\n";

    fn block() -> Block {
        let files = [SourceFile {
            path: "a.ino".into(),
            text: SRC.into(),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        blocks(&input, &analyze(&input).facts, "ESP32-S3")
            .into_iter()
            .find(|b| b.name == "fill")
            .unwrap()
    }

    #[test]
    fn replies_come_in_many_shapes() {
        assert_eq!(parse_reply("{\"status\":\"ok\"}").unwrap().issues.len(), 0);
        let fenced = "Hier:\n```json\n{\"status\":\"issues\",\"issues\":[{\"line\":2,\"severity\":\"warning\",\"message\":\"Schleife läuft einmal zu weit.\",\"fix\":{\"old\":\"<=\",\"new\":\"<\"}}]}\n```";
        let r = parse_reply(fenced).unwrap();
        assert_eq!(r.issues.len(), 1);
        assert_eq!(r.issues[0].fix.as_ref().unwrap().new, "<");
        assert!(parse_reply("keine ahnung").is_err());
        // Mehr als drei Hinweise werden gekappt, leere Meldungen verworfen.
        let many = serde_json::json!({"status":"issues","issues":[{"line":1,"severity":"info","message":"a"},{"line":1,"severity":"info","message":" "},{"line":1,"severity":"info","message":"b"},{"line":1,"severity":"info","message":"c"},{"line":1,"severity":"info","message":"d"}]});
        assert_eq!(parse_reply(&many.to_string()).unwrap().issues.len(), 3);
    }

    #[test]
    fn issues_map_to_absolute_lines_with_a_precise_fix() {
        let b = block();
        let review = Review {
            issues: vec![Issue {
                line: 2,
                severity: "warning".into(),
                message: "Off-by-one: Der Index SIZE liegt außerhalb.".into(),
                fix: Some(Fix {
                    old: "<=".into(),
                    new: "<".into(),
                }),
            }],
        };
        let d = to_diagnostics(&b, &review, SRC);
        assert_eq!(d.len(), 1);
        assert_eq!(
            (d[0].line, d[0].severity, d[0].source),
            (4, Severity::Warning, DiagnosticSource::Ai)
        );
        let e = &d[0].fix.as_ref().unwrap().edits[0];
        assert_eq!(
            (e.from_line, e.from_col, e.to_col, e.text.as_str()),
            (4, 20, 22, "<")
        );
        // Zeilen außerhalb der Funktion werden ignoriert.
        let wild = Review {
            issues: vec![
                Issue {
                    line: 99,
                    severity: "info".into(),
                    message: "x".into(),
                    fix: None,
                },
                Issue {
                    line: 0,
                    severity: "info".into(),
                    message: "y".into(),
                    fix: None,
                },
            ],
        };
        assert!(to_diagnostics(&b, &wild, SRC).is_empty());
    }

    #[test]
    fn the_prompt_never_contains_secrets() {
        let src = "const char* password = \"geheim-test-123\";\nvoid connect() {\n  WiFi.begin(\"Buero\", \"hunter2-hunter2\");\n  Serial.println(\"mein-token-ABCDEF\");\n  delay(1000);\n  Serial.println(WiFi.status());\n}\nvoid setup() { connect(); }\nvoid loop() {}\n";
        let files = [SourceFile {
            path: "a.ino".into(),
            text: src.into(),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        let b = blocks(&input, &analyze(&input).facts, "ESP32-S3")
            .into_iter()
            .find(|b| b.name == "connect")
            .unwrap();
        let red = Redactor::new([("API".to_string(), "mein-token-ABCDEF".to_string())]);
        let p = user_prompt(&b, &red, "");
        for leaked in [
            "geheim-test-123",
            "hunter2-hunter2",
            "mein-token-ABCDEF",
            "Buero",
        ] {
            assert!(!p.contains(leaked), "{leaked} im Prompt:\n{p}");
        }
        assert!(p.contains("connect"));
    }

    #[test]
    fn fixes_become_precise_edits() {
        let b = block();
        let fixes = vec![Fix {
            old: "i <= SIZE".into(),
            new: "i < SIZE".into(),
        }];
        let e = fix_edits(&b, SRC, &fixes).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(
            (
                e[0].from_line,
                e[0].from_col,
                e[0].to_line,
                e[0].to_col,
                e[0].text.as_str()
            ),
            (4, 18, 4, 27, "i < SIZE")
        );
        // Nicht eindeutig oder gar nicht vorhanden: keine Änderung.
        let twice = vec![Fix {
            old: "buf".into(),
            new: "x".into(),
        }];
        assert!(fix_edits(&b, SRC, &twice).is_err());
        let none = vec![Fix {
            old: "gibt es nicht".into(),
            new: "x".into(),
        }];
        assert!(fix_edits(&b, SRC, &none).is_err());
        // Text außerhalb der Funktion zählt nicht.
        let outside = vec![Fix {
            old: "#define SIZE 8".into(),
            new: "x".into(),
        }];
        assert!(fix_edits(&b, SRC, &outside).is_err());
    }

    #[test]
    fn fix_replies_come_in_many_shapes() {
        let (e, note) = parse_fix(
            "```json\n{\"edits\":[{\"old\":\"a\",\"new\":\"b\"}],\"note\":\"Geändert.\"}\n```",
        )
        .unwrap();
        assert_eq!((e.len(), note.as_str()), (1, "Geändert."));
        let (e, note) = parse_fix("{\"edits\":[],\"note\":\"Unklar.\"}").unwrap();
        assert_eq!((e.len(), note.as_str()), (0, "Unklar."));
        assert!(parse_fix("nope").is_err());
    }
}
