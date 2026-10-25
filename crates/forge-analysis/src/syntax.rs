use forge_core::{utf16_col, Diagnostic, DiagnosticSource, QuickFix, Severity, TextEdit};

/// Sofortige Strukturprüfung ohne Compiler: unbalancierte Klammern, nicht beendete Zeichenketten und
/// Kommentare. Diese Fehler sind eindeutig. Alles Feinere (fehlendes `;`, Typfehler) meldet der
/// Compiler wenige Zehntelsekunden später mit genauer Position.
pub fn syntax_diagnostics(file: &str, text: &str) -> Vec<Diagnostic> {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut stack: Vec<(char, usize, usize)> = Vec::new(); // (Zeichen, Zeile 0-basiert, Byte-Spalte)

    let diag =
        |row: usize, byte_col: usize, byte_len: usize, message: String, fix: Option<QuickFix>| {
            let line = lines.get(row).copied().unwrap_or("");
            let from = utf16_col(line, byte_col);
            let to = utf16_col(line, byte_col + byte_len).max(from + 1);
            Diagnostic {
                id: format!("syntax:{file}:{}:{from}", row + 1),
                file: file.into(),
                line: row as u32 + 1,
                from,
                to,
                severity: Severity::Error,
                message,
                source: DiagnosticSource::Syntax,
                fix,
                action: None,
            }
        };

    let mut in_block_comment: Option<(usize, usize)> = None;
    // Offene Zeichenkette, die mit Backslash am Zeilenende in die nächste Zeile läuft (Makros).
    let mut continued: Option<u8> = None;
    for (row, line) in lines.iter().enumerate() {
        let b = line.as_bytes();
        let mut i = 0;
        if let Some(q) = continued.take() {
            while i < b.len() {
                if b[i] == b'\\' {
                    i += 2;
                    continue;
                }
                i += 1;
                if b[i - 1] == q {
                    break;
                }
            }
            if i >= b.len() && line.trim_end().ends_with('\\') {
                continued = Some(q);
            }
        }
        while i < b.len() {
            if in_block_comment.is_some() {
                if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    in_block_comment = None;
                    i += 2;
                } else {
                    i += 1;
                }
                continue;
            }
            match b[i] {
                b'/' if b.get(i + 1) == Some(&b'/') => break,
                b'/' if b.get(i + 1) == Some(&b'*') => {
                    in_block_comment = Some((row, i));
                    i += 2;
                }
                q @ (b'"' | b'\'') => {
                    // Ziffernzeichentrenner wie 1'000 sind kein Zeichenliteral.
                    if q == b'\''
                        && i > 0
                        && b[i - 1].is_ascii_alphanumeric()
                        && b.get(i + 1).is_some_and(|c| c.is_ascii_alphanumeric())
                    {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    i += 1;
                    let mut closed = false;
                    while i < b.len() {
                        if b[i] == b'\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == q {
                            closed = true;
                            i += 1;
                            break;
                        }
                        i += 1;
                    }
                    // Zeilenende mit Backslash setzt die Zeichenkette fort (Makros). Sonst ist sie offen.
                    if !closed && line.trim_end().ends_with('\\') {
                        continued = Some(q);
                    } else if !closed {
                        let what = if q == b'"' { "Zeichenkette" } else { "Zeichen" };
                        let fix = QuickFix {
                            label: format!(
                                "{} schließen",
                                if q == b'"' {
                                    "Anführungszeichen"
                                } else {
                                    "Apostroph"
                                }
                            ),
                            edits: vec![TextEdit {
                                from_line: row as u32 + 1,
                                from_col: utf16_col(line, line.trim_end().len()),
                                to_line: row as u32 + 1,
                                to_col: utf16_col(line, line.trim_end().len()),
                                text: (q as char).to_string(),
                            }],
                        };
                        out.push(diag(
                            row,
                            start,
                            1,
                            format!("{what} ist nicht beendet"),
                            Some(fix),
                        ));
                    }
                }
                c @ (b'(' | b'{' | b'[') => {
                    stack.push((c as char, row, i));
                    i += 1;
                }
                c @ (b')' | b'}' | b']') => {
                    let want = match c {
                        b')' => '(',
                        b'}' => '{',
                        _ => '[',
                    };
                    match stack.last() {
                        Some(&(open, _, _)) if open == want => {
                            stack.pop();
                        }
                        _ => match stack.iter().rposition(|&(o, _, _)| o == want) {
                            // Es gibt einen passenden Öffner weiter unten: dazwischen liegt eine falsche Klammer.
                            Some(pos) => {
                                let (open, orow, _) = *stack.last().unwrap();
                                out.push(diag(
                                    row,
                                    i,
                                    1,
                                    format!(
                                        "{} passt nicht zu {} aus Zeile {}",
                                        c as char,
                                        open,
                                        orow + 1
                                    ),
                                    None,
                                ));
                                stack.truncate(pos);
                            }
                            None => out.push(diag(
                                row,
                                i,
                                1,
                                format!("{} ohne öffnende Klammer", c as char),
                                None,
                            )),
                        },
                    }
                    i += 1;
                }
                _ => i += 1,
            }
        }
    }

    if let Some((row, col)) = in_block_comment {
        out.push(diag(
            row,
            col,
            2,
            "Kommentar ist nicht beendet".into(),
            None,
        ));
    }
    for (open, row, col) in stack {
        let close = match open {
            '(' => ')',
            '{' => '}',
            _ => ']',
        };
        let last = lines.len().saturating_sub(1);
        let last_line = lines.get(last).copied().unwrap_or("");
        let at = utf16_col(last_line, last_line.len());
        let fix = QuickFix {
            label: format!("„{close}“ am Ende einfügen"),
            edits: vec![TextEdit {
                from_line: last as u32 + 1,
                from_col: at,
                to_line: last as u32 + 1,
                to_col: at,
                text: format!("\n{close}"),
            }],
        };
        out.push(diag(
            row,
            col,
            1,
            format!("{open} wird nie geschlossen"),
            Some(fix),
        ));
    }
    out.truncate(8);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r##"#include <WiFi.h>
