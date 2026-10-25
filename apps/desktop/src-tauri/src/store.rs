//! Kleine JSON-Speicher im Datenordner der App.

use std::collections::HashMap;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join(name))
}

fn read(app: &AppHandle, name: &str) -> HashMap<String, String> {
    path(app, name)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write(app: &AppHandle, name: &str, map: &HashMap<String, String>) {
    if let Some(p) = path(app, name) {
        let _ = std::fs::write(p, serde_json::to_vec_pretty(map).unwrap_or_default());
    }
}

/// Welcher Build zuletzt auf welches Gerät geschrieben wurde.
pub fn flashed_hash(app: &AppHandle, device: &str) -> Option<String> {
    read(app, "flashed.json").get(device).cloned()
}

pub fn set_flashed_hash(app: &AppHandle, device: &str, hash: &str) {
    let mut m = read(app, "flashed.json");
    m.insert(device.to_string(), hash.to_string());
    write(app, "flashed.json", &m);
}

/// Eigenschaften der KI-Prüfung, gespeichert im Datenordner der App.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub model: Option<String>,
    pub daily_tokens: Option<u64>,
}

pub fn ai_settings(app: &AppHandle) -> AiSettings {
    path(app, "ai-settings.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn set_ai_settings(app: &AppHandle, s: &AiSettings) {
    if let Some(p) = path(app, "ai-settings.json") {
        let _ = std::fs::write(p, serde_json::to_vec_pretty(s).unwrap_or_default());
    }
}

pub fn data_file(app: &AppHandle, name: &str) -> Option<PathBuf> {
    path(app, name)
}

/// Ob fbuild verwendet werden darf, sobald es installiert ist.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildSettings {
    pub fbuild_enabled: bool,
}

impl Default for BuildSettings {
    fn default() -> Self {
        Self {
            fbuild_enabled: true,
        }
    }
}

pub fn build_settings(app: &AppHandle) -> BuildSettings {
    path(app, "build-settings.json")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn set_build_settings(app: &AppHandle, s: &BuildSettings) {
    if let Some(p) = path(app, "build-settings.json") {
        let _ = std::fs::write(p, serde_json::to_vec_pretty(s).unwrap_or_default());
    }
}

/// Ordner für Werkzeuge, die Forge selbst installiert.
pub fn tools_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("tools");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}
