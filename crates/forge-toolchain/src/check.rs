//! Compiler-Prüfung ohne Build. Der Sketch wird wie bei Arduino zusammengesetzt (Prototypen, `#line`-Marken)
//! und mit den echten Compiler-Flags des Boards und `-fsyntax-only` geprüft. Das dauert etwa 0,6 Sekunden
//! und liefert dieselben Fehler wie der Build, samt Fix-its.

use crate::cli::ArduinoCli;
use crate::gcc;
use crate::hash::{short_key, source_files};
use crate::process::{os, stream, Stream};
use crate::translate::translate;
use crate::{CancellationToken, Error, Result};
use forge_analysis::{combine, includes, SketchSource};
use forge_core::{utf16_col, Diagnostic, DiagnosticAction, DiagnosticSource, Severity};
use regex::Regex;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const GCC_TIMEOUT: Duration = Duration::from_secs(40);
const SOURCE_EXT: &[&str] = &["ino", "cpp", "c", "cc", "h", "hpp", "hh", "s"];
const EXTRA_FLAGS: &[&str] = &[
    "-fsyntax-only",
    "-fdiagnostics-format=json",
    "-Wall",
    "-Wextra",
    "-Wno-unused-parameter",
    "-Wno-missing-field-initializers",
    "-Wno-psabi",
];

#[derive(Debug, Clone)]
pub struct CheckRequest {
    pub root: PathBuf,
    /// Board samt Menüoptionen.
    pub fqbn: String,
    /// Ungespeicherter Text aus dem Editor, nach absolutem Pfad.
    pub overrides: HashMap<PathBuf, String>,
    /// Signatur der installierten Bibliotheken. Ändert sie sich, wird die Compile-Datenbank neu erzeugt.
    pub libraries_signature: String,
}

#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    /// `false`, wenn die Prüfung nicht laufen konnte (Core fehlt o. Ä.). Dann sind keine Diagnosen vertrauenswürdig.
    pub ran: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub note: Option<String>,
    pub duration_ms: u64,
    /// Hash über den geprüften Inhalt.
    pub content_hash: String,
}

#[derive(Debug, Deserialize)]
struct DbEntry {
    directory: String,
    file: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    arguments: Option<Vec<String>>,
}

impl DbEntry {
    fn argv(&self) -> Vec<String> {
        self.arguments
            .clone()
            .or_else(|| self.command.as_deref().and_then(shlex::split))
            .unwrap_or_default()
    }
}

/// Was clangd braucht, um den Sketch wie der Compiler zu lesen.
#[derive(Debug, Clone)]
pub struct FlagsTemplate {
    pub directory: String,
    pub arguments: Vec<String>,
    /// Die Datei, auf die sich die Argumente beziehen.
    pub template_file: String,
    /// Der Spiegelordner des Checkers, auf den die Argumente zeigen.
    pub shadow: PathBuf,
    /// Name der Hauptdatei ohne Endung.
    pub main_name: String,
    /// `.cpp`- und `.c`-Dateien des Projekts.
    pub cpp_files: Vec<PathBuf>,
}

pub struct Checker {
    cli: ArduinoCli,
    cache: PathBuf,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    last: Mutex<HashMap<String, CheckResult>>,
}

fn is_cpp(p: &Path) -> bool {
    p.extension().is_some_and(|e| {
        matches!(
            e.to_string_lossy().to_lowercase().as_str(),
            "cpp" | "cc" | "c"
        )
    })
}

fn is_ino(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino"))
}

/// Verzeichnis für Zwischenstände eines Projekts, außerhalb des Projektordners.
pub fn project_cache(base: &Path, root: &Path) -> PathBuf {
    base.join("projects")
        .join(short_key(&[&root.to_string_lossy()]))
}

impl Checker {
    pub fn new(cli: ArduinoCli, cache_base: PathBuf) -> Self {
        Self {
            cli,
            cache: cache_base,
            locks: Mutex::new(HashMap::new()),
            last: Mutex::new(HashMap::new()),
        }
    }

