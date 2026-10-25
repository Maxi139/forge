use crate::state::AppState;
use forge_toolchain::models::{BoardEntry, BoardOption};
use forge_toolchain::process::Stream;
use forge_toolchain::CancellationToken;
use regex::Regex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

const ESP32_INDEX: &str = "https://espressif.github.io/arduino-esp32/package_esp32_index.json";
const ESP32_CORE: &str = "esp32:esp32";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub arduino_cli: Option<String>,
    pub esp32_core: Option<String>,
    pub esptool: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn toolchain_status(state: State<'_, AppState>) -> Result<ToolchainStatus, String> {
    let cli = match state.toolchain.cli() {
        Ok(c) => c,
        Err(e) => {
            return Ok(ToolchainStatus {
                arduino_cli: None,
                esp32_core: None,
                esptool: false,
                error: Some(e),
            })
        }
    };
    let version = cli.version().await.ok();
    let core = cli
        .cores()
        .await
        .ok()
        .and_then(|c| c.into_iter().find(|c| c.id == ESP32_CORE))
        .and_then(|c| c.installed);
    let esptool = state.toolchain.esptool().await.is_ok();
    Ok(ToolchainStatus {
        arduino_cli: version,
        esp32_core: core,
        esptool,
        error: None,
    })
}

#[derive(Serialize, Clone)]
struct Progress {
    line: String,
    fraction: Option<f32>,
}

/// Richtet den ESP32-Core ein: Espressif-Index eintragen, Index laden, Core installieren.
#[tauri::command]
pub async fn toolchain_setup(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let cli = state.toolchain.cli()?;
    let pct = Regex::new(r"(\d+(?:\.\d+)?)\s*%").map_err(|e| e.to_string())?;
    let cancel = CancellationToken::new();
    let emit = |app: &AppHandle, line: &str| {
        let fraction = pct
            .captures(line)
            .and_then(|c| c[1].parse::<f32>().ok())
            .map(|p| p / 100.0);
        let _ = app.emit(
            "toolchain:progress",
            Progress {
                line: line.to_string(),
                fraction,
            },
        );
    };

    let _ = app.emit(
        "toolchain:progress",
        Progress {
            line: "Trage Espressif-Quelle ein".into(),
            fraction: None,
        },
    );
    cli.ensure_board_url(ESP32_INDEX)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "toolchain:progress",
        Progress {
            line: "Lade Paketliste".into(),
            fraction: None,
        },
    );
    cli.update_index(&cancel, |_, l| emit(&app, l))
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(
        "toolchain:progress",
        Progress {
            line: "Installiere ESP32-Core".into(),
            fraction: None,
        },
    );
    cli.install_core(ESP32_CORE, &cancel, |s, l| {
        if s == Stream::Stdout || s == Stream::Stderr {
            emit(&app, l);
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    state.toolchain.invalidate();
    Ok(())
}

#[tauri::command]
pub async fn boards_list(state: State<'_, AppState>) -> Result<Vec<BoardEntry>, String> {
    let mut boards = state
        .toolchain
        .cli()?
        .boards("")
        .await
        .map_err(|e| e.to_string())?;
    // ESP32 zuerst, damit die Liste dort beginnt, wo die Nutzer suchen.
    boards.sort_by(|a, b| {
        (a.platform != ESP32_CORE)
            .cmp(&(b.platform != ESP32_CORE))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(boards)
}

#[tauri::command]
pub async fn board_options(
    state: State<'_, AppState>,
    fqbn: String,
) -> Result<Vec<BoardOption>, String> {
    state
        .toolchain
        .cli()?
        .board_options(&fqbn)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FbuildStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub enabled: bool,
    /// Die Boards, für die fbuild bei uns baut.
    pub boards: Vec<String>,
}

pub fn fbuild_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dirs: Vec<std::path::PathBuf> = crate::store::tools_dir(app).into_iter().collect();
    forge_toolchain::fbuild::locate(&dirs)
}

#[tauri::command]
pub async fn fbuild_status(app: AppHandle) -> FbuildStatus {
    let path = fbuild_path(&app);
    let version = match &path {
        Some(p) => forge_toolchain::fbuild::version(p).await,
        None => None,
    };
    FbuildStatus {
        installed: path.is_some(),
        version,
        enabled: crate::store::build_settings(&app).fbuild_enabled,
        boards: vec!["ESP32 Dev Module".into()],
    }
}

#[tauri::command]
pub fn fbuild_set_enabled(app: AppHandle, enabled: bool) {
    let mut s = crate::store::build_settings(&app);
    s.fbuild_enabled = enabled;
    crate::store::set_build_settings(&app, &s);
}

/// Installiert fbuild in einer eigenen Python-Umgebung und lädt die Toolchain vor.
#[tauri::command]
pub async fn fbuild_install(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let dir =
        crate::store::tools_dir(&app).ok_or("Der Speicherort der App ist nicht erreichbar.")?;
    let cancel = CancellationToken::new();
    let pct = Regex::new(r"\((\d+)%\)|(\d+(?:\.\d+)?)\s*%").map_err(|e| e.to_string())?;
    let say = |app: &AppHandle, line: &str| {
        let fraction = pct
            .captures(line)
            .and_then(|c| c.get(1).or(c.get(2)))
            .and_then(|m| m.as_str().parse::<f32>().ok())
            .map(|p| p / 100.0);
        let _ = app.emit(
            "toolchain:progress",
            Progress {
                line: line.to_string(),
                fraction,
            },
        );
    };
    let bin = forge_toolchain::fbuild::install(&dir, &cancel, |l| say(&app, l))
        .await
        .map_err(|e| e.to_string())?;
    let board =
        forge_toolchain::fbuild::board_for("esp32:esp32:esp32").ok_or("Kein Board für fbuild.")?;
    let work = state.toolchain.cache_base().join("fbuild-prefetch");
    forge_toolchain::fbuild::prefetch(&bin, &work, &board, &cancel, |l| say(&app, l))
        .await
        .map_err(|e| e.to_string())
}
