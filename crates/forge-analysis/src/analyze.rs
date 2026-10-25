//! Regeln, die ohne KI und ohne Compiler Fehler in der Logik von ESP32-Sketches finden:
//! tote Funktionen, unvollständige switch-Fälle, Interrupt-Fallen und Pinregeln.
//! Außerdem liefert sie die Fakten, die der KI-Prüfung als Kontext dienen.

use crate::functions::{functions, FunctionInfo};
use crate::parse::{neutralize_attribute_macros, parse};
use forge_core::{utf16_col, Diagnostic, DiagnosticSource, QuickFix, Severity, TextEdit};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use tree_sitter::Node;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chip {
    Esp32,
    Esp32S3,
    Esp32C3,
    Other,
}

impl Chip {
    pub fn from_fqbn(fqbn: &str) -> Self {
        let board = fqbn.split(':').nth(2).unwrap_or("").to_lowercase();
        if board.contains("esp32s3") || board.contains("s3") {
            Chip::Esp32S3
        } else if board.contains("esp32c3") || board.contains("c3") {
            Chip::Esp32C3
        } else if board.starts_with("esp32")
            && !board.contains("s2")
            && !board.contains("c6")
            && !board.contains("h2")
            && !board.contains("p4")
        {
            Chip::Esp32
        } else {
            Chip::Other
        }
    }
}

pub struct SourceFile {
    pub path: String,
    pub text: String,
}

pub struct AnalysisInput<'a> {
    pub files: &'a [SourceFile],
    pub chip: Chip,
}

/// Fakten über eine Funktion, wie sie die KI-Prüfung als Kontext braucht.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionFacts {
    pub file: String,
    pub name: String,
    pub signature: String,
    pub start_line: u32,
    pub end_line: u32,
    pub hash: String,
    /// `setup`, `loop`, `isr`, `task`, `callback` oder `unreachable`.
    pub contexts: Vec<String>,
    pub callers: Vec<String>,
    pub callees: Vec<String>,
    /// Wie oft der Name im Projekt vorkommt, ohne die Definition.
    pub references: usize,
    pub globals_used: Vec<String>,
    pub pins: Vec<u32>,
}

#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub diagnostics: Vec<Diagnostic>,
    pub facts: Vec<FunctionFacts>,
}

struct Global {
    volatile: bool,
    line: usize,
    col: usize,
}

const TASK_CALLS: &[&str] = &[
    "xTaskCreate",
    "xTaskCreatePinnedToCore",
    "xTaskCreateStatic",
    "xTaskCreateUniversal",
    "xTimerCreate",
    "esp_timer_create",
];
const ISR_NOT_ALLOWED: &[&str] = &[
    "delay",
    "delayMicroseconds",
    "Serial",
    "Serial1",
    "Serial2",
    "printf",
    "malloc",
    "free",
    "WiFi",
    "HTTPClient",
    "Wire",
    "SPI",
    "SD",
    "SPIFFS",
    "LittleFS",
    "String",
    "vTaskDelay",
];
const MUST_CHECK: &[&str] = &[
    "SPIFFS.begin",
    "LittleFS.begin",
    "SD.begin",
    "nvs_flash_init",
    "esp_wifi_init",
    "esp_wifi_start",
    "esp_now_init",
    "esp_now_add_peer",
    "esp_now_send",
    "i2c_driver_install",
    "uart_driver_install",
];
const PIN_CALLS: &[&str] = &[
    "pinMode",
    "digitalWrite",
    "digitalRead",
    "analogRead",
    "analogWrite",
    "ledcAttach",
    "touchRead",
    "tone",
    "pulseIn",
];

pub(crate) fn text<'a>(n: Node, src: &'a str) -> &'a str {
    &src[n.start_byte()..n.end_byte()]
}

struct Ctx<'a> {
    file: &'a str,
    lines: Vec<&'a str>,
}

