use crate::watch::ProjectWatcher;
use forge_devices::{DeviceInfo, Monitor, WatchHandle};
use forge_toolchain::build::BuildOutcome;
use forge_toolchain::check::Checker;
use forge_toolchain::cli::ArduinoCli;
use forge_toolchain::models::LibraryInfo;
use forge_toolchain::tools::{find_esptool, Tools};
use forge_toolchain::CancellationToken;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Zustand, den alle Befehle teilen.
#[derive(Default)]
pub struct AppState {
    /// Der geöffnete Projektordner. Dateibefehle arbeiten nur innerhalb davon.
    pub root: Mutex<Option<PathBuf>>,
    pub watcher: ProjectWatcher,
    pub toolchain: Toolchain,
    pub devices: DeviceHub,
    pub jobs: Jobs,
    /// Pfad, mit dem die App gestartet oder geöffnet wurde. Die Oberfläche holt ihn beim Start ab.
    pub pending_open: Mutex<Option<PathBuf>>,
    /// Das vorgeschlagene Modell, nach dem ersten Abruf der Modellliste.
    pub default_model: Mutex<Option<String>>,
    /// Ergebnis des letzten Builds, für Speicheranzeige und MCP.
    pub last_build: Mutex<Option<BuildOutcome>>,
    /// clangd für das geöffnete Projekt, mit dem Schlüssel, für den die Sitzung gebaut wurde.
    pub lsp: tokio::sync::Mutex<Option<(String, std::sync::Arc<forge_lsp::Lsp>)>>,
    /// Der laufende MCP-Server.
    pub mcp: Mutex<Option<forge_mcp::Server>>,
    /// Das geöffnete Projekt, wenn es aus einer `.forgeproj`-Datei stammt.
    pub package: Mutex<Option<PackageSession>>,
    /// Zählt jede Änderung. Ein wartendes Speichern gilt nur, solange niemand weiter geändert hat.
    pub package_gen: std::sync::atomic::AtomicU64,
    /// Es gibt Änderungen, die noch nicht in der Datei stehen.
    pub package_dirty: std::sync::atomic::AtomicBool,
    /// Höchstens ein Speichern gleichzeitig.
    pub package_save: tokio::sync::Mutex<()>,
}

/// Ein Projekt, dessen Arbeitskopie (`root`) in eine `.forgeproj`-Datei gespeichert wird.
#[derive(Clone, Debug)]
pub struct PackageSession {
    pub package: PathBuf,
    pub root: PathBuf,
    /// Kennung, unter der der Arbeitsordner liegt (bei Sketches zugleich die Kennung der Secrets).
    pub id: String,
    /// Bibliotheken und Core, wie sie zuletzt gespeichert wurden. Rückfall, wenn `arduino-cli` fehlt, und
    /// Merkzettel dafür, welche Bibliothek schon als eingebettet erkannt wurde.
    pub libraries: Vec<forge_package::Library>,
    pub cores: std::collections::BTreeMap<String, String>,
}

impl AppState {
    pub fn root(&self) -> Result<PathBuf, String> {
        self.root
            .lock()
            .map_err(|_| "Interner Fehler: Zustand gesperrt.".to_string())?
            .clone()
            .ok_or_else(|| "Es ist kein Projekt geöffnet.".to_string())
    }

    pub fn set_root(&self, path: PathBuf) {
        if let Ok(mut g) = self.root.lock() {
            *g = Some(path);
        }
    }
}

/// Werkzeuge, beim ersten Gebrauch gesucht.
#[derive(Default)]
pub struct Toolchain {
    pub cache_base: Mutex<Option<PathBuf>>,
    pub own_tools: Mutex<Vec<PathBuf>>,
    cli: Mutex<Option<ArduinoCli>>,
    checker: Mutex<Option<Arc<Checker>>>,
    data_dir: Mutex<Option<PathBuf>>,
    meta: Mutex<Option<Meta>>,
}

/// Versionen, die in den Build-Hash eingehen. Wird nach Installationen verworfen.
#[derive(Clone)]
pub struct Meta {
    pub cli_version: String,
    pub core_versions: HashMap<String, String>,
    pub libraries: Vec<LibraryInfo>,
}

impl Meta {
    pub fn library_pairs(&self) -> Vec<(String, String)> {
        self.libraries
            .iter()
            .map(|l| (l.name.clone(), l.version.clone()))
            .collect()
    }

