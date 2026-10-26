use super::secrets::redactor;
use crate::state::{AppState, Jobs};
use crate::store::{ai_settings, data_file, set_ai_settings};
use forge_ai::budget::Budget;
use forge_ai::models::{default_model, parse_models, ModelInfo};
use forge_ai::service::{
    fix_issue, reply_in_thread, review_blocks, run_task, FixOutcome, Paths, RunSummary, ThreadReply,
};
use forge_ai::task::{TaskOutcome, MAX_CONTEXT_CHARS};
use forge_ai::threads::{Resolution, Thread, ThreadStore};
use forge_ai::OpenRouter;
use forge_analysis::{analyze, blocks, web_blocks, web_lang, AnalysisInput, Chip, SourceFile};
use forge_project::{ForgeToml, CONFIG_FILE};
use forge_secrets::keychain::SecretStore;
use serde::Serialize;
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, State};

const KEY_ACCOUNT: &str = "app:openrouter";
const FALLBACK_MODEL: &str = "deepseek/deepseek-v4-flash";
const DEFAULT_DAILY_TOKENS: u64 = 50_000;

/// Nur in Entwicklungsbuilds: Schlüssel und Adresse aus der Umgebung, für Tests gegen einen Scheinserver.
#[cfg(debug_assertions)]
fn dev_override() -> (Option<String>, Option<String>) {
    (
        std::env::var("FORGE_OPENROUTER_KEY").ok(),
        std::env::var("FORGE_OPENROUTER_BASE").ok(),
    )
}

#[cfg(not(debug_assertions))]
fn dev_override() -> (Option<String>, Option<String>) {
    (None, None)
}

pub(crate) fn key() -> Option<String> {
    if let Some(k) = dev_override().0 {
        return Some(k);
    }
    super::secrets::store()
        .get(KEY_ACCOUNT)
        .ok()
        .flatten()
        .filter(|k| !k.is_empty())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiState {
    pub has_key: bool,
    pub model: String,
    pub model_is_default: bool,
    pub daily_tokens: u64,
    pub used_today: u64,
    pub project_enabled: bool,
}

fn project_enabled(root: Option<&std::path::Path>) -> bool {
    root.and_then(|r| ForgeToml::load(&r.join(CONFIG_FILE)).ok())
        .map(|c| c.ai.enabled)
        .unwrap_or(true)
}

fn effective_model(app: &AppHandle, state: &AppState) -> (String, bool) {
    if let Some(m) = ai_settings(app).model.filter(|m| !m.is_empty()) {
        return (m, false);
    }
    (
        state
            .default_model
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| FALLBACK_MODEL.to_string()),
        true,
    )
}

/// Schlüsselbundzugriffe laufen abseits des Hauptthreads. macOS kann dabei nach dem Passwort fragen,
/// und währenddessen darf das Fenster nicht einfrieren.
#[tauri::command]
pub async fn ai_state(app: AppHandle, state: State<'_, AppState>) -> Result<AiState, String> {
    let s = ai_settings(&app);
    let (model, model_is_default) = effective_model(&app, &state);
    let used_today = data_file(&app, "ai-usage.json")
        .map(|p| Budget::load(p).used())
        .unwrap_or(0);
    let project_enabled = project_enabled(state.root().ok().as_deref());
    let has_key = tokio::task::spawn_blocking(|| key().is_some())
        .await
        .unwrap_or(false);
    Ok(AiState {
        has_key,
        model,
        model_is_default,
        daily_tokens: s.daily_tokens.unwrap_or(DEFAULT_DAILY_TOKENS),
        used_today,
        project_enabled,
    })
}

/// Setzt Schlüssel (leer = entfernen), Modell (leer = automatisch) und Tageslimit.
#[tauri::command]
pub async fn ai_configure(
    app: AppHandle,
    key_value: Option<String>,
    model: Option<String>,
    daily_tokens: Option<u64>,
) -> Result<(), String> {
    if let Some(k) = key_value {
        tokio::task::spawn_blocking(move || {
            let store = super::secrets::store();
            if k.trim().is_empty() {
                store.delete(KEY_ACCOUNT).map_err(|e| e.to_string())
            } else {
                store.set(KEY_ACCOUNT, k.trim()).map_err(|e| e.to_string())
            }
        })
        .await
        .map_err(|e| e.to_string())??;
    }
    let mut s = ai_settings(&app);
    if let Some(m) = model {
        s.model = Some(m.trim().to_string()).filter(|m| !m.is_empty());
    }
    if let Some(d) = daily_tokens {
        s.daily_tokens = Some(d);
    }
    set_ai_settings(&app, &s);
    Ok(())
}

