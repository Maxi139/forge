use crate::parse::{neutralize_attribute_macros, parse};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use tree_sitter::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionInfo {
    pub name: String,
    pub return_type: String,
    /// Parameterliste samt Klammern, wie im Quelltext.
    pub params: String,
    /// Kopf ohne Rumpf und ohne Semikolon, Leerraum zusammengezogen.
    pub prototype: String,
    /// 1-basiert, einschließlich.
    pub start_line: u32,
    pub end_line: u32,
    pub start_byte: usize,
    pub end_byte: usize,
    pub body_start_byte: usize,
    /// Zeile (0-basiert) und Byte-Spalte des Namens.
    pub name_row: usize,
    pub name_col: usize,
    pub has_default_args: bool,
    pub has_attribute_macro: bool,
    pub has_error: bool,
    pub is_template: bool,
    pub is_static: bool,
    /// Aufgerufene Funktionen und Methoden im Rumpf, in der Reihenfolge des ersten Auftretens.
    pub calls: Vec<String>,
    /// Alle Bezeichner im Rumpf (auch Funktionszeiger wie bei `attachInterrupt`).
    pub idents: BTreeSet<String>,
    /// Hash über die Wörter der Funktion. Ändert sich nicht bei Leerraum und Kommentaren.
    pub hash: String,
}

fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn function_declarator(mut n: Node) -> Option<Node> {
    loop {
        match n.kind() {
            "function_declarator" => return Some(n),
            "pointer_declarator"
            | "reference_declarator"
            | "parenthesized_declarator"
            | "attributed_declarator" => {
                n = n
                    .child_by_field_name("declarator")
                    .or_else(|| n.named_child(0))?;
            }
            _ => return None,
        }
    }
}

fn collect_body(n: Node, src: &str, calls: &mut Vec<String>, idents: &mut BTreeSet<String>) {
    match n.kind() {
        "identifier" | "field_identifier" => {
            idents.insert(text(n, src).to_string());
        }
        "call_expression" => {
            if let Some(f) = n.child_by_field_name("function") {
                let name = match f.kind() {
                    "identifier" => Some(text(f, src).to_string()),
                    "field_expression" => f
                        .child_by_field_name("field")
                        .map(|x| text(x, src).to_string()),
                    "qualified_identifier" => f
                        .child_by_field_name("name")
                        .map(|x| text(x, src).to_string()),
                    _ => None,
                };
                if let Some(name) = name {
                    if !calls.contains(&name) {
                        calls.push(name);
                    }
                }
            }
        }
        _ => {}
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        collect_body(ch, src, calls, idents);
    }
}

fn contains_kind(n: Node, kind: &str) -> bool {
    if n.kind() == kind {
        return true;
    }
    let mut c = n.walk();
    let found = n.children(&mut c).any(|ch| contains_kind(ch, kind));
    found
}

fn words(n: Node, src: &str, out: &mut Vec<String>) {
    if n.kind() == "comment" {
        return;
    }
    if n.child_count() == 0 {
        let t = text(n, src).trim();
        if !t.is_empty() {
            out.push(t.to_string());
        }
        return;
    }
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        words(ch, src, out);
    }
}

fn info(def: Node, outer: Node, src: &str, orig: &str, is_template: bool) -> Option<FunctionInfo> {
    let decl = function_declarator(def.child_by_field_name("declarator")?)?;
    let name_node = decl.child_by_field_name("declarator")?;
    let name = text(name_node, src).to_string();
    let params_node = decl.child_by_field_name("parameters")?;
    let body = def.child_by_field_name("body")?;

    let head = &orig[outer.start_byte()..body.start_byte()];
    let ret = def
        .child_by_field_name("type")
        .map(|t| collapse(text(t, src)))
        .unwrap_or_default();

    let mut calls = Vec::new();
    let mut idents = BTreeSet::new();
    collect_body(body, src, &mut calls, &mut idents);

    let mut w = Vec::new();
    words(outer, src, &mut w);
    let hash = format!("{:x}", Sha256::digest(w.join(" ").as_bytes()));

    let mut has_attr = false;
    for m in [
        "IRAM_ATTR",
        "DRAM_ATTR",
        "RTC_DATA_ATTR",
        "ICACHE_RAM_ATTR",
        "ICACHE_FLASH_ATTR",
        "PROGMEM",
        "EXT_RAM_ATTR",
        "WORD_ALIGNED_ATTR",
        "NOINLINE_ATTR",
        "FORCE_INLINE_ATTR",
        "RTC_IRAM_ATTR",
        "RTC_NOINIT_ATTR",
        "RTC_FAST_ATTR",
        "RTC_SLOW_ATTR",
        "DMA_ATTR",
        "EXT_RAM_BSS_ATTR",
    ] {
        if head.contains(m) {
            has_attr = true;
            break;
        }
    }

    Some(FunctionInfo {
        name,
        return_type: ret,
        params: collapse(text(params_node, src)),
        prototype: collapse(head),
        start_line: outer.start_position().row as u32 + 1,
        end_line: outer.end_position().row as u32 + 1,
        start_byte: outer.start_byte(),
        end_byte: outer.end_byte(),
        body_start_byte: body.start_byte(),
        name_row: name_node.start_position().row,
        name_col: name_node.start_position().column,
        has_default_args: contains_kind(params_node, "optional_parameter_declaration"),
        has_attribute_macro: has_attr,
        has_error: outer.has_error(),
        is_template,
        is_static: head.split_whitespace().any(|t| t == "static"),
        calls,
        idents,
        hash,
    })
}