impl Ctx<'_> {
    fn diag(
        &self,
        rule: &str,
        row: usize,
        col_byte: usize,
        len_bytes: usize,
        severity: Severity,
        message: String,
        fix: Option<QuickFix>,
    ) -> Diagnostic {
        let line = self.lines.get(row).copied().unwrap_or("");
        let from = utf16_col(line, col_byte);
        let to = utf16_col(line, col_byte + len_bytes).max(from + 1);
        Diagnostic {
            id: format!("analysis:{rule}:{}:{}:{from}", self.file, row + 1),
            file: self.file.into(),
            line: row as u32 + 1,
            from,
            to,
            severity,
            message,
            source: DiagnosticSource::Analysis,
            fix,
            action: None,
        }
    }

    fn insert_fix(&self, label: &str, row: usize, col_byte: usize, insert: &str) -> QuickFix {
        let line = self.lines.get(row).copied().unwrap_or("");
        let col = utf16_col(line, col_byte);
        QuickFix {
            label: label.into(),
            edits: vec![TextEdit {
                from_line: row as u32 + 1,
                from_col: col,
                to_line: row as u32 + 1,
                to_col: col,
                text: insert.into(),
            }],
        }
    }
}

pub(crate) fn walk<'t>(n: Node<'t>, f: &mut impl FnMut(Node<'t>)) {
    f(n);
    let mut c = n.walk();
    for ch in n.children(&mut c) {
        walk(ch, f);
    }
}

/// `#define NAME 4` und `const int NAME = 4;` als Namenstabelle.
fn constants(root: Node, src: &str) -> HashMap<String, i64> {
    let mut out = HashMap::new();
    walk(root, &mut |n| match n.kind() {
        "preproc_def" => {
            if let (Some(name), Some(value)) = (
                n.child_by_field_name("name"),
                n.child_by_field_name("value"),
            ) {
                if let Ok(v) = text(value, src).trim().parse::<i64>() {
                    out.insert(text(name, src).to_string(), v);
                }
            }
        }
        "init_declarator" => {
            if let (Some(d), Some(v)) = (
                n.child_by_field_name("declarator"),
                n.child_by_field_name("value"),
            ) {
                if d.kind() == "identifier" && v.kind() == "number_literal" {
                    if let Ok(x) = text(v, src).trim().parse::<i64>() {
                        out.entry(text(d, src).to_string()).or_insert(x);
                    }
                }
            }
        }
        _ => {}
    });
    out
}

/// Globale Variablen auf oberster Ebene.
fn globals(root: Node, src: &str) -> BTreeMap<String, Global> {
    let mut out = BTreeMap::new();
    let mut c = root.walk();
    for d in root.children(&mut c) {
        if d.kind() != "declaration" {
            continue;
        }
        let volatile = {
            let mut cc = d.walk();
            let v = d
                .children(&mut cc)
                .any(|x| x.kind() == "type_qualifier" && text(x, src) == "volatile");
            v
        };
        let mut cc = d.walk();
        for decl in d.children_by_field_name("declarator", &mut cc) {
            let ident = match decl.kind() {
                "init_declarator" => decl.child_by_field_name("declarator"),
                _ => Some(decl),
            };
            let mut ident = ident;
            while let Some(i) = ident {
                if matches!(
                    i.kind(),
                    "pointer_declarator" | "array_declarator" | "reference_declarator"
                ) {
                    ident = i.child_by_field_name("declarator");
                } else {
                    break;
                }
            }
            if let Some(i) = ident.filter(|i| i.kind() == "identifier") {
                out.insert(
                    text(i, src).to_string(),
                    Global {
                        volatile,
                        line: d.start_position().row,
                        col: d.start_position().column,
                    },
                );
            }
        }
    }
    out
}

struct EnumInfo {
    name: String,
    members: Vec<String>,
}

fn enums(root: Node, src: &str) -> Vec<EnumInfo> {
    let mut out = Vec::new();
    walk(root, &mut |n| {
        if n.kind() == "enum_specifier" {
            let Some(body) = n.child_by_field_name("body") else {
                return;
            };
            let name = n
                .child_by_field_name("name")
                .map(|x| text(x, src).to_string())
                .unwrap_or_default();
            let mut c = body.walk();
            let members = body
                .children(&mut c)
                .filter(|e| e.kind() == "enumerator")
                .filter_map(|e| e.child_by_field_name("name"))
                .map(|e| text(e, src).to_string())
                .collect();
            out.push(EnumInfo { name, members });
        }
    });
    out
}

