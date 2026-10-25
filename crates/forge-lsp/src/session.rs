use crate::map::LineMap;
use crate::rpc::Client;
use crate::Result;
use forge_analysis::{combine, SketchSource};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::Mutex;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const START_TIMEOUT: Duration = Duration::from_secs(25);
const MAX_OPEN_DOCS: usize = 40;
const CODE_EXT: &[&str] = &["cpp", "cc", "c", "h", "hpp", "hh"];

/// Flags, die das Board für den Sketch benutzt (aus der Compile-Datenbank von arduino-cli).
#[derive(Debug, Clone)]
pub struct Template {
    pub directory: String,
    pub arguments: Vec<String>,
    pub template_file: String,
    /// Der Spiegelordner des Checkers. Seine Pfade ersetzen wir durch den Spiegel von clangd.
    pub shadow: PathBuf,
    pub main_name: String,
}

#[derive(Debug, Clone)]
pub struct StartOptions {
    pub clangd: PathBuf,
    pub work: PathBuf,
    pub root: PathBuf,
    pub template: Template,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    pub label: String,
    pub kind: u32,
    pub detail: Option<String>,
    pub insert: String,
    /// `insert` ist ein Snippet mit Platzhaltern (`${1:name}`).
    pub snippet: bool,
    /// Spalte, ab der der Text ersetzt wird.
    pub from: Option<u32>,
    pub doc: Option<String>,
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hover {
    pub text: String,
    pub line: u32,
    pub from: u32,
    pub to: u32,
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignatureHelp {
    pub label: String,
    /// Start und Ende jedes Parameters im Label.
    pub params: Vec<(u32, u32)>,
    pub active: u32,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    /// Ab 1.
    pub line: u32,
    pub col: u32,
    pub len: u32,
    pub kind: u32,
    pub mods: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTokens {
    pub types: Vec<String>,
    pub modifiers: Vec<String>,
    pub tokens: Vec<Token>,
}

#[derive(Default)]
struct State {
    /// Spiegelpfad zu (Version, Text) der geöffneten Dokumente.
    docs: BTreeMap<PathBuf, (i32, String)>,
    map: LineMap,
}

pub struct Lsp {
    client: Client,
    mirror: PathBuf,
    root: PathBuf,
    virt: PathBuf,
    template: Template,
    work: PathBuf,
    types: Vec<String>,
    modifiers: Vec<String>,
    state: Mutex<State>,
}

fn uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for b in path.to_string_lossy().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn from_uri(u: &str) -> Option<PathBuf> {
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

fn is_ino(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino"))
}

fn is_code(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| CODE_EXT.contains(&e.to_string_lossy().to_lowercase().as_str()))
}

fn text_of(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("value").and_then(Value::as_str).map(str::to_string),
        Value::Array(a) => {
            let parts: Vec<String> = a.iter().filter_map(text_of).collect();
            (!parts.is_empty()).then(|| parts.join("\n\n"))
        }
        _ => None,
    }
}

fn write_if_changed(path: &Path, text: &str) -> std::io::Result<bool> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(text) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, text)?;
    Ok(true)
}

