use crate::functions::{functions, FunctionInfo};

pub struct SketchSource {
    pub path: String,
    pub text: String,
}

/// Der Sketch so, wie der Compiler ihn sieht: `#include <Arduino.h>`, alle `.ino`-Dateien hintereinander
/// und Funktionsprototypen vor der ersten Funktion, jeweils mit `#line`-Marken auf die Originaldateien.
pub struct Combined {
    pub text: String,
}

fn escape(path: &str) -> String {
    path.replace('\\', "\\\\").replace('"', "\\\"")
}

fn eligible(f: &FunctionInfo) -> bool {
    // Wie arduino-cli: Funktionen mit Standardargumenten oder Attribut-Makros bekommen keinen Prototyp.
    !f.has_default_args && !f.has_attribute_macro && !f.has_error
}

/// Setzt die `.ino`-Dateien (Hauptdatei zuerst, dann alphabetisch) zusammen.
pub fn combine(sources: &[SketchSource]) -> Combined {
    let parsed: Vec<Vec<FunctionInfo>> = sources.iter().map(|s| functions(&s.text)).collect();

    // Alle Prototypen in Dateireihenfolge, und der Einfügepunkt: vor der ersten passenden Funktion.
    let mut protos: Vec<(usize, u32, String)> = Vec::new();
    let mut insert_at: Option<(usize, u32)> = None;
    for (si, funcs) in parsed.iter().enumerate() {
        for f in funcs.iter().filter(|f| eligible(f)) {
            if insert_at.is_none() {
                insert_at = Some((si, f.start_line));
            }
            protos.push((si, f.start_line, format!("{};", f.prototype)));
        }
    }

    let mut out = String::from("#include <Arduino.h>\n");
    for (si, src) in sources.iter().enumerate() {
        let path = escape(&src.path);
        let lines: Vec<&str> = src.text.split('\n').collect();
        match insert_at {
            Some((at_file, at_line)) if at_file == si => {
                let split = (at_line as usize - 1).min(lines.len());
                out.push_str(&format!("#line 1 \"{path}\"\n"));
                for l in &lines[..split] {
                    out.push_str(l);
                    out.push('\n');
                }
                for (pi, line, proto) in &protos {
                    out.push_str(&format!(
                        "#line {line} \"{}\"\n{proto}\n",
                        escape(&sources[*pi].path)
                    ));
                }
                out.push_str(&format!("#line {at_line} \"{path}\"\n"));
                out.push_str(&lines[split..].join("\n"));
            }
            _ => {
                out.push_str(&format!("#line 1 \"{path}\"\n"));
                out.push_str(&src.text);
            }
        }
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }
    Combined { text: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "#include <WiFi.h>\n#define LED 2\n\nstruct Point { int x; int y; };\nstatic int counter = 0;\n\nvoid setup() { blink(3); }\nvoid loop() {\n  Point p = makePoint(1, 2);\n}\n\nvoid blink(int times, int ms = 100) {\n}\nstatic int helper(const char* s) { return s[0]; }\nPoint makePoint(int x, int y) { return Point{x, y}; }\nvoid IRAM_ATTR onIsr() { counter++; }\nchar *name() { return nullptr; }\n";

    #[test]
    fn matches_arduino_cli_preprocessing() {
        let c = combine(&[SketchSource {
            path: "/p/proto.ino".into(),
            text: SRC.into(),
        }]);
        let expect = "#include <Arduino.h>\n#line 1 \"/p/proto.ino\"\n#include <WiFi.h>\n#define LED 2\n\nstruct Point { int x; int y; };\nstatic int counter = 0;\n\n#line 7 \"/p/proto.ino\"\nvoid setup();\n#line 8 \"/p/proto.ino\"\nvoid loop();\n#line 14 \"/p/proto.ino\"\nstatic int helper(const char* s);\n#line 15 \"/p/proto.ino\"\nPoint makePoint(int x, int y);\n#line 17 \"/p/proto.ino\"\nchar *name();\n#line 7 \"/p/proto.ino\"\nvoid setup() { blink(3); }";
        assert!(c.text.starts_with(expect), "\n{}", c.text);
        assert!(c.text.ends_with("char *name() { return nullptr; }\n"));
    }

    #[test]
    fn works_without_functions() {
        let c = combine(&[SketchSource {
            path: "a.ino".into(),
            text: "int x;".into(),
        }]);
        assert_eq!(c.text, "#include <Arduino.h>\n#line 1 \"a.ino\"\nint x;\n");
    }
}
