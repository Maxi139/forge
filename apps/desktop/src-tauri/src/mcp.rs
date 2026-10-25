//! MCP-Anbindung: macht Projekt, Build, Flash und Monitor für Claude Code erreichbar.

use crate::commands::ai::key as openrouter_key;
use crate::commands::build::{
    analysis_run, check_run, flash_run, run_build, start_monitor, BoardArg, FlashArgs, FlashResult,
};
use crate::commands::recents;
use crate::commands::secrets::redactor;
use crate::state::{AppState, MonitorSpec};
use crate::store::data_file;
use async_trait::async_trait;
use forge_devices::{list_devices, DeviceInfo};
use forge_mcp::{serve, write_info, Host, ServerInfo, ToolDef};
use forge_project::ProjectInfo;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, State};

const PREFERRED_PORT: u16 = 47615;
const SETTINGS: &str = "mcp-settings.json";
const INFO: &str = "mcp.json";
const MAX_DIAGNOSTICS: usize = 60;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Settings {
    enabled: bool,
    token: Option<String>,
}

fn settings(app: &AppHandle) -> Settings {
    data_file(app, SETTINGS)
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, s: &Settings) {
    if let Some(p) = data_file(app, SETTINGS) {
        let _ = std::fs::write(p, serde_json::to_vec_pretty(s).unwrap_or_default());
    }
}

fn info_path(app: &AppHandle) -> Option<PathBuf> {
    data_file(app, INFO)
}

fn new_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Startet den Server, wenn die Verbindung erlaubt ist. Ein laufender Server bleibt.
pub async fn start(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state
        .mcp
        .lock()
        .map_err(|_| "Zustand gesperrt".to_string())?
        .is_some()
    {
        return Ok(());
    }
    let mut s = settings(app);
    let token = s.token.clone().unwrap_or_else(new_token);
    s.token = Some(token.clone());
    save_settings(app, &s);
    let server = serve(
        std::sync::Arc::new(ForgeHost { app: app.clone() }),
        PREFERRED_PORT,
        token.clone(),
    )
    .await
    .map_err(|e| e.to_string())?;
    if let Some(p) = info_path(app) {
        write_info(
            &p,
            &ServerInfo {
                port: server.port,
                token,
            },
        )
        .map_err(|e| e.to_string())?;
    }
    *state
        .mcp
        .lock()
        .map_err(|_| "Zustand gesperrt".to_string())? = Some(server);
    Ok(())
}

pub fn stop(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Ok(mut g) = state.mcp.lock() {
        g.take();
    }
    if let Some(p) = info_path(app) {
        let _ = std::fs::remove_file(p);
    }
}

/// Beim Start: läuft nur, wenn der Nutzer die Verbindung erlaubt hat.
pub fn start_if_enabled(app: &AppHandle) {
    if settings(app).enabled {
        let handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = start(&handle).await {
                eprintln!("MCP-Server startet nicht: {e}");
            }
        });
    }
}

/// Die stdio-Brücke, aufgerufen mit `--mcp-bridge --info <Datei>`.
pub fn bridge(info: Option<PathBuf>) {
    let Some(info) = info else {
        eprintln!("Aufruf: forge-desktop --mcp-bridge --info <Datei>");
        std::process::exit(2);
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Laufzeit");
    rt.block_on(forge_mcp::bridge::run(&info));
}

/* ---------- Befehle für die Oberfläche ---------- */

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub port: Option<u16>,
    /// `.mcp.json` des Projekts nennt Forge.
    pub connected: bool,
}

fn mcp_json_has_forge(root: &Path) -> bool {
    std::fs::read(root.join(".mcp.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .is_some_and(|v| v["mcpServers"]["forge"].is_object())
}

#[tauri::command]
pub fn mcp_status(app: AppHandle, state: State<AppState>) -> McpStatus {
    let port = state
        .mcp
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| s.port));
    McpStatus {
        enabled: settings(&app).enabled,
        running: port.is_some(),
        port,
        connected: state.root().ok().is_some_and(|r| mcp_json_has_forge(&r)),
    }
}

#[tauri::command]
pub async fn mcp_set_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut s = settings(&app);
    s.enabled = enabled;
    save_settings(&app, &s);
    if enabled {
        start(&app).await
    } else {
        stop(&app);
        Ok(())
    }
}