fn visit(n: Node, src: &str, orig: &str, out: &mut Vec<FunctionInfo>) {
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        match ch.kind() {
            "function_definition" => out.extend(info(ch, ch, src, orig, false)),
            "template_declaration" => {
                let mut cc = ch.walk();
                for inner in ch.children(&mut cc) {
                    if inner.kind() == "function_definition" {
                        out.extend(info(inner, ch, src, orig, true));
                    }
                }
            }
            "linkage_specification" => {
                if let Some(body) = ch.child_by_field_name("body") {
                    visit(body, src, orig, out);
                }
            }
            _ => {}
        }
    }
}

/// Alle Funktionen auf oberster Ebene (auch in `extern "C" { }`), in Dateireihenfolge.
pub fn functions(text: &str) -> Vec<FunctionInfo> {
    let neutral = neutralize_attribute_macros(text);
    let tree = parse(&neutral);
    let mut out = Vec::new();
    visit(tree.root_node(), &neutral, text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
volatile int counter = 0;
void IRAM_ATTR onIsr() { counter++; }
void setup() {
  Serial.begin(115200);
  attachInterrupt(4, onIsr, RISING);
  blink(3);
}
void blink(int times, int ms = 100) {
  for (int i = 0; i < times; i++) { digitalWrite(2, HIGH); delay(ms); }
}
static int helper(const char* s) { return s[0]; }
char *name() { return nullptr; }
"#;

    #[test]
    fn extracts_signatures_calls_and_flags() {
        let f = functions(SRC);
        let names: Vec<_> = f.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["onIsr", "setup", "blink", "helper", "name"]);

        let isr = &f[0];
        assert!(isr.has_attribute_macro);
        assert_eq!(isr.prototype, "void IRAM_ATTR onIsr()");

        let setup = &f[1];
        assert_eq!(setup.calls, ["begin", "attachInterrupt", "blink"]);
        assert!(setup.idents.contains("onIsr"));
        assert_eq!((setup.start_line, setup.end_line), (4, 8));

        assert!(f[2].has_default_args);
        assert!(f[3].is_static);
        assert_eq!(f[4].prototype, "char *name()");
    }

    #[test]
    fn hash_ignores_whitespace_and_comments_but_not_code() {
        let a = functions("void f() { int a = 1; }");
        let b = functions("void f()\n{\n  // Kommentar\n  int a = 1;\n}\n");
        let c = functions("void f() { int a = 2; }");
        assert_eq!(a[0].hash, b[0].hash);
        assert_ne!(a[0].hash, c[0].hash);
    }
}

/// Fingerabdruck des Programmtexts ohne Kommentare und Leerraum. Zwei Fassungen mit gleichem
/// Fingerabdruck erzeugen dieselbe Firmware (bis auf Zeilennummern in Log-Makros).
pub fn fingerprint(text: &str) -> String {
    let tree = parse(text);
    let mut w = Vec::new();
    words(tree.root_node(), text, &mut w);
    format!("{:x}", Sha256::digest(w.join(" ").as_bytes()))
}

#[cfg(test)]
mod fingerprint_tests {
    use super::fingerprint;

    #[test]
    fn comments_and_layout_do_not_change_it() {
        let a = fingerprint("int x = 1; // eins\nvoid f() {}\n");
        let b = fingerprint("/* neu */ int   x=1 ;\n\nvoid f()\n{\n}\n");
        let c = fingerprint("int x = 2; void f() {}");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
