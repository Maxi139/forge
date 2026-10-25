//! Selbstaktualisierung über `tauri-plugin-updater`. Ohne Schlüssel in `tauri.conf.json` (siehe docs/release.md)
//! bleibt alles stumm: Die Prüfung meldet dann nur „nicht eingerichtet“.

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
}

/// Der öffentliche Schlüssel und mindestens eine Adresse müssen in `plugins.updater` stehen.
fn configured(app: &AppHandle) -> bool {
    let Some(cfg) = app.config().plugins.0.get("updater") else {
        return false;
    };
    let pubkey = cfg.get("pubkey").and_then(|v| v.as_str()).unwrap_or("");
    let endpoints = cfg
        .get("endpoints")
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());
    !pubkey.trim().is_empty() && endpoints
}

async fn find(app: &AppHandle) -> Result<Option<tauri_plugin_updater::Update>, String> {
    if !configured(app) {
        return Err("Updates sind in dieser Version nicht eingerichtet.".into());
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    updater.check().await.map_err(|e| {
        log::warn!("Update-Prüfung fehlgeschlagen: {e}");
        e.to_string()
    })
}

/// Fragt den Update-Server, ob es eine neuere Version gibt.
#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, String> {
    Ok(find(&app).await?.map(|u| UpdateInfo {
        version: u.version,
        notes: u.body,
    }))
}

/// Lädt die neue Version, prüft die Signatur und ersetzt die App. Neu gestartet wird erst auf Wunsch.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let Some(update) = find(&app).await? else {
        return Err("Es gibt kein Update mehr.".into());
    };
    log::info!("Installiere Update {}", update.version);
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| {
            log::error!("Update-Installation fehlgeschlagen: {e}");
            e.to_string()
        })
}

#[tauri::command]
pub fn update_restart(app: AppHandle) {
    app.restart();
}
