//! Protokoll: eine Datei unter `~/Library/Logs/cc.forge.ide`, dazu der Panic-Hook.

use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_log::{Builder, RotationStrategy, Target, TargetKind};

const DATEI: &str = "forge";
const MAX_BYTES: u128 = 1_000_000;

/// Das Plugin schreibt in die Logdatei, rotiert bei 1 MB und behält nur die letzte alte Datei.
pub fn plugin() -> tauri::plugin::TauriPlugin<Wry> {
    let mut targets = vec![Target::new(TargetKind::LogDir {
        file_name: Some(DATEI.into()),
    })];
    if cfg!(debug_assertions) {
        targets.push(Target::new(TargetKind::Stdout));
    }
    Builder::new()
        .targets(targets)
        .max_file_size(MAX_BYTES)
        .rotation_strategy(RotationStrategy::KeepOne)
        .level(if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .build()
}

/// Panics landen mit Ort und Thread in der Logdatei, danach läuft der bisherige Hook weiter.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        log::error!(
            "Panic im Thread „{}“: {info}\n{}",
            thread.name().unwrap_or("?"),
            std::backtrace::Backtrace::force_capture()
        );
        log::logger().flush();
        previous(info);
    }));
}

/// Zeigt die Logdatei im Finder (der Ordner, falls es noch keine Datei gibt).
pub fn reveal(app: &AppHandle) {
    let Ok(dir) = app.path().app_log_dir() else {
        return;
    };
    let file = dir.join(format!("{DATEI}.log"));
    let result = if file.exists() {
        tauri_plugin_opener::reveal_item_in_dir(&file)
    } else {
        let _ = std::fs::create_dir_all(&dir);
        tauri_plugin_opener::reveal_item_in_dir(&dir)
    };
    if let Err(e) = result {
        log::warn!("Protokoll lässt sich nicht im Finder zeigen: {e}");
    }
}