    async fn lock_for(&self, key: &str) -> Arc<Mutex<()>> {
        self.locks
            .lock()
            .await
            .entry(key.to_string())
            .or_default()
            .clone()
    }

    pub async fn check(
        &self,
        req: &CheckRequest,
        cancel: &CancellationToken,
    ) -> Result<CheckResult> {
        let started = Instant::now();
        let root = &req.root;
        let cache = project_cache(&self.cache, root);

        // 1. Quellen und ihr Text (Editor vor Platte).
        let texts = load_texts(root, &req.overrides).await;
        let main_name = main_stem(root, &texts);
        let content_hash = {
            let mut h = Sha256::new();
            h.update(req.fqbn.as_bytes());
            h.update(req.libraries_signature.as_bytes());
            for (p, t) in &texts {
                h.update(p.to_string_lossy().as_bytes());
                h.update(t.as_bytes());
            }
            format!("{:x}", h.finalize())
        };

        // Gleicher Inhalt wie beim letzten Lauf: Ergebnis wiederverwenden.
        let cache_key = short_key(&[&root.to_string_lossy(), &req.fqbn]);
        if let Some(prev) = self.last.lock().await.get(&cache_key) {
            if prev.content_hash == content_hash && prev.ran {
                return Ok(prev.clone());
            }
        }

        // 2. Spiegelordner mit dem Stand des Editors.
        let shadow = cache.join("shadow").join(&main_name);
        sync_shadow(root, &shadow, &texts, &main_name).await?;

        // 3. Compile-Datenbank für diese Include-Menge.
        let include_set: BTreeSet<String> = texts
            .iter()
            .flat_map(|(_, t)| includes(t))
            .map(|i| i.name)
            .collect();
        let db_key = short_key(&[
            &req.fqbn,
            &req.libraries_signature,
            &include_set.iter().cloned().collect::<Vec<_>>().join(","),
            &main_name,
        ]);
        let db_dir = cache.join("check").join(&db_key);
        let lock = self.lock_for(&db_key).await;
        let _guard = lock.lock().await;
        if let Err(stderr) = self.ensure_db(&req.fqbn, &db_dir, &shadow, cancel).await? {
            let result = db_failure(
                &stderr,
                root,
                &texts,
                &shadow,
                started,
                content_hash.clone(),
            );
            self.last.lock().await.insert(cache_key, result.clone());
            return Ok(result);
        }
        drop(_guard);

        // 4. Compiler mit den Flags des Boards.
        let db: Vec<DbEntry> =
            serde_json::from_slice(&tokio::fs::read(db_dir.join("compile_commands.json")).await?)
                .map_err(|e| Error::Parse {
                tool: "arduino-cli",
                detail: e.to_string(),
            })?;

        let mut stderr_all = String::new();
        let ino: Vec<&(PathBuf, String)> = ordered_inos(&texts, &main_name);
        if let Some(template) = db.iter().find(|e| e.file.ends_with(".ino.cpp")) {
            let combined = combine(
                &ino.iter()
                    .map(|(p, t)| SketchSource {
                        path: p.to_string_lossy().into_owned(),
                        text: t.clone(),
                    })
                    .collect::<Vec<_>>(),
            );
            let combined_path = db_dir.join("combined.cpp");
            tokio::fs::write(&combined_path, combined.text).await?;
            stderr_all.push_str(
                &self
                    .run_gcc(template, &combined_path, &shadow, cancel)
                    .await?,
            );
        }
        for (path, _) in texts.iter().filter(|(p, _)| is_cpp(p)) {
            let shadow_file = shadow.join(path.strip_prefix(root).unwrap_or(path));
            if let Some(template) = db.iter().find(|e| Path::new(&e.file) == shadow_file) {
                let out = self
                    .run_gcc(template, &shadow_file, &shadow, cancel)
                    .await?;
                // Der Spiegelordner ist ein Implementierungsdetail: Pfade zurück auf das Projekt.
                stderr_all.push_str(&out.replace(
                    shadow.to_string_lossy().as_ref(),
                    root.to_string_lossy().as_ref(),
                ));
            }
        }

        // 5. Diagnosen.
        let mut diagnostics = Vec::new();
        for chunk in split_json_arrays(&stderr_all) {
            diagnostics.extend(gcc::to_diagnostics(&chunk, root, &mut |file| {
                let p = PathBuf::from(file);
                texts
                    .iter()
                    .find(|(tp, _)| *tp == p)
                    .map(|(_, t)| t.clone())
                    .or_else(|| std::fs::read_to_string(&p).ok())
            }));
        }
        diagnostics.dedup_by(|a, b| a.id == b.id && a.message == b.message);

        let result = CheckResult {
            ran: true,
            diagnostics,
            note: None,
            duration_ms: started.elapsed().as_millis() as u64,
            content_hash,
        };
        self.last.lock().await.insert(cache_key, result.clone());
        Ok(result)
    }

