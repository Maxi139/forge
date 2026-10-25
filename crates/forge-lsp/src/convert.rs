//! Antworten von clangd (Orte, Gliederung, Umbenennen) zurück auf die Originaldateien des Projekts.
//! Alles hier ist reine Umrechnung ohne Sprachserver, damit es sich ohne clangd testen lässt.

use crate::map::LineMap;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Wo clangd die Dateien des Projekts sieht.
pub struct Ctx<'a> {
    pub map: &'a LineMap,
    /// Der zusammengesetzte Sketch (alle `.ino`).
    pub virt: &'a Path,
    /// Spiegel der übrigen Quelldateien.
    pub mirror: &'a Path,
    pub root: &'a Path,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: String,
    /// Ab 1.
    pub line: u32,
    /// Ab 0, UTF-16.
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    /// Außerhalb des Projekts (Bibliothek oder Core), nur zum Lesen.
    pub external: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Symbol {
    pub name: String,
    pub detail: Option<String>,
    pub kind: u32,
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    /// Tiefe in der Gliederung, 0 für die oberste Ebene.
    pub depth: u32,
    pub container: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RenameTarget {
    pub line: u32,
    pub col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub name: String,
}

/// Eine Änderung im Format der Oberfläche (Zeilen ab 1, Spalten ab 0).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub from_line: u32,
    pub from_col: u32,
    pub to_line: u32,
    pub to_col: u32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEdits {
    pub file: String,
    pub edits: Vec<TextEdit>,
}

pub(crate) fn from_uri(u: &str) -> Option<PathBuf> {
    let rest = u.strip_prefix("file://")?;
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&rest[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    Some(PathBuf::from(String::from_utf8_lossy(&out).into_owned()))
}

fn num(v: &Value) -> Option<u32> {
    v.as_u64().map(|n| n as u32)
}

/// (Anfangszeile, Anfangsspalte, Endzeile, Endspalte) eines LSP-Bereichs, alles ab 0.
fn range(r: &Value) -> Option<(u32, u32, u32, u32)> {
    Some((
        num(&r["start"]["line"])?,
        num(&r["start"]["character"])?,
        num(&r["end"]["line"])?,
        num(&r["end"]["character"])?,
    ))
}

/// Bereich in `target` (Pfad bei clangd) zurück ins Projekt.
pub fn locate_back(cx: &Ctx, target: &Path, r: &Value) -> Option<Location> {
    let (sl, sc, el, ec) = range(r)?;
    if target == cx.virt {
        let (file, line) = cx.map.to_original(sl)?;
        let (_, end_line) = cx.map.to_original(el).unwrap_or((file, line));
        return Some(Location {
            file: file.to_string(),
            line,
            col: sc,
            end_line,
            end_col: ec,
            external: false,
        });
    }
    if let Ok(rel) = target.strip_prefix(cx.mirror) {
        return Some(Location {
            file: cx.root.join(rel).to_string_lossy().into_owned(),
            line: sl + 1,
            col: sc,
            end_line: el + 1,
            end_col: ec,
            external: false,
        });
    }
    Some(Location {
        file: target.to_string_lossy().into_owned(),
        line: sl + 1,
        col: sc,
        end_line: el + 1,
        end_col: ec,
        external: true,
    })
}

/// `Location` (mit `uri`) oder `LocationLink` (mit `targetUri`) von clangd.
pub fn location_of(cx: &Ctx, loc: &Value) -> Option<Location> {
    let uri = loc["uri"].as_str().or(loc["targetUri"].as_str())?.trim();
    let r = if loc["range"].is_object() {
        &loc["range"]
    } else {
        &loc["targetSelectionRange"]
    };
    locate_back(cx, &from_uri(uri)?, r)
}

/// Gliederung eines Dokuments. clangd liefert `DocumentSymbol` (verschachtelt) oder `SymbolInformation` (flach).
/// `doc` ist das Dokument bei clangd, `file` die Originaldatei, deren Gliederung gefragt ist.
pub fn symbols(cx: &Ctx, doc: &Path, file: &str, res: &Value) -> Vec<Symbol> {
    let mut out = Vec::new();
    let Some(list) = res.as_array() else {
        return out;
    };
    for item in list {
        if item["location"].is_object() {
            // SymbolInformation
            let Some(loc) = locate_back(cx, doc, &item["location"]["range"]) else {
                continue;
            };
            if loc.file != file {
                continue;
            }
            out.push(Symbol {
                name: item["name"].as_str().unwrap_or("").to_string(),
                detail: None,
                kind: num(&item["kind"]).unwrap_or(0),
                line: loc.line,
                col: loc.col,
                end_line: loc.end_line,
                end_col: loc.end_col,
                depth: 0,
                container: item["containerName"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
            });
        } else {
            walk_symbols(cx, doc, file, item, 0, None, &mut out);
        }
    }
    out
}

fn walk_symbols(
    cx: &Ctx,
    doc: &Path,
    file: &str,
    item: &Value,
    depth: u32,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let name = item["name"].as_str().unwrap_or("");
    let sel = if item["selectionRange"].is_object() {
        &item["selectionRange"]
    } else {
        &item["range"]
    };
    // Erzeugte Zeilen (Prototypen) und fremde Dateien gehören nicht in die Gliederung, ihre Kinder schon.
    let mapped = locate_back(cx, doc, sel).filter(|l| l.file == file);
    let mut next_depth = depth;
    if let Some(loc) = mapped {
        let (end_line, end_col) = match range(&item["range"]).and_then(|(_, _, el, ec)| {
            locate_back(
                cx,
                doc,
                &serde_json::json!({
                    "start": { "line": el, "character": ec },
                    "end": { "line": el, "character": ec }
                }),
            )
            .filter(|e| e.file == file)
            .map(|e| (e.line, ec))
        }) {
            Some(v) => v,
            None => (loc.end_line, loc.end_col),
        };
        out.push(Symbol {
            name: name.to_string(),
            detail: item["detail"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            kind: num(&item["kind"]).unwrap_or(0),
            line: loc.line,
            col: loc.col,
            end_line,
            end_col,
            depth,
            container: container.map(str::to_string),
        });
        next_depth += 1;
    }
    if let Some(children) = item["children"].as_array() {
        for c in children {
            walk_symbols(
                cx,
                doc,
                file,
                c,
                next_depth,
                if next_depth > depth {
                    Some(name)
                } else {
                    container
                },
                out,
            );
        }
    }
}

/// Antwort auf `prepareRename`: ein Bereich, ein Bereich mit Platzhalter oder `defaultBehavior`.
/// `text` ist der Text des Bereichs, falls clangd keinen Platzhalter nennt.
pub fn rename_target(
    cx: &Ctx,
    doc: &Path,
    res: &Value,
    text_at: impl Fn(&Location) -> Option<String>,
) -> Option<RenameTarget> {
    let r = if res["range"].is_object() {
        &res["range"]
    } else if res["start"].is_object() {
        res
    } else {
        return None;
    };
    let loc = locate_back(cx, doc, r)?;
    if loc.external {
        return None;
    }
    let name = res["placeholder"]
        .as_str()
        .map(str::to_string)
        .or_else(|| text_at(&loc))?;
    Some(RenameTarget {
        line: loc.line,
        col: loc.col,
        end_line: loc.end_line,
        end_col: loc.end_col,
        name,
    })
}

/// Ergebnis von [`workspace_edit`].
#[derive(Debug, PartialEq, Eq)]
pub enum Renamed {
    Files(Vec<FileEdits>),
    /// Das Symbol kommt auch in Core oder Bibliotheken vor. Halb umbenannt würde der Build brechen.
    TouchesExternal(String),
}

fn text_edits(cx: &Ctx, uri: &str, edits: &Value, files: &mut Vec<FileEdits>) -> Option<String> {
    let target = from_uri(uri)?;
    let list = edits.as_array()?;
    for e in list {
        let Some(loc) = locate_back(cx, &target, &e["range"]) else {
            continue;
        };
        if loc.external {
            return Some(loc.file);
        }
        // Prototypen und erzeugte Zeilen: die echte Stelle kommt ohnehin separat.
        let edit = TextEdit {
            from_line: loc.line,
            from_col: loc.col,
            to_line: loc.end_line,
            to_col: loc.end_col,
            text: e["newText"].as_str().unwrap_or("").to_string(),
        };
        let slot = match files.iter().position(|f| f.file == loc.file) {
            Some(i) => i,
            None => {
                files.push(FileEdits {
                    file: loc.file.clone(),
                    edits: vec![],
                });
                files.len() - 1
            }
        };
        if !files[slot].edits.contains(&edit) {
            files[slot].edits.push(edit);
        }
    }
    None
}

/// `WorkspaceEdit` von clangd (`changes` oder `documentChanges`) auf Projektdateien abbilden.
pub fn workspace_edit(cx: &Ctx, res: &Value) -> Renamed {
    let mut files: Vec<FileEdits> = Vec::new();
    if let Some(changes) = res["changes"].as_object() {
        for (uri, edits) in changes {
            if let Some(ext) = text_edits(cx, uri, edits, &mut files) {
                return Renamed::TouchesExternal(ext);
            }
        }
    }
    if let Some(docs) = res["documentChanges"].as_array() {
        for d in docs {
            // Datei anlegen, umbenennen oder löschen kommt beim Umbenennen eines Symbols nicht vor.
            let Some(uri) = d["textDocument"]["uri"].as_str() else {
                continue;
            };
            if let Some(ext) = text_edits(cx, uri, &d["edits"], &mut files) {
                return Renamed::TouchesExternal(ext);
            }
        }
    }
    files.retain(|f| !f.edits.is_empty());
    Renamed::Files(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const COMBINED: &str = "#include <Arduino.h>\n#line 1 \"/p/blink.ino\"\nint z = 0;\n\n#line 3 \"/p/blink.ino\"\nvoid setup();\n#line 8 \"/p/blink.ino\"\nvoid loop();\n#line 3 \"/p/blink.ino\"\nvoid setup() {\n  z++;\n}\n#line 1 \"/p/extra.ino\"\nvoid helper() {}\n";

    fn cx<'a>(map: &'a LineMap, virt: &'a Path, mirror: &'a Path, root: &'a Path) -> Ctx<'a> {
        Ctx {
            map,
            virt,
            mirror,
            root,
        }
    }

    fn r(sl: u32, sc: u32, el: u32, ec: u32) -> Value {
        json!({ "start": { "line": sl, "character": sc }, "end": { "line": el, "character": ec } })
    }

    #[test]
    fn locations_map_to_sketch_mirror_and_external() {
        let map = LineMap::parse(COMBINED);
        let (virt, mirror, root) = (
            Path::new("/w/src/blink.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let cx = cx(&map, virt, mirror, root);
        let a = locate_back(&cx, virt, &r(9, 5, 9, 10)).unwrap();
        assert_eq!(
            (a.file.as_str(), a.line, a.col, a.end_col),
            ("/p/blink.ino", 3, 5, 10)
        );
        let b = locate_back(&cx, Path::new("/w/src/lib/util.h"), &r(2, 0, 2, 4)).unwrap();
        assert_eq!(
            (b.file.as_str(), b.line, b.external),
            ("/p/lib/util.h", 3, false)
        );
        let c = locate_back(&cx, Path::new("/core/Print.h"), &r(0, 0, 0, 1)).unwrap();
        assert!(c.external);
        // Der erzeugte Prototyp (virtuelle Zeile 5) ist kein Ort im Projekt.
        assert!(locate_back(&cx, virt, &r(5, 5, 5, 10)).is_none());
    }

    #[test]
    fn document_symbols_are_filtered_to_the_asked_file() {
        let map = LineMap::parse(COMBINED);
        let (virt, mirror, root) = (
            Path::new("/w/src/blink.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let cx = cx(&map, virt, mirror, root);
        let res = json!([
            { "name": "z", "kind": 13, "range": r(2, 0, 2, 10), "selectionRange": r(2, 4, 2, 5) },
            { "name": "setup", "kind": 12, "range": r(5, 0, 5, 13), "selectionRange": r(5, 5, 5, 10) },
            { "name": "setup", "kind": 12, "detail": "void ()", "range": r(9, 0, 11, 1), "selectionRange": r(9, 5, 9, 10),
              "children": [ { "name": "tmp", "kind": 13, "range": r(10, 2, 10, 6), "selectionRange": r(10, 2, 10, 5) } ] },
            { "name": "helper", "kind": 12, "range": r(13, 0, 13, 16), "selectionRange": r(13, 5, 13, 11) }
        ]);
        let blink = symbols(&cx, virt, "/p/blink.ino", &res);
        let names: Vec<_> = blink
            .iter()
            .map(|s| (s.name.as_str(), s.line, s.depth))
            .collect();
        // Der Prototyp fällt weg, `helper` gehört zur anderen Datei.
        assert_eq!(names, vec![("z", 1, 0), ("setup", 3, 0), ("tmp", 4, 1)]);
        assert_eq!(blink[1].end_line, 5);
        assert_eq!(blink[2].container.as_deref(), Some("setup"));
        let extra = symbols(&cx, virt, "/p/extra.ino", &res);
        assert_eq!(extra.len(), 1);
        assert_eq!((extra[0].name.as_str(), extra[0].line), ("helper", 1));
    }

    #[test]
    fn flat_symbol_information_is_supported() {
        let map = LineMap::default();
        let cx = cx(
            &map,
            Path::new("/w/src/x.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let res = json!([{ "name": "f", "kind": 12, "containerName": "ns", "location": { "uri": "file:///w/src/a.cpp", "range": r(4, 0, 6, 1) } }]);
        let out = symbols(&cx, Path::new("/w/src/a.cpp"), "/p/a.cpp", &res);
        assert_eq!(
            (out[0].line, out[0].end_line, out[0].container.as_deref()),
            (5, 7, Some("ns"))
        );
    }

    #[test]
    fn rename_edits_stay_in_the_project_and_skip_prototypes() {
        let map = LineMap::parse(COMBINED);
        let (virt, mirror, root) = (
            Path::new("/w/src/blink.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let cx = cx(&map, virt, mirror, root);
        let res = json!({ "changes": {
            "file:///w/src/blink.ino.cpp": [
                { "range": r(5, 5, 5, 10), "newText": "start" },
                { "range": r(9, 5, 9, 10), "newText": "start" }
            ],
            "file:///w/src/lib/util.h": [ { "range": r(0, 5, 0, 10), "newText": "start" } ]
        }});
        let Renamed::Files(files) = workspace_edit(&cx, &res) else {
            panic!("erwartet Projektdateien");
        };
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file, "/p/blink.ino");
        assert_eq!(files[0].edits.len(), 1, "der Prototyp zählt nicht");
        assert_eq!(
            (files[0].edits[0].from_line, files[0].edits[0].from_col),
            (3, 5)
        );
        assert_eq!(files[1].file, "/p/lib/util.h");
        assert_eq!(files[1].edits[0].from_line, 1);
    }

    #[test]
    fn rename_touching_the_core_is_refused() {
        let map = LineMap::default();
        let cx = cx(
            &map,
            Path::new("/w/src/x.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let res = json!({ "documentChanges": [
            { "textDocument": { "uri": "file:///w/src/a.cpp", "version": 1 }, "edits": [ { "range": r(1, 0, 1, 6), "newText": "Port" } ] },
            { "textDocument": { "uri": "file:///core/HardwareSerial.h", "version": 1 }, "edits": [ { "range": r(1, 0, 1, 6), "newText": "Port" } ] }
        ]});
        assert!(
            matches!(workspace_edit(&cx, &res), Renamed::TouchesExternal(f) if f == "/core/HardwareSerial.h")
        );
    }

    #[test]
    fn prepare_rename_accepts_range_placeholder_and_rejects_default() {
        let map = LineMap::parse(COMBINED);
        let (virt, mirror, root) = (
            Path::new("/w/src/blink.ino.cpp"),
            Path::new("/w/src"),
            Path::new("/p"),
        );
        let cx = cx(&map, virt, mirror, root);
        let plain = rename_target(&cx, virt, &r(9, 5, 9, 10), |l| {
            assert_eq!((l.line, l.col), (3, 5));
            Some("setup".into())
        })
        .unwrap();
        assert_eq!(
            (plain.name.as_str(), plain.line, plain.end_col),
            ("setup", 3, 10)
        );
        let with = rename_target(
            &cx,
            virt,
            &json!({ "range": r(9, 5, 9, 10), "placeholder": "setup" }),
            |_| None,
        )
        .unwrap();
        assert_eq!(with.name, "setup");
        assert!(rename_target(&cx, virt, &json!({ "defaultBehavior": true }), |_| None).is_none());
    }
}
