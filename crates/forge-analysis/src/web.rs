//! Webdateien des Sketches (HTML, CSS, JavaScript) als prüfbare Blöcke für die KI.
//! Jede Datei ist ein Block, lange Dateien zerfallen an Leerzeilen in mehrere. Als Kontext dienen die Routen
//! des Sketches und die IDs der Seiten, damit die KI erkennt, wenn die Seite etwas ruft, das der Sketch nicht kennt.

use crate::analyze::SourceFile;
use crate::context::{line_numbered, Block};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_BLOCK_LINES: usize = 220;
const MIN_BLOCK_CHARS: usize = 120;
const MAX_LIST: usize = 40;

/// Die Sprache einer Webdatei nach ihrer Endung.
pub fn web_lang(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => Some("html"),
        "css" => Some("css"),
        "js" | "mjs" => Some("js"),
        _ => None,
    }
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Adressen, die der Sketch beantwortet: `server.on("/led", …)`, auch mit `F("…")`.
pub fn routes(sources: &[SourceFile]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for f in sources {
        let mut rest = f.text.as_str();
        while let Some(at) = rest.find(".on(") {
            rest = &rest[at + 4..];
            let arg = rest.trim_start();
            let arg = arg.strip_prefix("F(").map(str::trim_start).unwrap_or(arg);
            if let Some(body) = arg.strip_prefix('"') {
                if let Some(end) = body.find('"') {
                    let route = &body[..end];
                    if route.starts_with('/') && route.len() < 80 {
                        out.insert(route.to_string());
                    }
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Alle `id="…"` in den HTML-Dateien.
fn ids(web: &[SourceFile]) -> Vec<String> {
    let mut out = BTreeSet::new();
    for f in web.iter().filter(|f| web_lang(&f.path) == Some("html")) {
        let mut rest = f.text.as_str();
        while let Some(at) = rest.find("id=") {
            let before = rest[..at].chars().next_back();
            rest = &rest[at + 3..];
            // `data-id=` oder `pid=` sind keine IDs.
            if before.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '_') {
                continue;
            }
            let Some(q) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            if let Some(end) = rest[1..].find(q) {
                let id = &rest[1..1 + end];
                if !id.is_empty() && id.len() < 60 && !id.contains(char::is_whitespace) {
                    out.insert(id.to_string());
                }
            }
        }
    }
    out.into_iter().collect()
}

/// Namen der `{{platzhalter}}` im Text.
fn placeholders(text: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find("{{") {
        rest = &rest[at + 2..];
        let Some(end) = rest.find("}}") else { break };
        let name = rest[..end].trim();
        if !name.is_empty()
            && name.len() < 40
            && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            && !name.starts_with(|c: char| c.is_ascii_digit())
        {
            out.insert(name.to_string());
        }
    }
    out.into_iter().collect()
}

/// Teilt Zeilen in Stücke von höchstens `MAX_BLOCK_LINES`, bevorzugt an einer Leerzeile.
fn chunks(lines: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let mut end = (start + MAX_BLOCK_LINES).min(lines.len());
        if end < lines.len() {
            if let Some(blank) = (start + MAX_BLOCK_LINES / 2..end)
                .rev()
                .find(|&i| lines[i].trim().is_empty())
            {
                end = blank + 1;
            }
        }
        out.push((start, end));
        start = end;
    }
    out
}

fn limited(list: &[String]) -> String {
    let shown: Vec<&str> = list.iter().take(MAX_LIST).map(String::as_str).collect();
    let more = list.len().saturating_sub(MAX_LIST);
    if more > 0 {
        format!("{}, … (+{more})", shown.join(", "))
    } else {
        shown.join(", ")
    }
}

/// Blöcke für alle Webdateien. `cpp` ist der Quelltext des Sketches, dem die Routen entnommen werden.
pub fn web_blocks(web: &[SourceFile], cpp: &[SourceFile], board: &str) -> Vec<Block> {
    let routes = routes(cpp);
    let ids = ids(web);
    let mut out = Vec::new();
    for f in web {
        let Some(lang) = web_lang(&f.path) else {
            continue;
        };
        if f.text.trim().len() < MIN_BLOCK_CHARS {
            continue;
        }
        let lines: Vec<&str> = f.text.split('\n').collect();
        let parts = chunks(&lines);
        let name = file_name(&f.path);
        let kind = match lang {
            "html" => "HTML-Seite (kann Stile und Skripte enthalten)",
            "css" => "Stylesheet",
            _ => "JavaScript",
        };
        // Wer bindet diese Datei ein? Das hilft bei Stylesheets und Skripten.
        let users: Vec<String> = web
            .iter()
            .filter(|o| {
                web_lang(&o.path) == Some("html") && o.path != f.path && o.text.contains(name)
            })
            .map(|o| file_name(&o.path).to_string())
            .collect();
        let linked: Vec<String> = web
            .iter()
            .filter(|o| {
                o.path != f.path
                    && web_lang(&o.path) != Some("html")
                    && f.text.contains(file_name(&o.path))
            })
            .map(|o| file_name(&o.path).to_string())
            .collect();
        let names = placeholders(&f.text);

        for (i, &(from, to)) in parts.iter().enumerate() {
            let code = lines[from..to].join("\n");
            if code.trim().len() < MIN_BLOCK_CHARS {
                continue;
            }
            let mut ctx = format!("Board: {board} (Arduino-ESP32)\n");
            ctx.push_str(&format!(
                "Webdatei des Sketches: {kind}. Der ESP32 liefert sie über seinen Webserver aus, abgerufen wird sie meist mit dem Handy.\n"
            ));
            if parts.len() > 1 {
                ctx.push_str(&format!(
                    "Das ist Teil {} von {} der Datei, ab Zeile {}.\n",
                    i + 1,
                    parts.len(),
                    from + 1
                ));
            }
            if !routes.is_empty() {
                ctx.push_str(&format!(
                    "Adressen, die der Sketch beantwortet: {}\n",
                    limited(&routes)
                ));
            } else {
                ctx.push_str("Im Sketch wurden keine Adressen (server.on) gefunden.\n");
            }
            if !ids.is_empty() && lang != "html" {
                ctx.push_str(&format!(
                    "IDs in den Seiten des Projekts: {}\n",
                    limited(&ids)
                ));
            }
            if !linked.is_empty() {
                ctx.push_str(&format!("Bindet ein: {}\n", linked.join(", ")));
            }
            if !users.is_empty() {
                ctx.push_str(&format!("Eingebunden von: {}\n", users.join(", ")));
            }
            if !names.is_empty() {
                ctx.push_str(&format!(
                    "Platzhalter in doppelten geschweiften Klammern ({}) ersetzt der Sketch vor dem Ausliefern durch Werte. Sie sind kein Fehler.\n",
                    names.iter().map(|n| format!("{{{{{n}}}}}")).collect::<Vec<_>>().join(", ")
                ));
            }
            let mut h = Sha256::new();
            h.update(b"web\0");
            h.update(code.as_bytes());
            h.update(ctx.as_bytes());
            out.push(Block {
                file: f.path.clone(),
                name: if parts.len() > 1 {
                    format!("{name} (Teil {})", i + 1)
                } else {
                    name.to_string()
                },
                start_line: from as u32 + 1,
                end_line: to as u32,
                code: line_numbered(&code),
                key: format!("{:x}", h.finalize()),
                context: ctx,
                lang,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile {
            path: path.into(),
            text: text.into(),
        }
    }

    const PAGE: &str = "<!doctype html>\n<html>\n<body>\n  <button id=\"knopf\" data-id=\"x\">LED</button>\n  <p>Messwert: {{wert}}</p>\n  <script src=\"app.js\"></script>\n</body>\n</html>\n";
    const JS: &str = "document.getElementById('knopf').addEventListener('click', async () => {\n  const r = await fetch('/led', { method: 'POST' });\n  if (!r.ok) console.error('Fehler');\n});\n";
    const INO: &str = "WebServer server(80);\nvoid setup() {\n  server.on(\"/\", handleRoot);\n  server.on(F(\"/led\"), HTTP_POST, handleLed);\n  server.on( \"status\", x);\n}\n";

    #[test]
    fn routes_come_from_server_on_calls() {
        assert_eq!(routes(&[file("a.ino", INO)]), vec!["/", "/led"]);
    }

    #[test]
    fn each_web_file_becomes_a_block_with_context() {
        let web = [file("/p/web/index.html", PAGE), file("/p/web/app.js", JS)];
        let blocks = web_blocks(&web, &[file("/p/a.ino", INO)], "ESP32-S3");
        assert_eq!(blocks.len(), 2);
        let html = &blocks[0];
        assert_eq!((html.lang, html.name.as_str()), ("html", "index.html"));
        assert!(html
            .context
            .contains("Adressen, die der Sketch beantwortet: /, /led"));
        assert!(html.context.contains("{{wert}}") && html.context.contains("Bindet ein: app.js"));
        assert!(html.code.starts_with("  1  <!doctype html>"));
        let js = &blocks[1];
        assert_eq!(js.lang, "js");
        // `data-id` zählt nicht, `id="knopf"` schon.
        assert!(js
            .context
            .contains("IDs in den Seiten des Projekts: knopf\n"));
        assert!(js.context.contains("Eingebunden von: index.html"));
    }

    #[test]
    fn the_key_follows_code_and_context() {
        let key = |js: &str, ino: &str| {
            let web = [file("a.js", js)];
            web_blocks(&web, &[file("a.ino", ino)], "x")[0].key.clone()
        };
        let base = key(JS, INO);
        assert_eq!(base, key(JS, INO));
        assert_ne!(base, key(&JS.replace("/led", "/an"), INO));
        assert_ne!(base, key(JS, &INO.replace("/led", "/an")));
    }

    #[test]
    fn short_files_are_skipped_and_long_files_split_at_blank_lines() {
        assert!(web_blocks(&[file("a.js", "let a = 1;\n")], &[], "x").is_empty());
        let long: String = (0..500)
            .map(|i| {
                format!(
                    "const wert{i} = {i};\n{}",
                    if i % 40 == 39 { "\n" } else { "" }
                )
            })
            .collect();
        let blocks = web_blocks(&[file("big.js", &long)], &[], "x");
        assert!(blocks.len() >= 3);
        assert!(blocks.iter().all(|b| b.end_line - b.start_line < 220));
        // Die Teile schließen lückenlos aneinander an.
        for w in blocks.windows(2) {
            assert_eq!(w[0].end_line + 1, w[1].start_line);
        }
        assert_eq!(blocks[1].name, "big.js (Teil 2)");
    }

    /// Der erzeugte Block steht mitten im Sketch. Er darf weder Syntaxfehler noch Regelmeldungen auslösen.
    #[test]
    fn generated_blocks_do_not_disturb_the_cpp_analysis() {
        use crate::analyze::{analyze, AnalysisInput, Chip};
        let page = "<form action=\"/save\"><input type=\"password\" name=\"pass\"></form>\n<style>a { color: red; }</style>\n<script>if (a) { b(\"x\"); } // )forge\"\n</script>\n";
        let sketch = format!(
            "#include <WebServer.h>\n\n// >>> forge:web web/index.html name=INDEX_HTML\n// Erzeugt aus web/index.html.\nconst char INDEX_HTML[] PROGMEM = R\"forge1({page})forge1\";\n// <<< forge:web\n\nString renderIndex(const String& wert) {{\n  String html;\n  html.reserve(40);\n  html += F(R\"forge(<p>{{ \"a\" }}</p>)forge\");\n  html += wert;\n  return html;\n}}\n\nWebServer server(80);\nvoid setup() {{\n  server.on(\"/\", []() {{ server.send_P(200, \"text/html\", INDEX_HTML); }});\n  server.begin();\n}}\nvoid loop() {{\n  server.handleClient();\n}}\n"
        );
        assert!(
            crate::syntax_diagnostics("a.ino", &sketch).is_empty(),
            "{:?}",
            crate::syntax_diagnostics("a.ino", &sketch)
        );
        let files = [file("a.ino", &sketch)];
        let result = analyze(&AnalysisInput {
            files: &files,
            chip: Chip::Esp32S3,
        });
        let in_block = |line: u32| (3..=6).contains(&line);
        assert!(
            result.diagnostics.iter().all(|d| !in_block(d.line)),
            "{:?}",
            result.diagnostics
        );
    }
}