    /// Erzeugt die Compile-Datenbank, falls sie fehlt. `Err(Ausgabe)`, wenn arduino-cli sie nicht liefern konnte.
    async fn ensure_db(
        &self,
        fqbn: &str,
        db_dir: &Path,
        shadow: &Path,
        cancel: &CancellationToken,
    ) -> Result<std::result::Result<(), String>> {
        if db_dir.join("compile_commands.json").is_file() {
            return Ok(Ok(()));
        }
        tokio::fs::create_dir_all(db_dir).await?;
        let mut stderr = Vec::new();
        let args = os(&[
            "compile",
            "-b",
            fqbn,
            "--only-compilation-database",
            "--build-path",
            &db_dir.to_string_lossy(),
            &shadow.to_string_lossy(),
        ]);
        let code = stream(&self.cli.path, &args, None, "C", cancel, |s, l| {
            if s == Stream::Stderr || l.contains("rror") {
                stderr.push(l.to_string());
            }
        })
        .await?;
        if code != Some(0) || !db_dir.join("compile_commands.json").is_file() {
            let _ = tokio::fs::remove_dir_all(db_dir).await;
            return Ok(Err(stderr.join("\n")));
        }
        Ok(Ok(()))
    }

    /// Die Compiler-Aufrufe des Boards für den Sketch, als Vorlage für clangd.
    pub async fn flags_template(
        &self,
        req: &CheckRequest,
        cancel: &CancellationToken,
    ) -> Result<Option<FlagsTemplate>> {
        let root = &req.root;
        let cache = project_cache(&self.cache, root);
        let texts = load_texts(root, &req.overrides).await;
        let main_name = main_stem(root, &texts);
        let shadow = cache.join("shadow").join(&main_name);
        sync_shadow(root, &shadow, &texts, &main_name).await?;
        let include_set: BTreeSet<String> = texts
            .iter()
            .flat_map(|(_, t)| includes(t))
            .map(|i| i.name)
            .collect();
        let db_key = short_key(&[
            &req.fqbn,
            &req.libraries_signature,
            &include_set.iter().cloned().collect::<Vec<_>>().join(","),
            &main_name,
        ]);
        let db_dir = cache.join("check").join(&db_key);
        let lock = self.lock_for(&db_key).await;
        let _guard = lock.lock().await;
        if self
            .ensure_db(&req.fqbn, &db_dir, &shadow, cancel)
            .await?
            .is_err()
        {
            return Ok(None);
        }
        let db: Vec<DbEntry> =
            serde_json::from_slice(&tokio::fs::read(db_dir.join("compile_commands.json")).await?)
                .map_err(|e| Error::Parse {
                tool: "arduino-cli",
                detail: e.to_string(),
            })?;
        let Some(ino) = db.iter().find(|e| e.file.ends_with(".ino.cpp")) else {
            return Ok(None);
        };
        let cpp_files = texts
            .iter()
            .filter(|(p, _)| is_cpp(p))
            .map(|(p, _)| p.clone())
            .collect();
        Ok(Some(FlagsTemplate {
            directory: ino.directory.clone(),
            arguments: ino.argv(),
            template_file: ino.file.clone(),
            shadow: shadow.clone(),
            main_name,
            cpp_files,
        }))
    }