#[tauri::command]
pub fn ai_set_project_enabled(state: State<AppState>, enabled: bool) -> Result<(), String> {
    let root = state.root()?;
    let path = root.join(CONFIG_FILE);
    let mut cfg = ForgeToml::load(&path).map_err(|_| {
        "Nur Projekte können die Prüfung ein- oder ausschalten. Lege ein Projekt an.".to_string()
    })?;
    cfg.ai.enabled = enabled;
    cfg.save(&path).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelList {
    pub models: Vec<ModelInfo>,
    pub suggested: Option<String>,
}

/// Die öffentliche Modellliste von OpenRouter, nach Preis sortiert, mit einem Vorschlag.
#[tauri::command]
pub async fn ai_models(state: State<'_, AppState>) -> Result<ModelList, String> {
    let raw = OpenRouter::models(None).await.map_err(|e| e.to_string())?;
    let mut models: Vec<ModelInfo> = parse_models(raw)
        .into_iter()
        .filter(|m| m.supports_json && m.prompt_price > 0.0)
        .collect();
    models.sort_by(|a, b| a.prompt_price.total_cmp(&b.prompt_price));
    let suggested = default_model(&models);
    if let Ok(mut g) = state.default_model.lock() {
        *g = suggested.clone();
    }
    Ok(ModelList { models, suggested })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRun {
    pub summary: RunSummary,
    /// Alle Blöcke des Laufs. Was sonst noch angezeigt wird, gibt es nicht mehr.
    pub blocks: Vec<(String, String)>,
    /// Grund, warum nicht geprüft wurde.
    pub skipped: Option<String>,
}

fn skipped(why: &str) -> AiRun {
    AiRun {
        summary: RunSummary::default(),
        blocks: vec![],
        skipped: Some(why.into()),
    }
}

/// Prüft die Funktionen des Sketches auf Logikfehler. Ergebnisse kommen als `ai:block` und am Ende gesammelt.
#[tauri::command]
pub async fn ai_check(
    app: AppHandle,
    state: State<'_, AppState>,
    board: super::build::BoardArg,
    overrides: HashMap<String, String>,
    skip: Option<Vec<String>>,
) -> Result<AiRun, String> {
    let root = state.root()?;
    if !project_enabled(Some(&root)) {
        return Ok(skipped("projectOff"));
    }
    let Some(api_key) = key() else {
        return Ok(skipped("noKey"));
    };

    if state
        .default_model
        .lock()
        .map(|g| g.is_none())
        .unwrap_or(false)
        && ai_settings(&app).model.is_none()
    {
        if let Ok(raw) = OpenRouter::models(None).await {
            if let Ok(mut g) = state.default_model.lock() {
                *g = default_model(&parse_models(raw));
            }
        }
    }
    let (model, _) = effective_model(&app, &state);
    let settings = ai_settings(&app);

    let cancel = Jobs::begin(&state.jobs.ai);
    let prepared = prepare_blocks(
        root.clone(),
        board.fqbn.clone(),
        overrides.clone(),
        skip.unwrap_or_default(),
    )
    .await?;
    let (block_list, texts) = prepared;
    let names: Vec<(String, String)> = block_list
        .iter()
        .map(|b| (b.file.clone(), b.name.clone()))
        .collect();

    let red = redactor(&root, [("OPENROUTER_KEY".to_string(), api_key.clone())]);
    let client = OpenRouter::new(api_key, dev_override().1);
    let paths = paths(&app, &root)?;
    let handle = app.clone();
    let summary = review_blocks(
        &client,
        &paths,
        &model,
        settings.daily_tokens.unwrap_or(DEFAULT_DAILY_TOKENS),
        &red,
        block_list,
        &texts,
        &cancel,
        move |r| {
            let _ = handle.emit("ai:block", r);
        },
    )
    .await;
    if summary.calls > 0 {
        // Neue Ergebnisse gehören in die Projektdatei.
        super::package::touch(&app);
    }
    Ok(AiRun {
        summary,
        blocks: names,
        skipped: None,
    })
}

/// Die Funktionen des Projekts samt Texten, so wie die KI sie sieht. Dateien mit Strukturfehlern fehlen.
async fn prepare_blocks(
    root: std::path::PathBuf,
    fqbn: String,
    overrides: HashMap<String, String>,
    skip: Vec<String>,
) -> Result<(Vec<forge_analysis::Block>, HashMap<String, String>), String> {
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();
        for path in forge_toolchain::hash::source_files(&root) {
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !matches!(ext.as_str(), "ino" | "cpp" | "c" | "cc" | "h" | "hpp") {
                continue;
            }
            let key = path.to_string_lossy().to_string();
            if let Some(text) = overrides
                .get(&key)
                .cloned()
                .or_else(|| std::fs::read_to_string(&path).ok())
            {
                files.push(SourceFile { path: key, text });
            }
        }
        // Dateien mit Strukturfehlern prüft die KI nicht. Sie würde nur raten.
        let broken: Vec<String> = files
            .iter()
            .filter(|f| !forge_analysis::syntax_diagnostics(&f.path, &f.text).is_empty())
            .map(|f| f.path.clone())
            .collect();
        let input = AnalysisInput {
            files: &files,
            chip: Chip::from_fqbn(&fqbn),
        };
        let facts = analyze(&input).facts;
        let label = forge_toolchain::models::fqbn_with_options(&fqbn, &Default::default());
        let all = blocks(&input, &facts, &label);
        let texts: HashMap<String, String> = files
            .iter()
            .map(|f| (f.path.clone(), f.text.clone()))
            .collect();
        let mut usable: Vec<_> = all
            .into_iter()
            .filter(|b| !broken.contains(&b.file))
            .collect();

        // Webdateien: Strukturfehler kennt nur die Oberfläche (`skip`), die KI prüft solche Dateien nicht.
        let web: Vec<SourceFile> = web_files(&root)
            .into_iter()
            .filter_map(|path| {
                let key = path.to_string_lossy().to_string();
                let text = overrides
                    .get(&key)
                    .cloned()
                    .or_else(|| std::fs::read_to_string(&path).ok())?;
                Some(SourceFile { path: key, text })
            })
            .collect();
        let web_ok: Vec<SourceFile> = web
            .iter()
            .filter(|f| !skip.contains(&f.path))
            .cloned()
            .collect();
        usable.extend(web_blocks(&web_ok, &files, &label));
        let mut texts = texts;
        texts.extend(web.into_iter().map(|f| (f.path, f.text)));
        (usable, texts)
    })
    .await
    .map_err(|e| e.to_string())
}

