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
use std::path::PathBuf;
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
