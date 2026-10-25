use super::build::{start_monitor, stop_monitor};
use crate::state::{AppState, MonitorSpec};
use forge_devices::{list_devices, probe_chip, ChipProbe, DeviceInfo};
use forge_secrets::keychain::SecretStore;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

/// USB-Geräte und Boards im WLAN in einer Liste.
fn with_network(state: &AppState, mut usb: Vec<DeviceInfo>) -> Vec<DeviceInfo> {
    if let Ok(g) = state.devices.network.lock() {
        usb.extend(g.iter().cloned());
    }
    usb
}

#[tauri::command]
pub fn devices_list(state: State<AppState>) -> Vec<DeviceInfo> {
    let fresh = list_devices();
    if let Ok(mut g) = state.devices.list.lock() {
        *g = fresh.clone();
    }
    with_network(&state, fresh)
}

/// Startet die Beobachtung der USB-Anschlüsse. Ändert sich die Liste, kommt das Ereignis `devices:changed`.
pub fn start_device_watch(app: &AppHandle, state: &AppState) {
    let handle = app.clone();
    let w = forge_devices::watch(move || {
        let fresh = list_devices();
        let changed = {
            let state = tauri::Manager::state::<AppState>(&handle);
            let Ok(mut g) = state.devices.list.lock() else {
                return;
            };
            let changed = *g != fresh;
            if changed {
                *g = fresh.clone();
            }
            changed
        };
        if changed {
            let state = tauri::Manager::state::<AppState>(&handle);
            let _ = handle.emit("devices:changed", with_network(&state, fresh));
        }
    });
    if let Ok(mut g) = state.devices.watch.lock() {
        *g = Some(w);
    }
    // Boards im WLAN melden sich per mDNS an und ab.
    let handle = app.clone();
    let browser = forge_devices::browse_ota(move |found| {
        let state = tauri::Manager::state::<AppState>(&handle);
        if let Ok(mut g) = state.devices.network.lock() {
            *g = found.iter().map(DeviceInfo::from_network).collect();
        }
        let usb = state
            .devices
            .list
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default();
        let _ = handle.emit("devices:changed", with_network(&state, usb));
    });
    if let Ok(mut g) = state.devices.ota.lock() {
        *g = browser;
    }
}

/// Fragt den Chip über esptool. Das startet das Board neu, deshalb nur auf Wunsch oder bei neuen Geräten.
#[tauri::command]
pub async fn device_probe(state: State<'_, AppState>, port: String) -> Result<ChipProbe, String> {
    let esptool = state.toolchain.esptool().await?;
    // Hält der Monitor den Port, lässt er sich nicht abfragen.
    let held = state
        .devices
        .monitor
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|(_, s)| s.port == port))
        .unwrap_or(false);
    if held {
        return Err("Der Monitor hält den Port. Schließe ihn, um den Chip abzufragen.".into());
    }
    tokio::task::spawn_blocking(move || {
        probe_chip(&esptool, &port, Duration::from_secs(12)).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn monitor_start(
    app: AppHandle,
    state: State<AppState>,
    device_id: Option<String>,
    port: String,
    baud: u32,
) {
    start_monitor(
        &app,
        &state,
        MonitorSpec {
            device_id,
            port,
            baud,
        },
    );
}

#[tauri::command]
pub fn monitor_stop(state: State<AppState>) {
    let _ = stop_monitor(&state);
}

#[tauri::command]
pub fn monitor_write(state: State<AppState>, text: String, ending: String) -> Result<(), String> {
    let g = state
        .devices
        .monitor
        .lock()
        .map_err(|_| "Zustand gesperrt".to_string())?;
    let (monitor, _) = g.as_ref().ok_or("Der Monitor ist nicht verbunden.")?;
    let suffix = match ending.as_str() {
        "none" => "",
        "cr" => "\r",
        "nlcr" => "\r\n",
        _ => "\n",
    };
    monitor.write(format!("{text}{suffix}").into_bytes());
    Ok(())
}

/// Startet das Board über DTR/RTS neu, ohne den Monitor zu schließen.
#[tauri::command]
pub fn monitor_reset(state: State<AppState>) -> Result<(), String> {
    let g = state
        .devices
        .monitor
        .lock()
        .map_err(|_| "Zustand gesperrt".to_string())?;
    let (monitor, _) = g.as_ref().ok_or("Der Monitor ist nicht verbunden.")?;
    monitor.reset();
    Ok(())
}

/// Ändert die Baudrate bei offenem Port. Nach einem Flash gilt die neue Rate weiter.
#[tauri::command]
pub fn monitor_set_baud(state: State<AppState>, baud: u32) -> Result<(), String> {
    if !(300..=4_000_000).contains(&baud) {
        return Err("Die Baudrate muss zwischen 300 und 4000000 liegen.".into());
    }
    let mut g = state
        .devices
        .monitor
        .lock()
        .map_err(|_| "Zustand gesperrt".to_string())?;
    let (monitor, spec) = g.as_mut().ok_or("Der Monitor ist nicht verbunden.")?;
    spec.baud = baud;
    monitor.set_baud(baud);
    Ok(())
}

fn ota_account(device_id: &str) -> String {
    format!("ota:{device_id}")
}

/// Ob für ein Netzwerkgerät ein OTA-Passwort im Schlüsselbund liegt.
#[tauri::command]
pub fn ota_password_status(device_id: String) -> bool {
    super::secrets::store()
        .get(&ota_account(&device_id))
        .ok()
        .flatten()
        .is_some()
}

/// Speichert das OTA-Passwort eines Geräts im Schlüsselbund, ohne Wert löscht es den Eintrag.
#[tauri::command]
pub fn ota_password_set(device_id: String, password: Option<String>) -> Result<(), String> {
    let store = super::secrets::store();
    let account = ota_account(&device_id);
    match password.filter(|p| !p.is_empty()) {
        Some(p) => store.set(&account, &p),
        None => store.delete(&account),
    }
    .map_err(|e| e.to_string())
}

pub(crate) fn ota_password(device_id: &str) -> Option<String> {
    super::secrets::store()
        .get(&ota_account(device_id))
        .ok()
        .flatten()
}

/// Schreibt einen Text an einen Ort, den der Nutzer im Speichern-Dialog gewählt hat (Monitor-Log, Plotter-CSV).
#[tauri::command]
pub fn export_write(path: std::path::PathBuf, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("Speichern fehlgeschlagen: {e}"))
}