fn file_functions<'a>(root: Node<'a>) -> Vec<Node<'a>> {
    let mut v = Vec::new();
    walk(root, &mut |n| {
        if n.kind() == "function_definition" {
            v.push(n);
        }
    });
    v
}

pub fn analyze(input: &AnalysisInput) -> AnalysisResult {
    let mut result = AnalysisResult::default();

    // Zuerst alles einlesen.
    struct Parsed<'a> {
        file: &'a SourceFile,
        neutral: String,
        funcs: Vec<FunctionInfo>,
    }
    let parsed: Vec<Parsed> = input
        .files
        .iter()
        .map(|f| Parsed {
            file: f,
            neutral: neutralize_attribute_macros(&f.text),
            funcs: functions(&f.text),
        })
        .collect();

    let mut all_funcs: Vec<(usize, &FunctionInfo)> = Vec::new();
    for (i, p) in parsed.iter().enumerate() {
        for f in &p.funcs {
            all_funcs.push((i, f));
        }
    }
    let names: HashSet<&str> = all_funcs.iter().map(|(_, f)| f.name.as_str()).collect();

    // Bezeichner außerhalb von Funktionsrümpfen (Tabellen mit Funktionszeigern, Initialisierer).
    let mut global_refs: HashSet<String> = HashSet::new();
    for p in &parsed {
        let tree = parse(&p.neutral);
        let mut c = tree.root_node().walk();
        for top in tree.root_node().children(&mut c) {
            if top.kind() != "function_definition" {
                walk(top, &mut |n| {
                    if n.kind() == "identifier" {
                        global_refs.insert(text(n, &p.neutral).to_string());
                    }
                });
            }
        }
    }

    // ISRs und Tasks: Funktionen, die an attachInterrupt oder xTaskCreate gehen, oder IRAM_ATTR tragen.
    let mut isr: HashSet<String> = HashSet::new();
    let mut tasks: HashSet<String> = HashSet::new();
    for p in &parsed {
        let tree = parse(&p.neutral);
        walk(tree.root_node(), &mut |n| {
            if n.kind() != "call_expression" {
                return;
            }
            let Some(fun) = n.child_by_field_name("function") else {
                return;
            };
            let callee = text(fun, &p.neutral);
            let Some(args) = n.child_by_field_name("arguments") else {
                return;
            };
            let ids: Vec<String> = {
                let mut c = args.walk();
                args.named_children(&mut c)
                    .filter(|a| a.kind() == "identifier")
                    .map(|a| text(a, &p.neutral).to_string())
                    .collect()
            };
            if callee == "attachInterrupt" || callee == "attachInterruptArg" {
                isr.extend(ids.into_iter().filter(|i| names.contains(i.as_str())));
            } else if TASK_CALLS.contains(&callee) {
                tasks.extend(ids.into_iter().filter(|i| names.contains(i.as_str())));
            }
        });
    }
    for (_, f) in &all_funcs {
        if f.has_attribute_macro
            && (f.prototype.contains("IRAM_ATTR") || f.prototype.contains("ICACHE_RAM_ATTR"))
        {
            isr.insert(f.name.clone());
        }
    }

    // Erreichbarkeit.
    let mut reach: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = ["setup", "loop", "app_main"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    queue.extend(isr.iter().cloned());
    queue.extend(tasks.iter().cloned());
    queue.extend(
        global_refs
            .iter()
            .filter(|g| names.contains(g.as_str()))
            .cloned(),
    );
    // Funktionen in .cpp ohne static sind von außen sichtbar: als erreichbar behandeln.
    for (i, f) in &all_funcs {
        let is_cpp = !parsed[*i].file.path.ends_with(".ino");
        if is_cpp && !f.is_static {
            queue.push(f.name.clone());
        }
    }
    while let Some(n) = queue.pop() {
        if !reach.insert(n.clone()) {
            continue;
        }
        for (_, f) in all_funcs.iter().filter(|(_, f)| f.name == n) {
            queue.extend(
                f.idents
                    .iter()
                    .filter(|i| names.contains(i.as_str()))
                    .cloned(),
            );
        }
    }

    // Aufrufer.
    let mut callers: HashMap<&str, BTreeSet<String>> = HashMap::new();
    for (_, f) in &all_funcs {
        for callee in f
            .idents
            .iter()
            .filter(|i| names.contains(i.as_str()) && **i != f.name)
        {
            callers
                .entry(callee.as_str())
                .or_default()
                .insert(f.name.clone());
        }
    }

    let all_text: String = input
        .files
        .iter()
        .map(|f| f.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let uses_wifi = all_text.contains("WiFi.h")
        || all_text.contains("WiFi.begin")
        || all_text.contains("esp_wifi");

    for p in &parsed {
        let tree = parse(&p.neutral);
        let root = tree.root_node();
        let ctx = Ctx {
            file: &p.file.path,
            lines: p.file.text.split('\n').collect(),
        };
        let consts = constants(root, &p.neutral);
        let globs = globals(root, &p.neutral);
        let enum_list = enums(root, &p.neutral);

        // 1. Funktionen, die nie aufgerufen werden.
        for f in &p.funcs {
            if !reach.contains(&f.name)
                && (p.file.path.ends_with(".ino") || f.is_static)
                && !f.is_template
            {
                result.diagnostics.push(ctx.diag(
                    "unreachable",
                    f.name_row,
                    f.name_col,
                    f.name.len(),
                    Severity::Info,
                    format!("„{}“ wird nirgends aufgerufen.", f.name),
                    None,
                ));
            }
        }

        for fnode in file_functions(root) {
            let Some(decl) = fnode.child_by_field_name("declarator") else {
                continue;
            };
            let mut d = decl;
            while matches!(
                d.kind(),
                "pointer_declarator" | "reference_declarator" | "parenthesized_declarator"
            ) {
                match d
                    .child_by_field_name("declarator")
                    .or_else(|| d.named_child(0))
                {
                    Some(x) => d = x,
                    None => break,
                }
            }
            let fname_node = d.child_by_field_name("declarator");
            let Some(fname) = fname_node.map(|n| text(n, &p.neutral).to_string()) else {
                continue;
            };
            let Some(body) = fnode.child_by_field_name("body") else {
                continue;
            };
            let in_isr = isr.contains(&fname);

            // 2. Interrupt-Regeln.
            if in_isr {
                let fi = p.funcs.iter().find(|f| f.name == fname);
                if let Some(fi) = fi {
                    if !fi.prototype.contains("IRAM_ATTR") {
                        let fix = ctx.insert_fix(
                            "IRAM_ATTR einfügen",
                            fi.name_row,
                            fi.name_col,
                            "IRAM_ATTR ",
                        );
                        result.diagnostics.push(ctx.diag(
                            "iram",
                            fi.name_row,
                            fi.name_col,
                            fi.name.len(),
                            Severity::Warning,
                            format!(
                                "„{}“ ist ein Interrupt-Handler und braucht IRAM_ATTR.",
                                fi.name
                            ),
                            Some(fix),
                        ));
                    }
                }
                let mut reported_forbidden: HashSet<String> = HashSet::new();
                let mut reported_volatile: HashSet<String> = HashSet::new();
                let mut locals: HashSet<String> = HashSet::new();
                walk(body, &mut |n| {
                    if n.kind() == "init_declarator" {
                        if let Some(i) = n
                            .child_by_field_name("declarator")
                            .filter(|i| i.kind() == "identifier")
                        {
                            locals.insert(text(i, &p.neutral).to_string());
                        }
                    }
                });
                walk(body, &mut |n| match n.kind() {
                    "call_expression" => {
                        let Some(fun) = n.child_by_field_name("function") else {
                            return;
                        };
                        let root_name = if fun.kind() == "field_expression" {
                            fun.child_by_field_name("argument")
                                .map(|a| text(a, &p.neutral).to_string())
                        } else {
                            Some(text(fun, &p.neutral).to_string())
                        };
                        if let Some(r) = root_name.filter(|r| ISR_NOT_ALLOWED.contains(&r.as_str()))
                        {
                            if reported_forbidden.insert(r.clone()) {
                                let pos = fun.start_position();
                                result.diagnostics.push(ctx.diag("isr-call", pos.row, pos.column, (fun.end_byte() - fun.start_byte()).min(40), Severity::Warning, format!("„{r}“ darf nicht in einem Interrupt aufgerufen werden. Setze im Handler nur ein Flag und arbeite in loop() weiter."), None));
                            }
                        }
                    }
                    "assignment_expression" | "update_expression" => {
                        let target = n
                            .child_by_field_name("left")
                            .or_else(|| n.child_by_field_name("argument"));
                        let mut t = target;
                        while let Some(x) = t {
                            if x.kind() == "subscript_expression" || x.kind() == "field_expression"
                            {
                                t = x.child_by_field_name("argument");
                            } else {
                                break;
                            }
                        }
                        if let Some(x) = t.filter(|x| x.kind() == "identifier") {
                            let name = text(x, &p.neutral).to_string();
                            if let Some(g) = globs
                                .get(&name)
                                .filter(|g| !g.volatile && !locals.contains(&name))
                            {
                                if reported_volatile.insert(name.clone()) {
                                    let fix = ctx.insert_fix(
                                        "volatile einfügen",
                                        g.line,
                                        g.col,
                                        "volatile ",
                                    );
                                    let pos = x.start_position();
                                    result.diagnostics.push(ctx.diag("volatile", pos.row, pos.column, name.len(), Severity::Warning, format!("„{name}“ wird im Interrupt geändert, ist aber nicht volatile. loop() kann den neuen Wert verpassen."), Some(fix)));
                                }
                            }
                        }
                    }
                    _ => {}
                });
            }

            // 3. Ungeprüfte Rückgabewerte.
            walk(body, &mut |n| {
                if n.kind() == "expression_statement" {
                    if let Some(call) = n.named_child(0).filter(|c| c.kind() == "call_expression") {
                        if let Some(fun) = call.child_by_field_name("function") {
                            let callee = text(fun, &p.neutral).replace(' ', "");
                            if MUST_CHECK.contains(&callee.as_str()) {
                                let pos = fun.start_position();
                                result.diagnostics.push(ctx.diag("unchecked", pos.row, pos.column, callee.len(), Severity::Warning, format!("Das Ergebnis von „{callee}“ wird nicht geprüft. Bei einem Fehler läuft der Sketch ohne Dateisystem oder Funk weiter."), None));
                            }
                        }
                    }
                }
            });

            // 4. Pinregeln.
            walk(body, &mut |n| {
                if n.kind() != "call_expression" {
                    return;
                }
                let Some(fun) = n.child_by_field_name("function") else {
                    return;
                };
                let callee = text(fun, &p.neutral);
                if !PIN_CALLS.contains(&callee) {
                    return;
                }
                let Some(args) = n.child_by_field_name("arguments") else {
                    return;
                };
                let mut c = args.walk();
                let argv: Vec<Node> = args.named_children(&mut c).collect();
                let Some(first) = argv.first() else { return };
                let pin = match first.kind() {
                    "number_literal" => text(*first, &p.neutral).trim().parse::<i64>().ok(),
                    "identifier" => consts.get(text(*first, &p.neutral)).copied(),
                    _ => None,
                };
                let Some(pin) = pin.filter(|p| (0..64).contains(p)) else {
                    return;
                };
                let as_output = callee == "digitalWrite"
                    || callee == "analogWrite"
                    || callee == "ledcAttach"
                    || callee == "tone"
                    || (callee == "pinMode"
                        && argv
                            .get(1)
                            .is_some_and(|m| text(*m, &p.neutral).contains("OUTPUT")));
                let pos = first.start_position();
                let len = first.end_byte() - first.start_byte();
                if let Some((sev, msg)) = pin_issue(input.chip, pin, callee, as_output, uses_wifi) {
                    result
                        .diagnostics
                        .push(ctx.diag("pin", pos.row, pos.column, len, sev, msg, None));
                }
            });

            // 5. Unvollständige switch-Fälle.
            walk(body, &mut |n| {
                if n.kind() != "switch_statement" {
                    return;
                }
                let Some(sw_body) = n.child_by_field_name("body") else {
                    return;
                };
                let mut has_default = false;
                let mut cases: Vec<String> = Vec::new();
                let mut c = sw_body.walk();
                for cs in sw_body
                    .children(&mut c)
                    .filter(|x| x.kind() == "case_statement")
                {
                    match cs.child_by_field_name("value") {
                        Some(v) if v.kind() == "identifier" => {
                            cases.push(text(v, &p.neutral).to_string())
                        }
                        Some(_) => {}
                        None => has_default = true,
                    }
                }
                if has_default || cases.is_empty() {
                    return;
                }
                for e in &enum_list {
                    if cases.iter().all(|c| e.members.contains(c)) && cases.len() < e.members.len()
                    {
                        let missing: Vec<&str> = e
                            .members
                            .iter()
                            .filter(|m| !cases.contains(m))
                            .map(String::as_str)
                            .collect();
                        let pos = n.start_position();
                        let label = if e.name.is_empty() {
                            "enum".to_string()
                        } else {
                            e.name.clone()
                        };
                        result.diagnostics.push(ctx.diag(
                            "switch",
                            pos.row,
                            pos.column,
                            6,
                            Severity::Warning,
                            format!(
                                "Der switch über {label} behandelt {} nicht und hat kein default.",
                                missing.join(", ")
                            ),
                            None,
                        ));
                        break;
                    }
                }
            });
        }
    }

    // Fakten für die KI-Prüfung.
    for (i, f) in &all_funcs {
        let mut contexts = Vec::new();
        if f.name == "setup" {
            contexts.push("setup".to_string());
        }
        if f.name == "loop" {
            contexts.push("loop".to_string());
        }
        if isr.contains(&f.name) {
            contexts.push("isr".to_string());
        }
        if tasks.contains(&f.name) {
            contexts.push("task".to_string());
        }
        if contexts.is_empty() && reach.contains(&f.name) {
            contexts.push("callback".to_string());
        }
        if !reach.contains(&f.name) {
            contexts.push("unreachable".to_string());
        }
        let refs = input
            .files
            .iter()
            .map(|x| x.text.matches(&f.name).count())
            .sum::<usize>()
            .saturating_sub(1);
        let file = parsed[*i].file;
        let tree = parse(&parsed[*i].neutral);
        let consts = constants(tree.root_node(), &parsed[*i].neutral);
        let globs = globals(tree.root_node(), &parsed[*i].neutral);
        let globals_used: Vec<String> = f
            .idents
            .iter()
            .filter(|x| globs.contains_key(x.as_str()))
            .cloned()
            .collect();
        let mut pins: BTreeSet<u32> = BTreeSet::new();
        for id in &f.idents {
            if let Some(v) = consts.get(id).filter(|v| (0..64).contains(*v)) {
                if id.to_uppercase().contains("PIN")
                    || id.to_uppercase().contains("LED")
                    || id.to_uppercase().contains("BTN")
                    || id.to_uppercase().contains("SENSOR")
                {
                    pins.insert(*v as u32);
                }
            }
        }
        result.facts.push(FunctionFacts {
            file: file.path.clone(),
            name: f.name.clone(),
            signature: f.prototype.clone(),
            start_line: f.start_line,
            end_line: f.end_line,
            hash: f.hash.clone(),
            contexts,
            callers: callers
                .get(f.name.as_str())
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default(),
            callees: f
                .idents
                .iter()
                .filter(|x| names.contains(x.as_str()) && **x != f.name)
                .cloned()
                .collect(),
            references: refs,
            globals_used,
            pins: pins.into_iter().collect(),
        });
    }

    result
        .diagnostics
        .sort_by(|a, b| (a.file.as_str(), a.line, a.from).cmp(&(b.file.as_str(), b.line, b.from)));
    result.diagnostics.dedup_by(|a, b| a.id == b.id);
    result
}

fn pin_issue(
    chip: Chip,
    pin: i64,
    callee: &str,
    as_output: bool,
    uses_wifi: bool,
) -> Option<(Severity, String)> {
    let p = pin as u32;
    match chip {
        Chip::Esp32 => {
            if (6..=11).contains(&p) {
                return Some((
                    Severity::Error,
                    format!(
                        "GPIO {p} ist mit dem Flash-Speicher verbunden und nicht frei nutzbar."
                    ),
                ));
            }
            if (34..=39).contains(&p) && as_output {
                return Some((
                    Severity::Error,
                    format!("GPIO {p} ist nur ein Eingang und kann nicht als Ausgang dienen."),
                ));
            }
            if callee == "analogRead"
                && uses_wifi
                && [0, 2, 4, 12, 13, 14, 15, 25, 26, 27].contains(&p)
            {
                return Some((Severity::Warning, format!("GPIO {p} gehört zu ADC2. Mit aktivem WLAN liefert analogRead dort keine Werte. Nimm einen ADC1-Pin (32 bis 39).")));
            }
            if [0, 2, 5, 12, 15].contains(&p) && as_output {
                return Some((Severity::Info, format!("GPIO {p} ist ein Strapping-Pin. Eine Last daran kann den Start des Boards stören.")));
            }
        }
        Chip::Esp32S3 => {
            if (26..=32).contains(&p) {
                return Some((
                    Severity::Error,
                    format!(
                        "GPIO {p} ist mit dem Flash-Speicher verbunden und nicht frei nutzbar."
                    ),
                ));
            }
            if (33..=37).contains(&p) {
                return Some((
                    Severity::Info,
                    format!("GPIO {p} ist bei Boards mit Octal-PSRAM belegt."),
                ));
            }
            if callee == "analogRead" && uses_wifi && (11..=20).contains(&p) {
                return Some((Severity::Warning, format!("GPIO {p} gehört zu ADC2. Mit aktivem WLAN liefert analogRead dort keine Werte. Nimm einen Pin von 1 bis 10.")));
            }
            if (p == 19 || p == 20) && as_output {
                return Some((
                    Severity::Info,
                    format!("GPIO {p} ist die USB-Schnittstelle des Chips."),
                ));
            }
            if [0, 3, 45, 46].contains(&p) && as_output {
                return Some((Severity::Info, format!("GPIO {p} ist ein Strapping-Pin. Eine Last daran kann den Start des Boards stören.")));
            }
        }
        Chip::Esp32C3 => {
            if (12..=17).contains(&p) {
                return Some((
                    Severity::Error,
                    format!(
                        "GPIO {p} ist mit dem Flash-Speicher verbunden und nicht frei nutzbar."
                    ),
                ));
            }
            if callee == "analogRead" && uses_wifi && p == 5 {
                return Some((
                    Severity::Warning,
                    "GPIO 5 gehört zu ADC2. Mit aktivem WLAN liefert analogRead dort keine Werte."
                        .into(),
                ));
            }
            if [2, 8, 9].contains(&p) && as_output {
                return Some((Severity::Info, format!("GPIO {p} ist ein Strapping-Pin. Eine Last daran kann den Start des Boards stören.")));
            }
        }
        Chip::Other => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(chip: Chip, src: &str) -> AnalysisResult {
        analyze(&AnalysisInput {
            files: &[SourceFile {
                path: "a.ino".into(),
                text: src.into(),
            }],
            chip,
        })
    }

    fn rules(r: &AnalysisResult) -> Vec<String> {
        r.diagnostics
            .iter()
            .map(|d| d.id.split(':').nth(1).unwrap_or("").to_string())
            .collect()
    }

    #[test]
    fn dead_functions_are_found_but_callbacks_are_not() {
        let r = run(
            Chip::Esp32S3,
            "void helper() {}\nvoid unused() { helper(); }\nvoid onEvent() {}\nvoid cb() {}\nvoid (*table[])() = { cb };\nvoid setup() { WiFi.onEvent(onEvent); }\nvoid loop() {}\n",
        );
        let dead: Vec<&str> = r
            .diagnostics
            .iter()
            .filter(|d| d.id.contains("unreachable"))
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            dead,
            [
                "„helper“ wird nirgends aufgerufen.",
                "„unused“ wird nirgends aufgerufen."
            ]
        );
        let unused = r.facts.iter().find(|f| f.name == "unused").unwrap();
        assert!(unused.contexts.contains(&"unreachable".to_string()));
        assert!(r
            .facts
            .iter()
            .find(|f| f.name == "onEvent")
            .unwrap()
            .contexts
            .contains(&"callback".to_string()));
    }

    #[test]
    fn interrupt_handlers_need_iram_volatile_and_no_blocking_calls() {
        let src = "int counter = 0;\nvoid onPress() { counter++; Serial.println(\"x\"); delay(5); }\nvoid setup() { attachInterrupt(digitalPinToInterrupt(4), onPress, FALLING); }\nvoid loop() { }\n";
        let r = run(Chip::Esp32S3, src);
        let found = rules(&r);
        assert!(found.contains(&"iram".to_string()), "{found:?}");
        assert!(found.contains(&"volatile".to_string()), "{found:?}");
        assert_eq!(
            found.iter().filter(|x| *x == "isr-call").count(),
            2,
            "{found:?}"
        );
        let iram = r
            .diagnostics
            .iter()
            .find(|d| d.id.contains("iram"))
            .unwrap();
        assert_eq!(iram.fix.as_ref().unwrap().edits[0].text, "IRAM_ATTR ");
        let vol = r
            .diagnostics
            .iter()
            .find(|d| d.id.contains("volatile"))
            .unwrap();
        let e = &vol.fix.as_ref().unwrap().edits[0];
        assert_eq!(
            (e.from_line, e.from_col, e.text.as_str()),
            (1, 0, "volatile ")
        );
    }

    #[test]
    fn a_correct_isr_is_quiet() {
        let src = "volatile int counter = 0;\nvoid IRAM_ATTR onPress() { counter++; }\nvoid setup() { attachInterrupt(4, onPress, FALLING); }\nvoid loop() { }\n";
        assert_eq!(rules(&run(Chip::Esp32S3, src)), Vec::<String>::new());
    }

    #[test]
    fn unchecked_results() {
        let r = run(
            Chip::Esp32,
            "void setup() { SPIFFS.begin(); if (!LittleFS.begin()) return; }\nvoid loop() {}\n",
        );
        let u: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.id.contains("unchecked"))
            .collect();
        assert_eq!(u.len(), 1);
        assert!(u[0].message.contains("SPIFFS.begin"));
    }

    #[test]
    fn pin_rules_depend_on_the_chip() {
        let src = "#include <WiFi.h>\n#define LED 6\n#define IN_ONLY 34\n#define ADC 12\nvoid setup() { pinMode(LED, OUTPUT); pinMode(IN_ONLY, OUTPUT); int v = analogRead(ADC); }\nvoid loop() {}\n";
        let esp32 = run(Chip::Esp32, src);
        let msgs: Vec<&str> = esp32
            .diagnostics
            .iter()
            .filter(|d| d.id.contains(":pin:"))
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(msgs.len(), 3, "{msgs:?}");
        assert!(
            msgs[0].contains("Flash")
                && msgs[1].contains("nur ein Eingang")
                && msgs[2].contains("ADC2")
        );
        // Beim S3 sind 6, 34 und 12 anders belegt.
        let s3 = run(Chip::Esp32S3, src);
        let s3_msgs: Vec<&str> = s3
            .diagnostics
            .iter()
            .filter(|d| d.id.contains(":pin:"))
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(s3_msgs.len(), 2, "{s3_msgs:?}");
        assert!(s3_msgs
            .iter()
            .any(|m| m.contains("GPIO 34") && m.contains("PSRAM")));
        assert!(s3_msgs.iter().any(|m| m.contains("ADC2")));
    }

    #[test]
    fn switch_over_an_enum_must_cover_it() {
        let src = "enum Mode { OFF, ON, AUTO };\nMode mode;\nvoid setup() { switch (mode) { case OFF: break; case ON: break; } }\nvoid loop() { switch (mode) { case OFF: break; default: break; } switch (mode) { case OFF: case ON: case AUTO: break; } }\n";
        let r = run(Chip::Esp32, src);
        let sw: Vec<_> = r
            .diagnostics
            .iter()
            .filter(|d| d.id.contains("switch"))
            .collect();
        assert_eq!(sw.len(), 1);
        assert!(sw[0].message.contains("AUTO"), "{}", sw[0].message);
        assert_eq!(sw[0].line, 3);
    }

    #[test]
    fn chip_from_fqbn() {
        assert_eq!(
            Chip::from_fqbn("esp32:esp32:esp32s3:FlashSize=16M"),
            Chip::Esp32S3
        );
        assert_eq!(Chip::from_fqbn("esp32:esp32:esp32"), Chip::Esp32);
        assert_eq!(Chip::from_fqbn("esp32:esp32:XIAO_ESP32S3"), Chip::Esp32S3);
        assert_eq!(Chip::from_fqbn("arduino:avr:uno"), Chip::Other);
    }
}