impl Lsp {
    pub async fn start(opts: StartOptions) -> Result<Self> {
        // clangd meldet Pfade ohne Symlinks (`/private/var/...`). Nur so stimmen sie mit unseren überein.
        std::fs::create_dir_all(opts.work.join("src"))?;
        let work = opts.work.canonicalize()?;
        let mirror = work.join("src");
        let virt = mirror.join(format!("{}.ino.cpp", opts.template.main_name));
        std::fs::write(
            work.join(".clangd"),
            "CompileFlags:\n  Remove: [-mlongcalls, -mtext-section-literals, -mdisable-hardware-atomics, -fstrict-volatile-bitfields, -fno-tree-switch-conversion, -fno-shrink-wrap, -freorder-blocks, -fno-jump-tables, -mfix-esp32-psram-cache-issue, -mfix-esp32-psram-cache-strategy=*, -fno-lto, -ffat-lto-objects, -fno-shrink-wrap, -mno-target-align, -mabi=*, -mcpu=*]\n",
        )?;
        let program = opts.template.arguments.first().cloned().unwrap_or_default();
        let args = vec![
            format!("--compile-commands-dir={}", work.display()),
            "--background-index=false".into(),
            "--header-insertion=never".into(),
            "--completion-style=detailed".into(),
            "--function-arg-placeholders=true".into(),
            "--limit-results=120".into(),
            "--pch-storage=memory".into(),
            "--enable-config".into(),
            "--log=error".into(),
            "-j=2".into(),
            format!("--query-driver={program}"),
        ];
        let client = Client::spawn(&opts.clangd, &args)?;
        let init = client
            .request(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": uri(&work),
                    "capabilities": {
                        "general": { "positionEncodings": ["utf-16"] },
                        "textDocument": {
                            "synchronization": { "didSave": false },
                            "completion": { "completionItem": { "snippetSupport": true, "documentationFormat": ["plaintext"], "labelDetailsSupport": true } },
                            "hover": { "contentFormat": ["plaintext"] },
                            "definition": { "linkSupport": false },
                            "signatureHelp": { "signatureInformation": { "documentationFormat": ["plaintext"], "parameterInformation": { "labelOffsetSupport": true } } },
                            "semanticTokens": { "requests": { "full": true }, "tokenTypes": [], "tokenModifiers": [], "formats": ["relative"] }
                        }
                    },
                    "workspaceFolders": [{ "uri": uri(&work), "name": "forge" }]
                }),
                START_TIMEOUT,
            )
            .await?;
        client.notify("initialized", json!({}));
        let legend = &init["capabilities"]["semanticTokensProvider"]["legend"];
        let strings = |v: &Value| -> Vec<String> {
            v.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        Ok(Self {
            client,
            mirror,
            root: opts.root,
            virt,
            work,
            template: opts.template,
            types: strings(&legend["tokenTypes"]),
            modifiers: strings(&legend["tokenModifiers"]),
            state: Mutex::new(State::default()),
        })
    }

    fn mirror_path(&self, file: &Path) -> Option<PathBuf> {
        if is_ino(file) {
            return Some(self.virt.clone());
        }
        file.strip_prefix(&self.root)
            .ok()
            .map(|rel| self.mirror.join(rel))
    }

    fn args_for(&self, file: &Path) -> Vec<String> {
        let t = &self.template;
        let shadow = t.shadow.to_string_lossy().into_owned();
        let mirror = self.mirror.to_string_lossy().into_owned();
        let mut out = Vec::new();
        let mut skip = false;
        for a in &t.arguments {
            if skip {
                skip = false;
                continue;
            }
            match a.as_str() {
                "-o" => skip = true,
                "-c" | "-MMD" | "-w" => {}
                _ if *a == t.template_file => out.push(file.to_string_lossy().into_owned()),
                _ => out.push(a.replace(&shadow, &mirror)),
            }
        }
        out
    }

    fn write_db(&self, files: &BTreeSet<PathBuf>) -> std::io::Result<bool> {
        let mut entries = vec![
            json!({ "directory": self.template.directory, "arguments": self.args_for(&self.virt), "file": self.virt }),
        ];
        for f in files {
            if is_code(f) {
                entries.push(json!({ "directory": self.template.directory, "arguments": self.args_for(f), "file": f }));
            }
        }
        let text = serde_json::to_string(&entries).unwrap_or_default();
        write_if_changed(&self.work.join("compile_commands.json"), &text)
    }

    /// Bringt Spiegel und clangd auf den Stand des Editors.
    pub async fn sync(&self, texts: &[(PathBuf, String)]) -> Result<()> {
        // Spiegel auf der Platte: clangd liest Header, die nicht geöffnet sind, von dort.
        let mut wanted: BTreeSet<PathBuf> = BTreeSet::new();
        let mut inos: Vec<&(PathBuf, String)> = texts
            .iter()
            .filter(|(p, _)| is_ino(p) && p.parent() == Some(self.root.as_path()))
            .collect();
        let main = &self.template.main_name;
        inos.sort_by(|a, b| {
            let am =
                a.0.file_stem()
                    .is_some_and(|s| s.to_string_lossy() == *main);
            let bm =
                b.0.file_stem()
                    .is_some_and(|s| s.to_string_lossy() == *main);
            bm.cmp(&am).then_with(|| a.0.cmp(&b.0))
        });
        let mut docs: Vec<(PathBuf, String)> = Vec::new();
        for (p, t) in texts.iter().filter(|(p, _)| is_code(p)) {
            if let Some(m) = self.mirror_path(p) {
                write_if_changed(&m, t)?;
                wanted.insert(m.clone());
                docs.push((m, t.clone()));
            }
        }
        let combined = combine(
            &inos
                .iter()
                .map(|(p, t)| SketchSource {
                    path: p.to_string_lossy().into_owned(),
                    text: t.clone(),
                })
                .collect::<Vec<_>>(),
        );
        if !inos.is_empty() {
            write_if_changed(&self.virt, &combined.text)?;
            docs.insert(0, (self.virt.clone(), combined.text.clone()));
        }
        let db_changed = self.write_db(&wanted)?;
        // Entfernte Dateien auch im Spiegel entfernen.
        prune(&self.mirror, &wanted, &self.virt);

        let mut st = self.state.lock().await;
        if inos.is_empty() {
            st.map = LineMap::default();
        } else {
            st.map = LineMap::parse(&combined.text);
        }
        let closed: Vec<PathBuf> = st
            .docs
            .keys()
            .filter(|k| !docs.iter().any(|(p, _)| p == *k))
            .cloned()
            .collect();
        for k in closed {
            st.docs.remove(&k);
            self.client.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": uri(&k) } }),
            );
        }
        for (path, text) in docs.into_iter().take(MAX_OPEN_DOCS) {
            match st.docs.get_mut(&path) {
                None => {
                    self.client.notify(
                        "textDocument/didOpen",
                        json!({ "textDocument": { "uri": uri(&path), "languageId": "cpp", "version": 1, "text": text } }),
                    );
                    st.docs.insert(path, (1, text));
                }
                Some((version, old)) if *old != text || db_changed => {
                    *version += 1;
                    self.client.notify(
                        "textDocument/didChange",
                        json!({ "textDocument": { "uri": uri(&path), "version": *version }, "contentChanges": [{ "text": text }] }),
                    );
                    *old = text;
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    /// Position im Original zu Dokument und Position bei clangd.
    async fn locate(&self, file: &str, line: u32, col: u32) -> Option<(PathBuf, u32, u32)> {
        let path = PathBuf::from(file);
        let mirror = self.mirror_path(&path)?;
        if is_ino(&path) {
            let st = self.state.lock().await;
            let v = st.map.to_virtual(file, line)?;
            return Some((mirror, v, col));
        }
        Some((mirror, line.saturating_sub(1), col))
    }

    /// Ort bei clangd zurück ins Projekt.
    async fn to_location(&self, loc: &Value) -> Option<Location> {
        let target = from_uri(loc["uri"].as_str()?.trim())?;
        let range = &loc["range"];
        let (sl, sc) = (
            range["start"]["line"].as_u64()? as u32,
            range["start"]["character"].as_u64()? as u32,
        );
        let (el, ec) = (
            range["end"]["line"].as_u64()? as u32,
            range["end"]["character"].as_u64()? as u32,
        );
        if target == self.virt {
            let st = self.state.lock().await;
            let (file, line) = st.map.to_original(sl)?;
            let (_, end_line) = st.map.to_original(el).unwrap_or((file, line));
            return Some(Location {
                file: file.to_string(),
                line,
                col: sc,
                end_line,
                end_col: ec,
                external: false,
            });
        }
        if let Ok(rel) = target.strip_prefix(&self.mirror) {
            return Some(Location {
                file: self.root.join(rel).to_string_lossy().into_owned(),
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

    fn pos_params(doc: &Path, line: u32, col: u32) -> Value {
        json!({ "textDocument": { "uri": uri(doc) }, "position": { "line": line, "character": col } })
    }

    pub async fn complete(&self, file: &str, line: u32, col: u32) -> Result<Vec<CompletionItem>> {
        let Some((doc, l, c)) = self.locate(file, line, col).await else {
            return Ok(vec![]);
        };
        let res = self
            .client
            .request(
                "textDocument/completion",
                Self::pos_params(&doc, l, c),
                REQUEST_TIMEOUT,
            )
            .await?;
        let items = res.get("items").cloned().unwrap_or(res);
        let Some(items) = items.as_array() else {
            return Ok(vec![]);
        };
        Ok(items
            .iter()
            .filter_map(|i| {
                let label = i["label"].as_str()?.trim().to_string();
                let detail = [i["labelDetails"]["detail"].as_str(), i["detail"].as_str()]
                    .into_iter()
                    .flatten()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                let insert = i["textEdit"]["newText"]
                    .as_str()
                    .or(i["insertText"].as_str())
                    .unwrap_or(&label)
                    .to_string();
                Some(CompletionItem {
                    kind: i["kind"].as_u64().unwrap_or(0) as u32,
                    detail: (!detail.is_empty()).then_some(detail),
                    from: i["textEdit"]["range"]["start"]["character"]
                        .as_u64()
                        .map(|v| v as u32),
                    doc: text_of(&i["documentation"]),
                    sort: i["sortText"].as_str().map(str::to_string),
                    snippet: i["insertTextFormat"].as_u64() == Some(2),
                    insert,
                    label,
                })
            })
            .collect())
    }

    pub async fn hover(&self, file: &str, line: u32, col: u32) -> Result<Option<Hover>> {
        let Some((doc, l, c)) = self.locate(file, line, col).await else {
            return Ok(None);
        };
        let res = self
            .client
            .request(
                "textDocument/hover",
                Self::pos_params(&doc, l, c),
                REQUEST_TIMEOUT,
            )
            .await?;
        let Some(text) = text_of(&res["contents"]) else {
            return Ok(None);
        };
        let r = &res["range"];
        Ok(Some(Hover {
            text: text.trim().to_string(),
            line,
            from: r["start"]["character"].as_u64().unwrap_or(col as u64) as u32,
            to: r["end"]["character"].as_u64().unwrap_or(col as u64) as u32,
        }))
    }

    pub async fn definition(&self, file: &str, line: u32, col: u32) -> Result<Vec<Location>> {
        let Some((doc, l, c)) = self.locate(file, line, col).await else {
            return Ok(vec![]);
        };
        let res = self
            .client
            .request(
                "textDocument/definition",
                Self::pos_params(&doc, l, c),
                REQUEST_TIMEOUT,
            )
            .await?;
        let list: Vec<Value> = match res {
            Value::Array(a) => a,
            Value::Null => vec![],
            single => vec![single],
        };
        let mut out = Vec::new();
        for item in &list {
            if let Some(l) = self.to_location(item).await {
                if !out.contains(&l) {
                    out.push(l);
                }
            }
        }
        Ok(out)
    }

    pub async fn signature(
        &self,
        file: &str,
        line: u32,
        col: u32,
    ) -> Result<Option<SignatureHelp>> {
        let Some((doc, l, c)) = self.locate(file, line, col).await else {
            return Ok(None);
        };
        let res = self
            .client
            .request(
                "textDocument/signatureHelp",
                Self::pos_params(&doc, l, c),
                REQUEST_TIMEOUT,
            )
            .await?;
        let sigs = res["signatures"].as_array().cloned().unwrap_or_default();
        if sigs.is_empty() {
            return Ok(None);
        }
        let idx = res["activeSignature"].as_u64().unwrap_or(0) as usize;
        let sig = sigs.get(idx).unwrap_or(&sigs[0]);
        let label = sig["label"].as_str().unwrap_or("").to_string();
        let params = sig["parameters"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| match &p["label"] {
                        Value::Array(r) => {
                            Some((r.first()?.as_u64()? as u32, r.get(1)?.as_u64()? as u32))
                        }
                        Value::String(s) => label.find(s.as_str()).map(|i| {
                            (
                                label[..i].encode_utf16().count() as u32,
                                (label[..i].encode_utf16().count() + s.encode_utf16().count())
                                    as u32,
                            )
                        }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(Some(SignatureHelp {
            label,
            params,
            active: sig["activeParameter"]
                .as_u64()
                .or(res["activeParameter"].as_u64())
                .unwrap_or(0) as u32,
            doc: text_of(&sig["documentation"]),
        }))
    }

    pub async fn tokens(&self, file: &str) -> Result<SemanticTokens> {
        let path = PathBuf::from(file);
        let Some(doc) = self.mirror_path(&path) else {
            return Ok(self.empty_tokens());
        };
        let res = self
            .client
            .request(
                "textDocument/semanticTokens/full",
                json!({ "textDocument": { "uri": uri(&doc) } }),
                REQUEST_TIMEOUT,
            )
            .await?;
        let data: Vec<u64> = res["data"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_u64).collect())
            .unwrap_or_default();
        let st = self.state.lock().await;
        let mut tokens = Vec::new();
        let (mut line, mut col) = (0u32, 0u32);
        for c in data.as_chunks::<5>().0 {
            line += c[0] as u32;
            col = if c[0] == 0 {
                col + c[1] as u32
            } else {
                c[1] as u32
            };
            let original = if is_ino(&path) {
                match st.map.to_original(line) {
                    Some((f, l)) if f == file => Some(l),
                    _ => None,
                }
            } else {
                Some(line + 1)
            };
            if let Some(l) = original {
                tokens.push(Token {
                    line: l,
                    col,
                    len: c[2] as u32,
                    kind: c[3] as u32,
                    mods: c[4] as u32,
                });
            }
        }
        Ok(SemanticTokens {
            types: self.types.clone(),
            modifiers: self.modifiers.clone(),
            tokens,
        })
    }

    fn empty_tokens(&self) -> SemanticTokens {
        SemanticTokens {
            types: self.types.clone(),
            modifiers: self.modifiers.clone(),
            tokens: vec![],
        }
    }

    pub fn is_for(&self, root: &Path) -> bool {
        self.root == root
    }
}

fn prune(dir: &Path, keep: &BTreeSet<PathBuf>, virt: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            prune(&p, keep, virt);
        } else if p != virt && !keep.contains(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_roundtrip_with_spaces_and_umlauts() {
        let p = PathBuf::from("/Users/me/Mein Projekt/größe.ino");
        let u = uri(&p);
        assert_eq!(u, "file:///Users/me/Mein%20Projekt/gr%C3%B6%C3%9Fe.ino");
        assert_eq!(from_uri(&u).unwrap(), p);
    }

    #[test]
    fn text_of_handles_markup_and_lists() {
        assert_eq!(
            text_of(&json!({"kind":"plaintext","value":"hallo"})).unwrap(),
            "hallo"
        );
        assert_eq!(text_of(&json!(["a", {"value":"b"}])).unwrap(), "a\n\nb");
        assert!(text_of(&json!(null)).is_none());
    }
}