/// Trägt Forge in die `.mcp.json` des Projekts ein. Darin steht nur der Aufruf der Brücke, nie ein Token.
#[tauri::command]
pub async fn mcp_connect(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let root = state.root()?;
    mcp_set_enabled(app.clone(), true).await?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let info = info_path(&app).ok_or("Der Speicherort der App ist nicht erreichbar.")?;
    let path = root.join(".mcp.json");
    let mut doc: Value = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_else(|| json!({}));
    if !doc.is_object() {
        doc = json!({});
    }
    if !doc["mcpServers"].is_object() {
        doc["mcpServers"] = json!({});
    }
    doc["mcpServers"]["forge"] =
        json!({ "command": exe, "args": ["--mcp-bridge", "--info", info] });
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    // Der Pfad gilt nur für diesen Rechner. Er gehört nicht ins Repository.
    let ignore = root.join(".gitignore");
    if let Ok(text) = std::fs::read_to_string(&ignore) {
        if !text.lines().any(|l| l.trim() == ".mcp.json") {
            let sep = if text.ends_with('\n') || text.is_empty() {
                ""
            } else {
                "\n"
            };
            let _ = std::fs::write(&ignore, format!("{text}{sep}.mcp.json\n"));
        }
    }
    Ok(())
}

/// Öffnet Claude Code im Terminal, im Ordner des Projekts.
#[tauri::command]
pub fn mcp_open_in_claude(state: State<AppState>) -> Result<(), String> {
    let root = state.root()?;
    let script = format!("tell application \"Terminal\"\nactivate\ndo script \"cd \" & (quoted form of {}) & \" && claude\"\nend tell", applescript_string(&root.to_string_lossy()));
    let out = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err("Terminal ließ sich nicht öffnen. Erlaube Forge die Steuerung von Terminal in den Systemeinstellungen.".into())
    }
}

fn applescript_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/* ---------- Werkzeuge ---------- */

pub struct ForgeHost {
    app: AppHandle,
}

