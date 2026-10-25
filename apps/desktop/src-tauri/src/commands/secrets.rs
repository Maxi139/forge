use crate::state::AppState;
use forge_project::{secrets_id, ForgeToml, CONFIG_FILE};
use forge_secrets::keychain::{self, Keychain, Scope, SecretStore};
use forge_secrets::{library, scan, Redactor};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::Path;
use tauri::State;

pub fn store() -> Keychain {
    Keychain
}

/// Namen der Secrets eines Projekts: aus `forge.toml` und aus `Secret("...")` im Code.
pub fn project_secret_names(root: &Path) -> (BTreeSet<String>, BTreeSet<String>) {
    let declared: BTreeSet<String> = ForgeToml::load(&root.join(CONFIG_FILE))
        .map(|c| c.secrets.names.into_iter().collect())
        .unwrap_or_default();
    let mut used = BTreeSet::new();
    for f in forge_toolchain::hash::source_files(root) {
        if f.extension().is_some_and(|e| {
            matches!(
                e.to_string_lossy().to_lowercase().as_str(),
                "ino" | "cpp" | "c" | "h" | "hpp"
            )
        }) {
            if let Ok(t) = std::fs::read_to_string(&f) {
                used.extend(scan::used_names(&t));
            }
        }
    }
    (declared, used)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub name: String,
    pub set: bool,
    pub scope: Option<Scope>,
    /// Der Code liest dieses Secret.
    pub used: bool,
}

#[tauri::command]
pub async fn secrets_list(state: State<'_, AppState>) -> Result<Vec<SecretStatus>, String> {
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        let id = secrets_id(&root);
        let (declared, used) = project_secret_names(&root);
        let s = store();
        let mut out = Vec::new();
        for name in declared.iter().chain(used.iter()).collect::<BTreeSet<_>>() {
            let found = keychain::resolve(&s, &id, name).map_err(|e| e.to_string())?;
            out.push(SecretStatus {
                name: name.clone(),
                set: found.is_some(),
                scope: found.map(|(_, sc)| sc),
                used: used.contains(name),
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 15
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err("Der Name darf höchstens 15 Zeichen haben und nur aus Buchstaben, Ziffern und _ bestehen.".into());
    }
    Ok(())
}

fn declare(root: &Path, name: &str, add: bool) {
    let path = root.join(CONFIG_FILE);
    if let Ok(mut cfg) = ForgeToml::load(&path) {
        cfg.secrets.names.retain(|n| n != name);
        if add {
            cfg.secrets.names.push(name.to_string());
        }
        let _ = cfg.save(&path);
    }
}

/// Speichert einen Wert im Schlüsselbund. `global` macht ihn für alle Projekte verfügbar.
#[tauri::command]
pub async fn secret_set(
    state: State<'_, AppState>,
    name: String,
    value: String,
    global: bool,
) -> Result<(), String> {
    valid_name(&name)?;
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        keychain::set(
            &store(),
            &secrets_id(&root),
            &name,
            &value,
            if global {
                Scope::Global
            } else {
                Scope::Project
            },
        )
        .map_err(|e| e.to_string())?;
        declare(&root, &name, true);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn secret_remove(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let root = state.root()?;
    tokio::task::spawn_blocking(move || {
        let id = secrets_id(&root);
        // Nur den Wert dieses Projekts entfernen. Ein globaler Wert gehört auch anderen Projekten.
        store()
            .delete(&format!("project:{id}:{name}"))
            .map_err(|e| e.to_string())?;
        declare(&root, &name, false);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stellt die Arduino-Bibliothek `ForgeSecrets` bereit, wenn das Projekt sie braucht.
pub async fn ensure_library(state: &AppState, unsaved: &[&String]) -> Result<(), String> {
    let root = state.root()?;
    let (declared, used) = project_secret_names(&root);
    let mentions = unsaved
        .iter()
        .any(|t| t.contains("ForgeSecrets.h") || !scan::used_names(t).is_empty())
        || forge_toolchain::hash::source_files(&root)
            .iter()
            .any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.contains("ForgeSecrets.h")));
    if declared.is_empty() && used.is_empty() && !mentions {
        return Ok(());
    }
    let dir = state
        .toolchain
        .cli()?
        .user_libraries_dir()
        .await
        .map_err(|e| e.to_string())?;
    if library::ensure_installed(&dir).map_err(|e| e.to_string())? {
        state.toolchain.invalidate();
    }
    Ok(())
}

/// Alle bekannten Werte dieses Projekts als Schwärzer. Er läuft vor jeder Anfrage an KI oder MCP.
pub fn redactor(root: &Path, extra: impl IntoIterator<Item = (String, String)>) -> Redactor {
    let id = secrets_id(root);
    let (declared, used) = project_secret_names(root);
    let s = store();
    let mut items: Vec<(String, String)> = extra.into_iter().collect();
    for name in declared.iter().chain(used.iter()) {
        if let Ok(Some((v, _))) = keychain::resolve(&s, &id, name) {
            items.push((name.clone(), v));
        }
    }
    Redactor::new(items)
}
