use crate::state::AppState;
use forge_project::{resolve_inside, resolve_inside_strict};
use std::path::{Path, PathBuf};
use tauri::State;

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn inside(state: &State<AppState>, path: &Path) -> Result<PathBuf, String> {
    resolve_inside(&state.root()?, path).map_err(msg)
}

/// Für Umbenennen und Löschen: der Projektordner selbst ist kein gültiges Ziel.
fn inside_strict(state: &State<AppState>, path: &Path) -> Result<PathBuf, String> {
    resolve_inside_strict(&state.root()?, path).map_err(msg)
}

#[tauri::command(async)]
pub fn file_read(state: State<AppState>, path: PathBuf) -> Result<String, String> {
    let path = inside(&state, &path)?;
    let bytes = std::fs::read(&path).map_err(msg)?;
    String::from_utf8(bytes).map_err(|_| {
        "Die Datei ist keine Textdatei und kann hier nicht bearbeitet werden.".to_string()
    })
}

/// Schreibt über eine temporäre Datei, damit ein Absturz nie eine halbe Datei hinterlässt.
#[tauri::command(async)]
pub fn file_write(state: State<AppState>, path: PathBuf, text: String) -> Result<(), String> {
    let path = inside(&state, &path)?;
    let tmp = path.with_extension(format!(
        "{}.forge-tmp",
        path.extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let result = std::fs::write(&tmp, text).and_then(|_| {
        // Rechte der bestehenden Datei behalten (z. B. ausführbare Skripte).
        #[cfg(unix)]
        if let Ok(meta) = std::fs::metadata(&path) {
            let _ = std::fs::set_permissions(&tmp, meta.permissions());
        }
        std::fs::rename(&tmp, &path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(msg)
}

#[tauri::command(async)]
pub fn file_create(state: State<AppState>, path: PathBuf, is_dir: bool) -> Result<(), String> {
    let path = inside(&state, &path)?;
    if path.exists() {
        return Err(format!(
            "{} existiert bereits.",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    if is_dir {
        std::fs::create_dir_all(&path).map_err(msg)
    } else {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(msg)?;
        }
        std::fs::write(&path, "").map_err(msg)
    }
}

#[tauri::command(async)]
pub fn file_rename(state: State<AppState>, from: PathBuf, to: PathBuf) -> Result<(), String> {
    let from = inside_strict(&state, &from)?;
    let to = inside_strict(&state, &to)?;
    if to.exists() {
        return Err(format!(
            "{} existiert bereits.",
            to.file_name().unwrap_or_default().to_string_lossy()
        ));
    }
    std::fs::rename(from, to).map_err(msg)
}

/// Verschiebt in den Papierkorb statt endgültig zu löschen.
#[tauri::command(async)]
pub fn file_trash(state: State<AppState>, path: PathBuf) -> Result<(), String> {
    let path = inside_strict(&state, &path)?;
    trash::delete(path).map_err(|e| format!("Konnte nicht in den Papierkorb verschieben: {e}"))
}
