use crate::state::{AppState, Jobs};
use forge_project::{ForgeToml, CONFIG_FILE};
use forge_toolchain::models::{LibraryInfo, LibrarySearchHit, OutdatedItem};
use forge_toolchain::process::Stream;
use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

#[derive(Serialize, Clone)]
struct LibProgress {
    name: String,
    line: String,
    fraction: Option<f32>,
}

#[tauri::command]
pub async fn libraries_installed(state: State<'_, AppState>) -> Result<Vec<LibraryInfo>, String> {
    state
        .toolchain
        .cli()?
        .libraries()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn libraries_search(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<LibrarySearchHit>, String> {
    let cli = state.toolchain.cli()?;
    let installed = cli.libraries().await.map_err(|e| e.to_string())?;
    cli.search_libraries(&query, &installed)
        .await
        .map_err(|e| e.to_string())
}

pub(crate) enum Op {
    Install(Option<String>),
    Uninstall,
    Upgrade,
    /// Installation aus einer ZIP-Datei; `name` ist dann der Dateiname ohne Endung.
    InstallZip(std::path::PathBuf),
}

pub(crate) async fn run(
    app: &AppHandle,
    state: &AppState,
    name: String,
    op: Op,
) -> Result<(), String> {
    let cli = state.toolchain.cli()?;
    let cancel = Jobs::begin(&state.jobs.library);
    let pct = Regex::new(r"(\d+(?:\.\d+)?)\s*%").map_err(|e| e.to_string())?;
    let mut sink = |_: Stream, l: &str| {
        let fraction = pct
            .captures(l)
            .and_then(|c| c[1].parse::<f32>().ok())
            .map(|p| p / 100.0);
        let _ = app.emit(
            "library:progress",
            LibProgress {
                name: name.clone(),
                line: l.to_string(),
                fraction,
            },
        );
    };
    match &op {
        Op::Install(v) => {
            cli.install_library(&name, v.as_deref(), &cancel, &mut sink)
                .await
        }
        Op::Uninstall => cli.uninstall_library(&name, &cancel, &mut sink).await,
        Op::Upgrade => cli.upgrade_library(&name, &cancel, &mut sink).await,
        Op::InstallZip(zip) => cli.install_library_zip(zip, &cancel, &mut sink).await,
    }
    .map_err(|e| e.to_string())?;
    state.toolchain.invalidate();

    // Projekte merken sich ihre Bibliotheken samt Version in forge.toml.
    if let Ok(root) = state.root() {
        let cfg_path = root.join(CONFIG_FILE);
        if cfg_path.is_file() {
            if let Ok(mut cfg) = ForgeToml::load(&cfg_path) {
                match op {
                    Op::Uninstall => {
                        cfg.libraries.remove(&name);
                    }
                    _ => {
                        if let Some(l) = cli
                            .libraries()
                            .await
                            .ok()
                            .and_then(|l| l.into_iter().find(|l| l.name == name))
                        {
                            cfg.libraries.insert(name.clone(), l.version);
                        }
                    }
                }
                let _ = cfg.save(&cfg_path);
            }
        }
    }
    let _ = app.emit("library:changed", &name);
    // Die Projektdatei merkt sich die benutzten Bibliotheken.
    super::package::touch(app);
    Ok(())
}

#[tauri::command]
pub async fn library_install(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
    version: Option<String>,
) -> Result<(), String> {
    run(&app, &state, name, Op::Install(version)).await
}

#[tauri::command]
pub async fn library_uninstall(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    run(&app, &state, name, Op::Uninstall).await
}

#[tauri::command]
pub async fn library_upgrade(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    run(&app, &state, name, Op::Upgrade).await
}

/// Bibliotheken, für die es eine neuere Version gibt.
#[tauri::command]
pub async fn libraries_outdated(state: State<'_, AppState>) -> Result<Vec<OutdatedItem>, String> {
    state
        .toolchain
        .cli()?
        .outdated_libraries()
        .await
        .map_err(|e| e.to_string())
}

/// Installiert eine Bibliothek aus einer ZIP-Datei.
#[tauri::command]
pub async fn library_install_zip(
    app: AppHandle,
    state: State<'_, AppState>,
    path: std::path::PathBuf,
) -> Result<(), String> {
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    run(&app, &state, name, Op::InstallZip(path)).await
}
