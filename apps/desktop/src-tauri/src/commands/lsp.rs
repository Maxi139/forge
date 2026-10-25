//! clangd für den Editor: Vervollständigung, Hover, Definition, Signaturhilfe, semantische Token.

use super::build::BoardArg;
use crate::state::AppState;
use forge_analysis::includes;
use forge_lsp::{
    CompletionItem, Hover, Location, Lsp, SemanticTokens, SignatureHelp, StartOptions, Template,
};
use forge_toolchain::check::{load_texts, CheckRequest};
use forge_toolchain::hash::short_key;
use forge_toolchain::CancellationToken;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

fn locate_clangd() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_CLANGD")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    [
        "/opt/homebrew/opt/llvm/bin/clangd",
        "/opt/homebrew/bin/clangd",
        "/usr/local/bin/clangd",
        "/usr/bin/clangd",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
}

async fn session(state: &AppState) -> Option<Arc<Lsp>> {
    state.lsp.lock().await.as_ref().map(|(_, l)| l.clone())
}

/// Hält clangd auf dem Stand des Editors. Gibt `false` zurück, wenn clangd oder der Core fehlt.
#[tauri::command]
pub async fn lsp_sync(
    state: State<'_, AppState>,
    board: BoardArg,
    overrides: HashMap<String, String>,
) -> Result<bool, String> {
    let root = state.root()?;
    let Some(clangd) = locate_clangd() else {
        return Ok(false);
    };
    let overrides: HashMap<PathBuf, String> = overrides
        .into_iter()
        .map(|(k, v)| (PathBuf::from(k), v))
        .collect();
    let texts = load_texts(&root, &overrides).await;
    let meta = state.toolchain.meta().await?;
    let fqbn = board.full();
    let libs = forge_toolchain::build::libs_signature(&meta.library_pairs());
    let include_set: BTreeSet<String> = texts
        .iter()
        .flat_map(|(_, t)| includes(t))
        .map(|i| i.name)
        .collect();
    let key = short_key(&[
        &root.to_string_lossy(),
        &fqbn,
        &libs,
        &include_set.into_iter().collect::<Vec<_>>().join(","),
    ]);

    let mut slot = state.lsp.lock().await;
    let fresh = matches!(slot.as_ref(), Some((k, _)) if *k == key);
    if !fresh {
        *slot = None;
        let checker = state.toolchain.checker()?;
        let cancel = CancellationToken::new();
        let req = CheckRequest {
            root: root.clone(),
            fqbn,
            overrides: overrides.clone(),
            libraries_signature: libs,
        };
        let Some(t) = checker
            .flags_template(&req, &cancel)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(false);
        };
        let work = state
            .toolchain
            .cache_base()
            .join("projects")
            .join(short_key(&[&root.to_string_lossy()]))
            .join("lsp");
        let lsp = Lsp::start(StartOptions {
            clangd,
            work,
            root: root.clone(),
            template: Template {
                directory: t.directory,
                arguments: t.arguments,
                template_file: t.template_file,
                shadow: t.shadow,
                main_name: t.main_name,
            },
        })
        .await
        .map_err(|e| e.to_string())?;
        *slot = Some((key, Arc::new(lsp)));
    }
    let lsp = slot
        .as_ref()
        .map(|(_, l)| l.clone())
        .ok_or("Kein Sprachserver.")?;
    drop(slot);
    lsp.sync(&texts).await.map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn lsp_complete(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Vec<CompletionItem>, String> {
    match session(&state).await {
        Some(l) => l
            .complete(&file, line, col)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn lsp_hover(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Option<Hover>, String> {
    match session(&state).await {
        Some(l) => l.hover(&file, line, col).await.map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn lsp_definition(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Vec<Location>, String> {
    match session(&state).await {
        Some(l) => l
            .definition(&file, line, col)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn lsp_signature(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Option<SignatureHelp>, String> {
    match session(&state).await {
        Some(l) => l
            .signature(&file, line, col)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn lsp_tokens(
    state: State<'_, AppState>,
    file: String,
) -> Result<Option<SemanticTokens>, String> {
    match session(&state).await {
        Some(l) => l.tokens(&file).await.map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

/// Liest eine Datei aus Core, Bibliotheken oder Projekt zum Nachschlagen. Schreiben geht darüber nie.
#[tauri::command]
pub async fn file_read_external(
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<String, String> {
    let path = path
        .canonicalize()
        .map_err(|_| "Die Datei gibt es nicht mehr.".to_string())?;
    let mut allowed: Vec<PathBuf> = Vec::new();
    if let Ok(root) = state.root() {
        allowed.push(root);
    }
    if let Ok(d) = state.toolchain.data_dir().await {
        allowed.push(d);
    }
    if let Ok(cli) = state.toolchain.cli() {
        if let Ok(d) = cli.user_libraries_dir().await {
            allowed.push(d);
        }
    }
    let ok = allowed
        .iter()
        .filter_map(|a| a.canonicalize().ok())
        .any(|a| path.starts_with(&a));
    if !ok {
        return Err("Diese Datei liegt außerhalb von Projekt, Core und Bibliotheken.".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "Die Datei ist keine Textdatei.".to_string())
}

/* ---------- Formatieren (clang-format) ---------- */

fn locate_clang_format() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("FORGE_CLANG_FORMAT")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    // Neben clangd liegt meist auch clang-format.
    let beside = locate_clangd().and_then(|c| c.parent().map(|d| d.join("clang-format")));
    beside
        .into_iter()
        .chain(
            [
                "/opt/homebrew/opt/llvm/bin/clang-format",
                "/opt/homebrew/bin/clang-format",
                "/usr/local/bin/clang-format",
                "/usr/bin/clang-format",
            ]
            .iter()
            .map(PathBuf::from),
        )
        .find(|p| p.is_file())
}

/// Eingebauter Stil, wenn das Projekt keine `.clang-format` hat.
const DEFAULT_STYLE: &str = "{BasedOnStyle: Google, IndentWidth: 2, ColumnLimit: 100}";

/// `--style`-Argument: Eine `.clang-format` im Projekt (bis zur Projektwurzel hinauf) hat Vorrang.
fn style_arg(root: &Path, file: &Path) -> String {
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d.join(".clang-format").is_file() || d.join("_clang-format").is_file() {
            return "--style=file".into();
        }
        if d == root {
            break;
        }
        dir = d.parent();
    }
    format!("--style={DEFAULT_STYLE}")
}

/// Dateiname, den clang-format für die Spracherkennung sieht: `.ino` wird als C++ behandelt.
fn assumed_name(file: &Path) -> PathBuf {
    match file.extension().and_then(|e| e.to_str()) {
        Some(e) if e.eq_ignore_ascii_case("ino") => file.with_extension("cpp"),
        _ => file.to_path_buf(),
    }
}

/// Ob clang-format gefunden wurde. Ohne bleibt der Befehl in der Oberfläche ausgegraut.
#[tauri::command]
pub async fn format_available() -> bool {
    locate_clang_format().is_some()
}

/// Formatiert den Text einer Projektdatei. Gibt den neuen Text zurück, schreibt nichts.
#[tauri::command]
pub async fn format_document(
    state: State<'_, AppState>,
    path: PathBuf,
    text: String,
) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let root = state.root()?;
    let tool = locate_clang_format().ok_or(
        "clang-format wurde nicht gefunden. Installiere es mit „brew install clang-format“.",
    )?;
    let mut child = tokio::process::Command::new(tool)
        .arg(format!(
            "--assume-filename={}",
            assumed_name(&path).display()
        ))
        .arg(style_arg(&root, &path))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("clang-format konnte nicht gestartet werden: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("clang-format ohne Eingabe.")?;
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(text.as_bytes()).await;
    });
    let out = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait_with_output())
        .await
        .map_err(|_| "clang-format antwortet nicht.".to_string())?
        .map_err(|e| e.to_string())?;
    let _ = writer.await;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let first = err.lines().next().unwrap_or("Unbekannter Fehler.");
        return Err(format!("clang-format: {first}"));
    }
    String::from_utf8(out.stdout).map_err(|_| "clang-format lieferte keinen Text.".to_string())
}

#[cfg(test)]
mod format_tests {
    use super::*;

    #[test]
    fn project_style_file_wins_over_builtin() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        let file = root.join("src/a.cpp");
        assert!(style_arg(&root, &file).contains("Google"));
        std::fs::write(root.join(".clang-format"), "BasedOnStyle: LLVM\n").unwrap();
        assert_eq!(style_arg(&root, &file), "--style=file");
    }

    #[test]
    fn sketches_are_formatted_as_cpp() {
        assert_eq!(
            assumed_name(Path::new("/p/blink.ino")),
            PathBuf::from("/p/blink.cpp")
        );
        assert_eq!(assumed_name(Path::new("/p/a.h")), PathBuf::from("/p/a.h"));
    }
}

/* ---------- Gliederung, Referenzen, Umbenennen ---------- */

/// Gliederung einer Datei. `None` ohne Sprachserver, dann bleibt der Oberfläche die eigene Funktionsliste.
#[tauri::command]
pub async fn lsp_symbols(
    state: State<'_, AppState>,
    file: String,
) -> Result<Option<Vec<forge_lsp::Symbol>>, String> {
    match session(&state).await {
        Some(l) => l.symbols(&file).await.map(Some).map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn lsp_references(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Vec<Location>, String> {
    match session(&state).await {
        Some(l) => l
            .references(&file, line, col)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
pub async fn lsp_prepare_rename(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
) -> Result<Option<forge_lsp::RenameTarget>, String> {
    match session(&state).await {
        Some(l) => l
            .prepare_rename(&file, line, col)
            .await
            .map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn lsp_rename(
    state: State<'_, AppState>,
    file: String,
    line: u32,
    col: u32,
    new_name: String,
) -> Result<Vec<forge_lsp::FileEdits>, String> {
    match session(&state).await {
        Some(l) => l
            .rename(&file, line, col, &new_name)
            .await
            .map_err(|e| e.to_string()),
        None => Err("Der Sprachserver läuft nicht. Umbenennen braucht clangd.".into()),
    }
}