    async fn run_gcc(
        &self,
        template: &DbEntry,
        source: &Path,
        shadow: &Path,
        cancel: &CancellationToken,
    ) -> Result<String> {
        let mut argv = template.argv();
        if argv.is_empty() {
            return Ok(String::new());
        }
        let program = PathBuf::from(argv.remove(0));
        let mut out: Vec<OsString> = Vec::new();
        let mut skip = false;
        for a in argv {
            if skip {
                skip = false;
                continue;
            }
            match a.as_str() {
                "-o" => skip = true,
                "-c" | "-MMD" | "-w" => {}
                _ if a == template.file => out.push(source.into()),
                _ => out.push(a.into()),
            }
        }
        out.extend(EXTRA_FLAGS.iter().map(OsString::from));
        out.push("-iquote".into());
        out.push(shadow.into());

        let mut stderr = String::new();
        let run = stream(
            &program,
            &out,
            Some(Path::new(&template.directory)),
            "C",
            cancel,
            |s, l| {
                if s == Stream::Stderr {
                    stderr.push_str(l);
                    stderr.push('\n');
                }
            },
        );
        match tokio::time::timeout(GCC_TIMEOUT, run).await {
            Ok(r) => {
                r?;
                Ok(stderr)
            }
            Err(_) => Err(Error::Timeout("Compiler".into())),
        }
    }

    /// Gibt zwischengespeicherte Ergebnisse frei, etwa nach Installation einer Bibliothek.
    pub async fn forget(&self) {
        self.last.lock().await.clear();
    }
}

/// Quellen des Projekts samt Text. Der Stand des Editors geht vor dem der Platte.
pub async fn load_texts(
    root: &Path,
    overrides: &HashMap<PathBuf, String>,
) -> Vec<(PathBuf, String)> {
    let mut texts: Vec<(PathBuf, String)> = Vec::new();
    for f in source_files(root) {
        let ext = f
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !SOURCE_EXT.contains(&ext.as_str()) {
            continue;
        }
        let text = match overrides.get(&f) {
            Some(t) => t.clone(),
            None => match tokio::fs::read(&f).await {
                Ok(b) => String::from_utf8_lossy(&b).into_owned(),
                Err(_) => continue,
            },
        };
        texts.push((f, text));
    }
    texts
}

fn main_stem(root: &Path, texts: &[(PathBuf, String)]) -> String {
    let dir_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if texts.iter().any(|(p, _)| {
        p.parent() == Some(root)
            && p.file_name()
                .is_some_and(|n| n.to_string_lossy() == format!("{dir_name}.ino"))
    }) {
        return dir_name;
    }
    let mut inos: Vec<&PathBuf> = texts
        .iter()
        .map(|(p, _)| p)
        .filter(|p| is_ino(p) && p.parent() == Some(root))
        .collect();
    inos.sort();
    inos.first()
        .and_then(|p| p.file_stem())
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or(dir_name)
}

/// Hauptdatei zuerst, dann die übrigen `.ino` alphabetisch (wie arduino-cli).
fn ordered_inos<'a>(texts: &'a [(PathBuf, String)], main: &str) -> Vec<&'a (PathBuf, String)> {
    let mut inos: Vec<&(PathBuf, String)> = texts.iter().filter(|(p, _)| is_ino(p)).collect();
    inos.sort_by(|a, b| {
        let am = a.0.file_stem().is_some_and(|s| s.to_string_lossy() == main);
        let bm = b.0.file_stem().is_some_and(|s| s.to_string_lossy() == main);
        bm.cmp(&am).then_with(|| a.0.cmp(&b.0))
    });
    inos
}

