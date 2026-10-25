//! Zerlegt einen Sketch in prüfbare Blöcke (Funktionen) und sammelt für jeden den Kontext,
//! den eine KI braucht, um Logikfehler zu erkennen: Rolle, Aufrufer, Konstanten, Globals, Pins.

use crate::analyze::{text, walk, AnalysisInput, FunctionFacts};
use crate::functions::functions;
use crate::parse::{neutralize_attribute_macros, parse};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};

const MAX_BLOCK_LINES: u32 = 150;
const MIN_BLOCK_CHARS: usize = 60;

#[derive(Debug, Clone)]
pub struct Block {
    pub file: String,
    pub name: String,
    pub start_line: u32,
    pub end_line: u32,
    /// Der Quelltext der Funktion.
    pub code: String,
    /// Hash über Funktion und Kontext. Ändert sich nur, wenn sich für die Prüfung etwas ändert.
    pub key: String,
    /// Beschreibung der Umgebung der Funktion als Text.
    pub context: String,
}

fn role(contexts: &[String]) -> &'static str {
    if contexts.iter().any(|c| c == "isr") {
        "Interrupt-Handler (läuft bei einer Flanke, unterbricht loop(); darf nicht blockieren)"
    } else if contexts.iter().any(|c| c == "task") {
        "FreeRTOS-Task oder Timer-Callback (läuft nebenläufig zu loop())"
    } else if contexts.iter().any(|c| c == "setup") {
        "setup(): läuft einmal beim Start"
    } else if contexts.iter().any(|c| c == "loop") {
        "loop(): läuft immer wieder, ohne Pause außer durch delay() und Blockierendes"
    } else if contexts.iter().any(|c| c == "unreachable") {
        "wird im Projekt nirgends aufgerufen"
    } else {
        "Hilfsfunktion oder Callback, wird von anderem Code aufgerufen"
    }
}