    /// Core-Version des Boards, z. B. `3.3.12` für `esp32:esp32:esp32s3`.
    pub fn core_version(&self, fqbn: &str) -> String {
        let id = fqbn.splitn(3, ':').take(2).collect::<Vec<_>>().join(":");
        self.core_versions.get(&id).cloned().unwrap_or_default()
    }
}

impl Toolchain {
    pub async fn meta(&self) -> Result<Meta, String> {
        if let Some(m) = self
            .meta
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone()
        {
            return Ok(m);
        }
        let cli = self.cli()?;
        let cli_version = cli.version().await.map_err(|e| e.to_string())?;
        let core_versions = cli
            .cores()
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter_map(|c| c.installed.map(|v| (c.id, v)))
            .collect();
        let libraries = cli.libraries().await.map_err(|e| e.to_string())?;
        let meta = Meta {
            cli_version,
            core_versions,
            libraries,
        };
        *self
            .meta
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())? = Some(meta.clone());
        Ok(meta)
    }

    /// Nach Installation oder Entfernung von Cores und Bibliotheken.
    pub fn invalidate(&self) {
        if let Ok(mut g) = self.meta.lock() {
            *g = None;
        }
        if let Ok(g) = self.checker.lock() {
            if let Some(c) = g.clone() {
                tauri::async_runtime::spawn(async move { c.forget().await });
            }
        }
    }

    pub fn cache_base(&self) -> PathBuf {
        self.cache_base
            .lock()
            .ok()
            .and_then(|g| g.clone())
            .unwrap_or_else(|| std::env::temp_dir().join("forge-cache"))
    }

    pub fn cli(&self) -> Result<ArduinoCli, String> {
        if let Some(c) = self
            .cli
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone()
        {
            return Ok(c);
        }
        let dirs = self.own_tools.lock().map(|g| g.clone()).unwrap_or_default();
        let tools = Tools::locate(&dirs).map_err(|e| e.to_string())?;
        let cli = ArduinoCli::new(tools.arduino_cli);
        *self
            .cli
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())? = Some(cli.clone());
        Ok(cli)
    }

    pub fn checker(&self) -> Result<Arc<Checker>, String> {
        if let Some(c) = self
            .checker
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone()
        {
            return Ok(c);
        }
        let checker = Arc::new(Checker::new(self.cli()?, self.cache_base()));
        *self
            .checker
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())? = Some(checker.clone());
        Ok(checker)
    }

    pub async fn data_dir(&self) -> Result<PathBuf, String> {
        if let Some(d) = self
            .data_dir
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone()
        {
            return Ok(d);
        }
        let dir = self.cli()?.data_dir().await.map_err(|e| e.to_string())?;
        *self
            .data_dir
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())? = Some(dir.clone());
        Ok(dir)
    }

    pub async fn esptool(&self) -> Result<PathBuf, String> {
        let data = self.data_dir().await?;
        find_esptool(&data).ok_or_else(|| forge_toolchain::Error::EsptoolMissing.to_string())
    }
}

pub struct MonitorSpec {
    pub device_id: Option<String>,
    pub port: String,
    pub baud: u32,
}

#[derive(Default)]
pub struct DeviceHub {
    pub list: Mutex<Vec<DeviceInfo>>,
    pub watch: Mutex<Option<WatchHandle>>,
    pub monitor: Mutex<Option<(Monitor, MonitorSpec)>>,
    /// Die letzten Zeilen des Monitors, auch für MCP.
    pub log: Mutex<VecDeque<String>>,
    /// Boards im WLAN, die OTA anbieten.
    pub network: Mutex<Vec<DeviceInfo>>,
    pub ota: Mutex<Option<forge_devices::OtaBrowser>>,
}

/// Laufende Aufträge, die sich abbrechen lassen.
#[derive(Default)]
pub struct Jobs {
    pub check: Mutex<Option<CancellationToken>>,
    pub build: Mutex<Option<CancellationToken>>,
    pub library: Mutex<Option<CancellationToken>>,
    pub ai: Mutex<Option<CancellationToken>>,
}

impl Jobs {
    /// Startet einen neuen Auftrag und bricht den vorherigen derselben Art ab.
    pub fn begin(slot: &Mutex<Option<CancellationToken>>) -> CancellationToken {
        let token = CancellationToken::new();
        if let Ok(mut g) = slot.lock() {
            if let Some(old) = g.replace(token.clone()) {
                old.cancel();
            }
        }
        token
    }

    pub fn cancel(slot: &Mutex<Option<CancellationToken>>) {
        if let Ok(mut g) = slot.lock() {
            if let Some(t) = g.take() {
                t.cancel();
            }
        }
    }
}