async fn sync_shadow(
    root: &Path,
    shadow: &Path,
    texts: &[(PathBuf, String)],
    main: &str,
) -> Result<()> {
    tokio::fs::create_dir_all(shadow).await?;
    let mut keep: BTreeSet<PathBuf> = BTreeSet::new();
    for (path, text) in texts {
        let rel = path.strip_prefix(root).unwrap_or(path);
        // Die Hauptdatei muss wie der Ordner heißen.
        let rel = if path.parent() == Some(root)
            && is_ino(path)
            && path
                .file_stem()
                .is_some_and(|s| s.to_string_lossy() == main)
        {
            PathBuf::from(format!("{main}.ino"))
        } else {
            rel.to_path_buf()
        };
        let target = shadow.join(&rel);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        if tokio::fs::read_to_string(&target).await.ok().as_deref() != Some(text.as_str()) {
            tokio::fs::write(&target, text).await?;
        }
        keep.insert(target);
    }
    // Entfernte Dateien auch im Spiegel entfernen.
    let mut stack = vec![shadow.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(e)) = rd.next_entry().await {
            let p = e.path();
            if e.file_type().await.map(|t| t.is_dir()).unwrap_or(false) {
                stack.push(p);
            } else if !keep.contains(&p) {
                let _ = tokio::fs::remove_file(&p).await;
            }
        }
    }
    Ok(())
}

/// Der Compiler gibt bei JSON-Ausgabe pro Lauf ein Array aus. Mehrere Läufe liegen hintereinander.
fn split_json_arrays(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' if depth > 0 => in_str = true,
            '[' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            ']' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push(text[s..=i].to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Die Compile-Datenbank ließ sich nicht erzeugen. Häufigster Grund: eine Bibliothek fehlt.
fn db_failure(
    stderr: &str,
    root: &Path,
    texts: &[(PathBuf, String)],
    shadow: &Path,
    started: Instant,
    content_hash: String,
) -> CheckResult {
    let re =
        Regex::new(r"fatal error: ([^\s:]+): No such file or directory").expect("gültiges Muster");
    let mut diagnostics = Vec::new();
    if let Some(cap) = re.captures(stderr) {
        let header = cap[1].to_string();
        'outer: for (path, text) in texts {
            for inc in includes(text) {
                if inc.name == header {
                    let line_text = text.split('\n').nth(inc.line as usize - 1).unwrap_or("");
                    let byte = line_text.find(&header).unwrap_or(0);
                    let from = utf16_col(line_text, byte);
                    let to = utf16_col(line_text, byte + header.len());
                    let stem = header
                        .trim_end_matches(".h")
                        .trim_end_matches(".hpp")
                        .to_string();
                    diagnostics.push(Diagnostic {
                        id: format!("compiler:{}:{}:{}", path.display(), inc.line, from),
                        file: path.to_string_lossy().into_owned(),
                        line: inc.line,
                        from,
                        to: to.max(from + 1),
                        severity: Severity::Error,
                        message: format!(
                            "{} Installiere die passende Bibliothek.",
                            translate(&format!("{header}: No such file or directory"))
                        ),
                        source: DiagnosticSource::Compiler,
                        fix: None,
                        action: Some(DiagnosticAction {
                            kind: "searchLibrary".into(),
                            arg: stem,
                            label: "Bibliothek suchen".into(),
                        }),
                    });
                    break 'outer;
                }
            }
        }
        return CheckResult {
            ran: true,
            diagnostics,
            note: None,
            duration_ms: started.elapsed().as_millis() as u64,
            content_hash,
        };
    }
    let _ = (root, shadow);
    let note = stderr
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|l| l.trim().trim_start_matches("Error: ").to_string());
    CheckResult {
        ran: false,
        diagnostics,
        note,
        duration_ms: started.elapsed().as_millis() as u64,
        content_hash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_multiple_json_arrays_and_ignores_brackets_in_strings() {
        let text = "[{\"message\":\"a ] b\"}]\nrauschen\n[{\"message\":\"c\"}]";
        let parts = split_json_arrays(text);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].contains("a ] b"));
    }

    #[test]
    fn main_file_comes_first() {
        let root = Path::new("/p/wetter");
        let t = vec![
            (PathBuf::from("/p/wetter/a_extra.ino"), String::new()),
            (PathBuf::from("/p/wetter/wetter.ino"), String::new()),
            (PathBuf::from("/p/wetter/b.ino"), String::new()),
        ];
        assert_eq!(main_stem(root, &t), "wetter");
        let order: Vec<_> = ordered_inos(&t, "wetter")
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(order, ["wetter.ino", "a_extra.ino", "b.ino"]);
    }

    #[test]
    fn missing_header_becomes_a_diagnostic_with_library_action() {
        let texts = vec![(
            PathBuf::from("/p/a.ino"),
            "#include <WiFi.h>\n#include <Adafruit_NeoPixel.h>\n".to_string(),
        )];
        let stderr =
            "/shadow/a/a.ino:2:10: fatal error: Adafruit_NeoPixel.h: No such file or directory";
        let r = db_failure(
            stderr,
            Path::new("/p"),
            &texts,
            Path::new("/shadow"),
            Instant::now(),
            String::new(),
        );
        assert!(r.ran);
        let d = &r.diagnostics[0];
        assert_eq!((d.line, d.from), (2, 10));
        assert!(d
            .message
            .starts_with("Datei „Adafruit_NeoPixel.h“ nicht gefunden"));
        assert_eq!(d.action.as_ref().unwrap().arg, "Adafruit_NeoPixel");
    }

    #[test]
    fn other_failures_are_not_trusted() {
        let r = db_failure(
            "Error: platform esp32:esp32 is not installed",
            Path::new("/p"),
            &[],
            Path::new("/s"),
            Instant::now(),
            String::new(),
        );
        assert!(!r.ran);
        assert_eq!(
            r.note.as_deref(),
            Some("platform esp32:esp32 is not installed")
        );
    }
}