fn paths(app: &AppHandle, root: &std::path::Path) -> Result<Paths, String> {
    Ok(Paths {
        // Zwischenspeicher und Gespräche gehören zum Projekt und reisen in der .forgeproj-Datei mit.
        cache: root.join(forge_package::AI_CACHE_FILE),
        budget: data_file(app, "ai-usage.json")
            .ok_or("Der Speicherort der App ist nicht erreichbar.")?,
        threads: root.join(forge_package::AI_THREADS_FILE),
    })
}

/// Alle Gespräche über Hinweise der KI in diesem Projekt, offene und geschlossene.
#[tauri::command]
pub fn ai_threads(state: State<AppState>) -> Result<Vec<Thread>, String> {
    let root = state.root()?;
    Ok(ThreadStore::load(root.join(forge_package::AI_THREADS_FILE))
        .all()
        .to_vec())
}

/// Der Nutzer antwortet auf einen Hinweis. Die KI liest den Verlauf und die Funktion und antwortet.
#[tauri::command]
pub async fn ai_thread_reply(
    app: AppHandle,
    state: State<'_, AppState>,
    board: super::build::BoardArg,
    overrides: HashMap<String, String>,
    thread_id: String,
    text: String,
) -> Result<ThreadReply, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("Schreib etwas, worauf die KI antworten kann.".into());
    }
    let root = state.root()?;
    let Some(api_key) = key() else {
        return Err(
            "Für Antworten braucht Forge einen OpenRouter-Schlüssel (Einstellungen → KI).".into(),
        );
    };
    let (model, _) = effective_model(&app, &state);
    let settings = ai_settings(&app);
    let p = paths(&app, &root)?;

    // Die Funktion, um die es geht, in ihrem heutigen Stand. Gibt es sie nicht mehr, antwortet die KI nur aus dem Verlauf.
    let function = ThreadStore::load(p.threads.clone())
        .get(&thread_id)
        .map(|t| t.function.clone())
        .ok_or("Das Gespräch gibt es nicht mehr.")?;
    let (block_list, _) =
        prepare_blocks(root.clone(), board.fqbn.clone(), overrides, Vec::new()).await?;
    let block = block_list.into_iter().find(|b| b.name == function);

    let red = redactor(&root, [("OPENROUTER_KEY".to_string(), api_key.clone())]);
    let client = OpenRouter::new(api_key, dev_override().1);
    let reply = reply_in_thread(
        &client,
        &p,
        &model,
        settings.daily_tokens.unwrap_or(DEFAULT_DAILY_TOKENS),
        &red,
        &thread_id,
        &text,
        block.as_ref(),
    )
    .await
    .map_err(|e| e.to_string());
    // Auch die Nachricht des Nutzers steht jetzt in der Projektdatei.
    super::package::touch(&app);
    reply
}