impl ForgeHost {
    fn state(&self) -> State<'_, AppState> {
        self.app.state::<AppState>()
    }

    fn project(&self) -> Result<ProjectInfo, String> {
        let root = self.state().root()?;
        forge_project::open(&root).map_err(|e| e.to_string())
    }

    fn board(&self) -> Result<BoardArg, String> {
        let p = self.project()?;
        Ok(BoardArg {
            fqbn: p.config.board.fqbn,
            options: p.config.board.options,
        })
    }

    /// Das einzige passende Gerät, oder der Grund, warum keines gewählt werden kann.
    fn pick_device(&self, port: Option<&str>) -> Result<DeviceInfo, String> {
        let all = list_devices();
        if let Some(port) = port {
            return all
                .into_iter()
                .find(|d| d.port == port)
                .ok_or_else(|| format!("Am Port {port} hängt kein Gerät."));
        }
        let mut esp: Vec<DeviceInfo> = all.iter().filter(|d| d.likely_esp).cloned().collect();
        match esp.len() {
            1 => Ok(esp.remove(0)),
            0 => Err("Kein ESP32 angeschlossen.".into()),
            _ => Err(format!(
                "Mehrere Geräte angeschlossen. Gib einen Port an: {}",
                esp.iter()
                    .map(|d| d.port.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        }
    }

    async fn ensure_monitor(&self, baud: u32) -> Result<(), String> {
        let running = self
            .state()
            .devices
            .monitor
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .is_some();
        if running {
            return Ok(());
        }
        let device = self.pick_device(None)?;
        start_monitor(
            &self.app,
            &self.state(),
            MonitorSpec {
                device_id: Some(device.id),
                port: device.port,
                baud,
            },
        );
        tokio::time::sleep(Duration::from_millis(1500)).await;
        Ok(())
    }

    fn relative(&self, file: &str) -> String {
        match self.state().root() {
            Ok(root) => Path::new(file)
                .strip_prefix(&root)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| file.to_string()),
            Err(_) => file.to_string(),
        }
    }

    fn diagnostics_json(&self, list: &[forge_core::Diagnostic]) -> Vec<Value> {
        list.iter()
            .take(MAX_DIAGNOSTICS)
            .map(|d| {
                let mut v = serde_json::to_value(d).unwrap_or(Value::Null);
                if let Some(f) = v["file"].as_str() {
                    v["file"] = Value::String(self.relative(f));
                }
                // Die Spalten sind für den Editor. Für Claude zählen Zeile, Meldung und Behebung.
                if let Some(o) = v.as_object_mut() {
                    for k in ["from", "to", "id", "action"] {
                        o.remove(k);
                    }
                }
                v
            })
            .collect()
    }

    async fn t_diagnostics(&self) -> Result<String, String> {
        let board = self.board()?;
        let compiled = check_run(self.app.state(), board.clone(), HashMap::new()).await?;
        let rules = analysis_run(self.app.state(), board, HashMap::new()).await?;
        let mut all = compiled.diagnostics.clone();
        all.extend(rules.diagnostics);
        let errors = all
            .iter()
            .filter(|d| format!("{:?}", d.severity).eq_ignore_ascii_case("error"))
            .count();
        Ok(json!({
            "errors": errors,
            "total": all.len(),
            "diagnostics": self.diagnostics_json(&all),
            "note": compiled.note,
        })
        .to_string())
    }

    async fn t_build(&self, args: &Value) -> Result<String, String> {
        let board = self.board()?;
        let force = args["force"].as_bool().unwrap_or(false);
        let outcome = run_build(&self.app, &self.state(), &board, force).await?;
        Ok(json!({
            "status": outcome.status,
            "backend": outcome.backend,
            "durationMs": outcome.duration_ms,
            "sizes": outcome.sizes,
            "diagnostics": self.diagnostics_json(&outcome.diagnostics),
            "error": outcome.error,
        })
        .to_string())
    }

    async fn t_flash(&self, args: &Value) -> Result<String, String> {
        let board = self.board()?;
        let device = self.pick_device(args["port"].as_str())?;
        let result = flash_run(
            self.app.clone(),
            self.app.state(),
            FlashArgs {
                board,
                port: device.port.clone(),
                device_id: Some(device.id.clone()),
                erase_all: args["eraseAll"].as_bool().unwrap_or(false),
                force: args["force"].as_bool().unwrap_or(false),
            },
        )
        .await?;
        Ok(match result {
            FlashResult::Flashed { flash, .. } => json!({ "status": "flashed", "port": device.port, "flash": flash }),
            FlashResult::AlreadyCurrent { .. } => json!({ "status": "alreadyCurrent", "port": device.port }),
            FlashResult::BuildFailed { build } => json!({ "status": "buildFailed", "diagnostics": self.diagnostics_json(&build.diagnostics), "error": build.error }),
            FlashResult::SecretsMissing { names } => json!({ "status": "secretsMissing", "names": names, "hint": "Die Werte lassen sich in Forge unter Secrets eintragen." }),
        }
        .to_string())
    }

    fn t_devices(&self) -> Result<String, String> {
        serde_json::to_string(&list_devices()).map_err(|e| e.to_string())
    }

    async fn t_serial_read(&self, args: &Value) -> Result<String, String> {
        self.ensure_monitor(args["baud"].as_u64().unwrap_or(115_200) as u32)
            .await?;
        let n = args["lines"].as_u64().unwrap_or(100).clamp(1, 1000) as usize;
        let log = self
            .state()
            .devices
            .log
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone();
        let tail: Vec<&String> = log
            .iter()
            .rev()
            .take(n)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if tail.is_empty() {
            return Ok("Noch keine Ausgabe.".into());
        }
        Ok(tail
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n"))
    }

    async fn t_serial_write(&self, args: &Value) -> Result<String, String> {
        let text = args["text"].as_str().ok_or("text fehlt")?;
        self.ensure_monitor(115_200).await?;
        let suffix = match args["ending"].as_str() {
            Some("none") => "",
            Some("crlf") => "\r\n",
            _ => "\n",
        };
        let state = self.state();
        let g = state
            .devices
            .monitor
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?;
        let (monitor, _) = g.as_ref().ok_or("Der Monitor ist nicht verbunden.")?;
        monitor.write(format!("{text}{suffix}").into_bytes());
        Ok("Gesendet.".into())
    }

    async fn t_board(&self) -> Result<String, String> {
        let p = self.project()?;
        let board = BoardArg {
            fqbn: p.config.board.fqbn.clone(),
            options: p.config.board.options.clone(),
        };
        let full = forge_toolchain::models::fqbn_with_options(&board.fqbn, &board.options);
        let props = self
            .state()
            .toolchain
            .cli()?
            .board_properties(&full)
            .await
            .map_err(|e| e.to_string())?;
        let pick: serde_json::Map<String, Value> = [
            "build.mcu",
            "build.f_cpu",
            "build.flash_size",
            "build.partitions",
            "build.psram_type",
            "upload.speed",
        ]
        .iter()
        .filter_map(|k| props.get(*k).map(|v| (k.to_string(), json!(v))))
        .collect();
        Ok(json!({ "project": p.name, "fqbn": full, "options": p.config.board.options, "properties": pick, "devices": list_devices().into_iter().filter(|d| d.likely_esp).collect::<Vec<_>>() }).to_string())
    }

    fn t_memory(&self) -> Result<String, String> {
        let last = self
            .state()
            .last_build
            .lock()
            .map_err(|_| "Zustand gesperrt".to_string())?
            .clone();
        let Some(b) = last else {
            return Err("Es gibt noch keinen Build. Führe zuerst build aus.".into());
        };
        let sizes: Vec<Value> = b.sizes.iter().map(|s| json!({ "label": s.label, "used": s.used, "total": s.total, "percent": if s.total > 0 { (s.used as f64 / s.total as f64 * 1000.0).round() / 10.0 } else { 0.0 } })).collect();
        Ok(json!({ "sizes": sizes, "partitions": b.partitions, "status": b.status }).to_string())
    }

    fn t_projects(&self) -> Result<String, String> {
        let recent: Vec<Value> = recents::recent_list(self.app.clone())
            .into_iter()
            .map(|r| json!({ "name": r.name, "path": r.path, "kind": r.kind }))
            .collect();
        let current = self.project().ok().map(|p| json!({ "name": p.name, "path": p.path, "kind": p.kind, "board": p.config.board.fqbn }));
        Ok(json!({ "current": current, "recent": recent }).to_string())
    }
}

fn tool(name: &str, description: &str, read_only: bool, props: Value) -> ToolDef {
    ToolDef::new(name, description, read_only, props)
}

#[async_trait]
impl Host for ForgeHost {
    fn tools(&self) -> Vec<ToolDef> {
        vec![
            tool("list_projects", "Das in Forge geöffnete Projekt und die zuletzt geöffneten. Alle weiteren Werkzeuge beziehen sich auf das geöffnete Projekt.", true, json!({})),
            tool("get_diagnostics", "Prüft das geöffnete Projekt mit dem Compiler und den Regeln von Forge (tote Funktionen, Interrupt-Fallen, Pinregeln, fest eingetragene Passwörter). Gibt Fehler und Warnungen mit Datei, Zeile und möglicher Behebung zurück.", true, json!({})),
            tool("build", "Kompiliert das Projekt für das eingestellte Board. Unveränderter Code wird übersprungen.", false, json!({ "force": { "type": "boolean", "description": "Auch bei unverändertem Code neu kompilieren." } })),
            tool(
                "flash",
                "Kompiliert bei Bedarf und flasht das Projekt auf den angeschlossenen ESP32. Secrets kommen aus dem Schlüsselbund und werden nie ausgegeben.",
                false,
                json!({ "port": { "type": "string", "description": "Serieller Port. Weglassen, wenn genau ein ESP32 angeschlossen ist." }, "eraseAll": { "type": "boolean" }, "force": { "type": "boolean", "description": "Auch flashen, wenn das Board schon diesen Stand hat." } }),
            ),
            tool("list_devices", "Angeschlossene serielle Geräte, ESP32 zuerst erkannt.", true, json!({})),
            tool(
                "serial_read",
                "Die letzten Zeilen der seriellen Ausgabe. Verbindet den Monitor, falls er nicht läuft.",
                true,
                json!({ "lines": { "type": "integer", "minimum": 1, "maximum": 1000, "description": "Anzahl der Zeilen, Standard 100." }, "baud": { "type": "integer", "description": "Baudrate, falls der Monitor erst verbunden wird. Standard 115200." } }),
            ),
            tool("serial_write", "Sendet Text an das Gerät.", false, json!({ "text": { "type": "string" }, "ending": { "type": "string", "enum": ["lf", "crlf", "none"], "description": "Zeilenende, Standard lf." } })),
            tool("board_info", "Board, Optionen, Chip und angeschlossene Geräte des geöffneten Projekts.", true, json!({})),
            tool("memory_usage", "Speicherverbrauch und Partitionen des letzten Builds.", true, json!({})),
        ]
    }

    async fn call(&self, tool: &str, args: Value) -> Result<String, String> {
        match tool {
            "list_projects" => self.t_projects(),
            "get_diagnostics" => self.t_diagnostics().await,
            "build" => self.t_build(&args).await,
            "flash" => self.t_flash(&args).await,
            "list_devices" => self.t_devices(),
            "serial_read" => self.t_serial_read(&args).await,
            "serial_write" => self.t_serial_write(&args).await,
            "board_info" => self.t_board().await,
            "memory_usage" => self.t_memory(),
            other => Err(format!("Unbekanntes Werkzeug: {other}")),
        }
    }

    /// Schlüsselbundwerte und der OpenRouter-Schlüssel verlassen den Rechner nie, auch nicht über MCP.
    fn redact(&self, text: &str) -> String {
        let Ok(root) = self.state().root() else {
            return text.to_string();
        };
        let extra = openrouter_key()
            .map(|k| vec![("OPENROUTER_KEY".to_string(), k)])
            .unwrap_or_default();
        redactor(&root, extra).apply(text)
    }
}
