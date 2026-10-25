//! Board und Optionen eines Projekts. Projekte speichern sie in `forge.toml`, Sketches in der App,
//! damit ihr Ordner unberührt bleibt.

use crate::state::AppState;
use forge_project::{open, ForgeToml, ProjectInfo, CONFIG_FILE};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SketchBoard {
    fqbn: String,
    #[serde(default)]
    options: BTreeMap<String, String>,
}

fn store_path(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("sketch-boards.json"))
}

fn read_store(app: &AppHandle) -> HashMap<String, SketchBoard> {
    store_path(app)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Ergänzt ein geöffnetes Projekt um das gemerkte Board, falls es ein Sketch ist.
pub fn apply_sketch_board(app: &AppHandle, info: &mut ProjectInfo) {
    if info.path.join(CONFIG_FILE).is_file() {
        return;
    }
    if let Some(b) = read_store(app).get(&info.path.to_string_lossy().to_string()) {
        if !b.fqbn.is_empty() {
            info.config.board.fqbn = b.fqbn.clone();
            info.config.board.options = b.options.clone();
        }
    }
}

/// Das gemerkte Board eines Sketches, falls es eins gibt.
pub fn sketch_board(app: &AppHandle, root: &Path) -> Option<(String, BTreeMap<String, String>)> {
    read_store(app)
        .get(&root.to_string_lossy().to_string())
        .filter(|b| !b.fqbn.is_empty())
        .map(|b| (b.fqbn.clone(), b.options.clone()))
}

/// Merkt sich das Board eines Sketches.
pub fn store_sketch_board(
    app: &AppHandle,
    root: &Path,
    fqbn: &str,
    options: BTreeMap<String, String>,
) -> Result<(), String> {
    let mut store = read_store(app);
    store.insert(
        root.to_string_lossy().to_string(),
        SketchBoard {
            fqbn: fqbn.to_string(),
            options,
        },
    );
    let path = store_path(app).ok_or("Der Speicherort der App ist nicht erreichbar.")?;
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

/// Das gemerkte Board eines Sketches zieht mit, wenn sein Ordner einen neuen Pfad bekommt.
pub fn move_sketch_board(app: &AppHandle, from: &Path, to: &Path) {
    let mut store = read_store(app);
    if let Some(b) = store.remove(&from.to_string_lossy().to_string()) {
        store.insert(to.to_string_lossy().to_string(), b);
        if let (Some(path), Ok(bytes)) = (store_path(app), serde_json::to_vec_pretty(&store)) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

fn save(
    app: &AppHandle,
    root: &Path,
    fqbn: &str,
    options: BTreeMap<String, String>,
) -> Result<(), String> {
    let cfg_path = root.join(CONFIG_FILE);
    if cfg_path.is_file() {
        let mut cfg = ForgeToml::load(&cfg_path).map_err(|e| e.to_string())?;
        cfg.board.fqbn = fqbn.to_string();
        cfg.board.options = options;
        return cfg.save(&cfg_path).map_err(|e| e.to_string());
    }
    store_sketch_board(app, root, fqbn, options)
}

#[tauri::command]
pub fn project_set_board(
    app: AppHandle,
    state: State<AppState>,
    fqbn: String,
    options: BTreeMap<String, String>,
) -> Result<ProjectInfo, String> {
    let root = state.root()?;
    save(&app, &root, &fqbn, options)?;
    let mut info = open(&root).map_err(|e| e.to_string())?;
    apply_sketch_board(&app, &mut info);
    info.package = state
        .package
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.package.clone()));
    // Bei Sketches steht das Board nicht im Ordner, die Projektdatei muss es trotzdem erfahren.
    super::package::touch(&app);
    Ok(info)
}