/// Schließt ein Gespräch, ohne dass die KI zustimmen muss. Der Hinweis erscheint nicht wieder.
#[tauri::command]
pub fn ai_thread_close(
    app: AppHandle,
    state: State<AppState>,
    thread_id: String,
) -> Result<Thread, String> {
    change_thread(&app, &state, &thread_id, |s, id| {
        s.close(id, Resolution::Dismissed).cloned()
    })
}

#[tauri::command]
pub fn ai_thread_reopen(
    app: AppHandle,
    state: State<AppState>,
    thread_id: String,
) -> Result<Thread, String> {
    change_thread(&app, &state, &thread_id, |s, id| s.reopen(id).cloned())
}

fn change_thread(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    f: impl FnOnce(&mut ThreadStore, &str) -> Option<Thread>,
) -> Result<Thread, String> {
    let root = state.root()?;
    let mut store = ThreadStore::load(root.join(forge_package::AI_THREADS_FILE));
    let thread = f(&mut store, id).ok_or("Das Gespräch gibt es nicht mehr.")?;
    store.save();
    super::package::touch(app);
    Ok(thread)
}

/// Lässt die KI einen Hinweis beheben und gibt die Änderung zurück. Angewendet wird sie im Editor, damit ⌘Z sie zurücknimmt.
#[tauri::command]
pub async fn ai_fix(
    app: AppHandle,
    state: State<'_, AppState>,
    board: super::build::BoardArg,
    overrides: HashMap<String, String>,
    file: String,
    line: u32,
    message: String,
) -> Result<FixOutcome, String> {
    let root = state.root()?;
    let Some(api_key) = key() else {
        return Err(
            "Zum Beheben braucht Forge einen OpenRouter-Schlüssel (Einstellungen → KI).".into(),
        );
    };
    let (model, _) = effective_model(&app, &state);
    let settings = ai_settings(&app);
    let p = paths(&app, &root)?;

    let (block_list, texts) =
        prepare_blocks(root.clone(), board.fqbn.clone(), overrides, Vec::new()).await?;
    let block = block_list
        .into_iter()
        .find(|b| b.file == file && b.start_line <= line && line <= b.end_line)
        .ok_or("Zu dieser Zeile gehört keine Funktion, oder die Datei hat Strukturfehler.")?;
    let text = texts.get(&file).ok_or("Die Datei gibt es nicht mehr.")?;

    let red = redactor(&root, [("OPENROUTER_KEY".to_string(), api_key.clone())]);
    let client = OpenRouter::new(api_key, dev_override().1);
    fix_issue(
        &client,
        &p,
        &model,
        settings.daily_tokens.unwrap_or(DEFAULT_DAILY_TOKENS),
        &red,
        &block,
        text,
        line,
        &message,
    )
    .await
    .map_err(|e| e.to_string())
}