#define LED 2
#define MSG "mehrzeilig \
weiter"
// Kommentar mit ( und " darin
/* Block { */
volatile int counter = 0;
void IRAM_ATTR onIsr() { counter++; }
const char *s = "a \" ( b";
char c = '\'';
int big = 1'000;
void setup() {
  Serial.printf("v=%.2f\n", analogRead(4) * 3.3f);
  int a[3] = {1, 2, 3};
}
void loop() { delay(10); }
"##;

    #[test]
    fn valid_arduino_code_has_no_findings() {
        assert_eq!(syntax_diagnostics("a.ino", GOOD), Vec::new());
    }

    #[test]
    fn unclosed_brace_points_at_the_opener_with_a_fix() {
        let d = syntax_diagnostics("a.ino", "void setup() {\n  int a = 1;\n");
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!((d[0].line, d[0].from), (1, 13));
        assert!(d[0].message.contains("{ wird nie geschlossen"));
        assert!(d[0].fix.is_some());
    }

    #[test]
    fn stray_closing_bracket() {
        let d = syntax_diagnostics("a.ino", "void f() {\n  g(1));\n}\n");
        assert!(
            d.iter()
                .any(|x| x.line == 2 && x.message.contains("ohne öffnende")),
            "{d:?}"
        );
    }

    #[test]
    fn mismatched_pair_names_the_opener_line() {
        let d = syntax_diagnostics("a.ino", "void f() {\n  g(1];\n}\n");
        assert!(
            d.iter()
                .any(|x| x.message.contains("passt nicht zu ( aus Zeile 2")),
            "{d:?}"
        );
    }

    #[test]
    fn unterminated_string_and_comment() {
        let d = syntax_diagnostics("a.ino", "void f() {\n  Serial.println(\"hallo);\n}\n");
        assert!(
            d.iter()
                .any(|x| x.line == 2 && x.message.starts_with("Zeichenkette")),
            "{d:?}"
        );
        let c = syntax_diagnostics("a.ino", "int a;\n/* offen\nint b;\n");
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].line, c[0].message.as_str()),
            (2, "Kommentar ist nicht beendet")
        );
    }

    #[test]
    fn columns_count_utf16_units() {
        let d = syntax_diagnostics("a.ino", "void f() {\n  Serial.println(\"ä😀\"));\n}\n");
        let line = "  Serial.println(\"ä😀\"));";
        assert_eq!(d[0].from, line.encode_utf16().count() as u32 - 2);
    }
}