#[cfg(test)]
mod integration {
    //! Läuft gegen das installierte arduino-cli und den ESP32-Core. Ohne beides passiert nichts.
    use super::*;
    use crate::tools::Tools;

    #[tokio::test]
    async fn finds_a_missing_semicolon_with_a_fix_and_passes_valid_code() {
        let Ok(tools) = Tools::locate(&[]) else {
            return;
        };
        let cli = ArduinoCli::new(tools.arduino_cli);
        let fqbn = "esp32:esp32:esp32s3";
        if cli.board_options(fqbn).await.is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("probe");
        std::fs::create_dir_all(&root).unwrap();
        let ino = root.join("probe.ino");
        std::fs::write(&ino, "void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(100); }\n").unwrap();

        let checker = Checker::new(cli, tmp.path().join("cache"));
        let cancel = CancellationToken::new();
        let req = |text: Option<&str>| CheckRequest {
            root: root.clone(),
            fqbn: fqbn.into(),
            overrides: text
                .map(|t| HashMap::from([(ino.clone(), t.to_string())]))
                .unwrap_or_default(),
            libraries_signature: String::new(),
        };

        let ok = checker.check(&req(None), &cancel).await.unwrap();
        assert!(ok.ran, "{:?}", ok.note);
        assert!(
            ok.diagnostics.iter().all(|d| d.severity != Severity::Error),
            "{:#?}",
            ok.diagnostics
        );

        // Ungespeicherter Text mit fehlendem Semikolon in Zeile 2.
        let bad = "void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(100) }\n";
        let r = checker.check(&req(Some(bad)), &cancel).await.unwrap();
        let err = r
            .diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .unwrap_or_else(|| panic!("{:#?}", r.diagnostics));
        assert_eq!(err.line, 2);
        assert!(err.message.contains("erwartet"), "{}", err.message);
        let fix = err.fix.as_ref().expect("Fix-it");
        assert_eq!(fix.edits[0].text, ";");
        assert!(err.file.ends_with("probe.ino"), "{}", err.file);
    }
}