const WEB_SKIP_DIRS: &[&str] = &[".git", ".forge", "build", "node_modules", ".vscode"];
const WEB_MAX_FILES: usize = 60;
const WEB_MAX_BYTES: u64 = 200_000;

/// Die Webdateien (HTML, CSS, JavaScript) des Projekts, sortiert.
fn web_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !WEB_SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    walk(&path, out);
                }
            } else if kind.is_file()
                && web_lang(&name).is_some()
                && entry.metadata().is_ok_and(|m| m.len() <= WEB_MAX_BYTES)
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out.truncate(WEB_MAX_FILES);
    out
}

/// Die Quelldateien des Projekts für eine Aufgabe: das geöffnete zuerst, dann der Rest, solange er in den Kontext passt.
fn task_files(
    root: &std::path::Path,
    overrides: &HashMap<String, String>,
    active: Option<&str>,
) -> Result<Vec<(String, String, String)>, String> {
    let mut all: Vec<(String, String, String)> = Vec::new();
    // Quelltext des Sketches und Webdateien: Eine Aufgabe betrifft oft beides, etwa eine neue Adresse samt Knopf.
    let paths = forge_toolchain::hash::source_files(root)
        .into_iter()
        .chain(web_files(root));
    for path in paths {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !matches!(ext.as_str(), "ino" | "cpp" | "c" | "cc" | "h" | "hpp")
            && web_lang(&path.to_string_lossy()).is_none()
        {
            continue;
        }
        let full = path.to_string_lossy().to_string();
        let Some(text) = overrides
            .get(&full)
            .cloned()
            .or_else(|| std::fs::read_to_string(&path).ok())
        else {
            continue;
        };
        let shown = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| full.clone());
        all.push((shown, full, text));
    }
    // Das geöffnete Dokument gehört auf jeden Fall dazu.
    all.sort_by_key(|(_, full, _)| Some(full.as_str()) != active);
    let mut used = 0;
    let mut out = Vec::new();
    for f in all {
        let size = f.2.chars().count();
        if used + size > MAX_CONTEXT_CHARS {
            if out.is_empty() {
                return Err(
                    "Die Datei ist zu groß für eine Aufgabe. Wähle eine kleinere Datei.".into(),
                );
            }
            continue;
        }
        used += size;
        out.push(f);
    }
    Ok(out)
}

/// Gibt der KI eine Aufgabe. Sie liefert Änderungen an mehreren Dateien und neue Dateien. Angewendet wird im Editor.
#[tauri::command]
pub async fn ai_task(
    app: AppHandle,
    state: State<'_, AppState>,
    board: super::build::BoardArg,
    overrides: HashMap<String, String>,
    task: String,
    active: Option<String>,
    problems: String,
) -> Result<TaskOutcome, String> {
    let task = task.trim().to_string();
    if task.is_empty() {
        return Err("Beschreibe, was die KI tun soll.".into());
    }
    let root = state.root()?;
    let Some(api_key) = key() else {
        return Err(
            "Für Aufgaben braucht Forge einen OpenRouter-Schlüssel (Einstellungen → KI).".into(),
        );
    };
    let (model, _) = effective_model(&app, &state);
    let settings = ai_settings(&app);
    let p = paths(&app, &root)?;

    let root2 = root.clone();
    let active2 = active.clone();
    let files =
        tokio::task::spawn_blocking(move || task_files(&root2, &overrides, active2.as_deref()))
            .await
            .map_err(|e| e.to_string())??;
    let shown_active = active.as_ref().and_then(|a| {
        files
            .iter()
            .find(|(_, full, _)| full == a)
            .map(|(s, _, _)| s.clone())
    });
    let label = forge_toolchain::models::fqbn_with_options(&board.fqbn, &Default::default());

    let red = redactor(&root, [("OPENROUTER_KEY".to_string(), api_key.clone())]);
    let client = OpenRouter::new(api_key, dev_override().1);
    run_task(
        &client,
        &p,
        &model,
        settings.daily_tokens.unwrap_or(DEFAULT_DAILY_TOKENS),
        &red,
        &root,
        &label,
        &files,
        shown_active.as_deref(),
        &problems,
        &task,
    )
    .await
    .map_err(|e| e.to_string())
}