/// Kopfzeilen: `#define`-Konstanten und globale Deklarationen, die die Funktion benutzt.
fn declarations(
    files: &[crate::analyze::SourceFile],
    idents: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let mut defines = Vec::new();
    let mut globals = Vec::new();
    for f in files {
        let neutral = neutralize_attribute_macros(&f.text);
        let tree = parse(&neutral);
        let lines: Vec<&str> = f.text.split('\n').collect();
        let root = tree.root_node();
        let mut c = root.walk();
        for top in root.children(&mut c) {
            let row = top.start_position().row;
            match top.kind() {
                "preproc_def" => {
                    if let Some(n) = top.child_by_field_name("name") {
                        if idents.contains(text(n, &neutral)) {
                            let line = lines.get(row).copied().unwrap_or("");
                            defines
                                .push(line.split("//").next().unwrap_or(line).trim().to_string());
                        }
                    }
                }
                "declaration" => {
                    let mut hit = false;
                    walk(top, &mut |n| {
                        if n.kind() == "identifier" && idents.contains(text(n, &neutral)) {
                            hit = true;
                        }
                    });
                    if hit {
                        // Nur die Deklaration selbst, ohne Kommentare drumherum.
                        let _ = row;
                        globals.push(
                            text(top, &neutral)
                                .split_whitespace()
                                .collect::<Vec<_>>()
                                .join(" ")
                                .chars()
                                .take(200)
                                .collect(),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    (defines, globals)
}

fn line_numbered(code: &str) -> String {
    code.split('\n')
        .enumerate()
        .map(|(i, l)| format!("{:>3}  {}", i + 1, l))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Alle Funktionen eines Sketches als prüfbare Blöcke. Zu kleine und zu große bleiben außen vor.
pub fn blocks(input: &AnalysisInput, facts: &[FunctionFacts], board: &str) -> Vec<Block> {
    let by_name: HashMap<&str, &FunctionFacts> =
        facts.iter().map(|f| (f.name.as_str(), f)).collect();
    let includes: BTreeSet<String> = input
        .files
        .iter()
        .flat_map(|f| crate::includes::includes(&f.text))
        .map(|i| i.name)
        .collect();
    let mut out = Vec::new();

    for file in input
        .files
        .iter()
        .filter(|f| f.path.ends_with(".ino") || f.path.ends_with(".cpp") || f.path.ends_with(".c"))
    {
        for func in functions(&file.text) {
            let lines = func.end_line - func.start_line + 1;
            if func.has_error
                || lines > MAX_BLOCK_LINES
                || file.text[func.body_start_byte..func.end_byte].len() < MIN_BLOCK_CHARS
            {
                continue;
            }
            let code = file.text[func.start_byte..func.end_byte].to_string();
            let Some(fact) = facts.iter().find(|f| {
                f.file == file.path && f.name == func.name && f.start_line == func.start_line
            }) else {
                continue;
            };

            let (defines, globals) = declarations(input.files, &func.idents);
            let callees: Vec<String> = fact
                .callees
                .iter()
                .filter_map(|c| by_name.get(c.as_str()))
                .take(8)
                .map(|f| format!("{};", f.signature))
                .collect();
            let callers: Vec<String> = fact.callers.iter().take(6).cloned().collect();

            let mut ctx = String::new();
            ctx.push_str(&format!("Board: {board} (Arduino-ESP32)\n"));
            ctx.push_str(&format!("Rolle: {}\n", role(&fact.contexts)));
            if !callers.is_empty() {
                ctx.push_str(&format!("Aufgerufen von: {}\n", callers.join(", ")));
            }
            if !callees.is_empty() {
                ctx.push_str(&format!(
                    "Ruft diese Funktionen des Projekts auf:\n{}\n",
                    callees.join("\n")
                ));
            }
            if !defines.is_empty() {
                ctx.push_str(&format!("Konstanten:\n{}\n", defines.join("\n")));
            }
            if !globals.is_empty() {
                ctx.push_str(&format!("Globale Variablen:\n{}\n", globals.join("\n")));
            }
            if !includes.is_empty() {
                ctx.push_str(&format!(
                    "Eingebundene Header: {}\n",
                    includes.iter().cloned().collect::<Vec<_>>().join(", ")
                ));
            }
            ctx.push_str(&format!("Aufrufstellen im Projekt: {}\n", fact.references));

            let mut h = Sha256::new();
            h.update(func.hash.as_bytes());
            h.update(ctx.as_bytes());
            out.push(Block {
                file: file.path.clone(),
                name: func.name.clone(),
                start_line: func.start_line,
                end_line: func.end_line,
                code: line_numbered(&code),
                key: format!("{:x}", h.finalize()),
                context: ctx,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{analyze, Chip, SourceFile};

    const SRC: &str = "#define SENSOR_PIN 4\n#define LIMIT 100\nvolatile int counter = 0;\nint last = 0;\nvoid IRAM_ATTR onPress() { counter++; }\nint readSensor() {\n  int raw = analogRead(SENSOR_PIN);\n  if (raw > LIMIT) last = raw;\n  return raw;\n}\nvoid setup() { attachInterrupt(4, onPress, FALLING); }\nvoid loop() {\n  int v = readSensor();\n  Serial.println(v);\n  if (v > LIMIT) {\n    Serial.println(\"zu hoch\");\n  }\n  delay(100);\n}\n";

    fn run() -> Vec<Block> {
        let files = [SourceFile {
            path: "a.ino".into(),
            text: SRC.into(),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        let facts = analyze(&input).facts;
        blocks(&input, &facts, "ESP32-S3")
    }

    #[test]
    fn blocks_carry_role_callers_constants_and_globals() {
        let b = run();
        let read = b.iter().find(|x| x.name == "readSensor").unwrap();
        assert!(
            read.context.contains("Aufgerufen von: loop"),
            "{}",
            read.context
        );
        assert!(
            read.context.contains("#define SENSOR_PIN 4")
                && read.context.contains("#define LIMIT 100")
        );
        assert!(read.context.contains("int last = 0;"));
        assert!(read.context.contains("Rolle: Hilfsfunktion"));
        assert!(read.code.starts_with("  1  int readSensor() {"));
        // Zu kurze Funktionen werden nicht geprüft.
        assert!(b.iter().all(|x| x.name != "onPress" && x.name != "setup"));
        let lp = b.iter().find(|x| x.name == "loop").unwrap();
        assert!(lp.context.contains("Rolle: loop()") && lp.context.contains("int readSensor();"));
    }

    #[test]
    fn the_key_changes_with_code_or_context_only() {
        let a = run();
        let files = [SourceFile {
            path: "a.ino".into(),
            text: SRC
                .replace("// x", "")
                .replace("int last = 0;", "int last = 0; // Kommentar"),
        }];
        let input = AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        };
        let facts = analyze(&input).facts;
        let b = blocks(&input, &facts, "ESP32-S3");
        let key = |v: &[Block], n: &str| v.iter().find(|x| x.name == n).unwrap().key.clone();
        assert_eq!(key(&a, "readSensor"), key(&b, "readSensor"));
        // Eine geänderte Konstante ändert den Kontext und damit den Schlüssel.
        let files2 = [SourceFile {
            path: "a.ino".into(),
            text: SRC.replace("LIMIT 100", "LIMIT 200"),
        }];
        let input2 = AnalysisInput {
            files: &files2,
            chip: Chip::Esp32S3,
        };
        let c = blocks(&input2, &analyze(&input2).facts, "ESP32-S3");
        assert_ne!(key(&a, "readSensor"), key(&c, "readSensor"));
    }
}
