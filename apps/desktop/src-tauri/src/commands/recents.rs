use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Die Befehle laufen auf dem Thread-Pool: Lesen, Ändern, Schreiben darf sich nicht überlappen.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const FILE: &str = "recents.json";
const MAX: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recent {
    pub path: PathBuf,
    pub name: String,
    pub kind: String,
    pub opened_at: u64,
}

fn file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(FILE))
}

fn read(app: &AppHandle) -> Vec<Recent> {
    file(app)
        .ok()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Zuletzt geöffnete Projekte. Ordner und Dateien, die es nicht mehr gibt, fallen heraus.
#[tauri::command(async)]
pub fn recent_list(app: AppHandle) -> Vec<Recent> {
    read(&app).into_iter().filter(|r| r.path.exists()).collect()
}

#[tauri::command(async)]
pub fn recent_push(
    app: AppHandle,
    path: PathBuf,
    name: String,
    kind: String,
) -> Result<(), String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = read(&app);
    list.retain(|r| r.path != path);
    let opened_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    list.insert(
        0,
        Recent {
            path,
            name,
            kind,
            opened_at,
        },
    );
    list.truncate(MAX);
    let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    crate::store::atomic_write(&file(&app)?, text.as_bytes()).map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn recent_remove(app: AppHandle, path: PathBuf) -> Result<(), String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = read(&app);
    list.retain(|r| r.path != path);
    let text = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
    crate::store::atomic_write(&file(&app)?, text.as_bytes()).map_err(|e| e.to_string())
}
