use super::build::{start_monitor, stop_monitor};
use crate::state::{AppState, MonitorSpec};
use forge_devices::{list_devices, probe_chip, ChipProbe, DeviceInfo};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

#[tauri::command]
pub fn devices_list(state: State<AppState>) -> Vec<DeviceInfo> {
    let fresh = list_devices();
    if let Ok(mut g) = state.devices.list.lock() {
        *g = fresh.clone();
    }
    fresh
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
            let _ = handle.emit("devices:changed", fresh);
        }
    });
    if let Ok(mut g) = state.devices.watch.lock() {
        *g = Some(w);
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
        "nlcr" => "\r\n",
        _ => "\n",
    };
    monitor.write(format!("{text}{suffix}").into_bytes());
    Ok(())
}
