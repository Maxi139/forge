use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

/// Fehlertexte sind für Nutzer gedacht und sagen, was jetzt hilft.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("arduino-cli wurde nicht gefunden. Installiere es mit „brew install arduino-cli“ und starte Forge neu.")]
    ArduinoCliMissing,
    #[error(
        "esptool wurde nicht gefunden. Installiere den ESP32-Core über „Bibliotheken und Boards“."
    )]
    EsptoolMissing,
    #[error("{tool} konnte nicht gestartet werden: {source}")]
    Spawn {
        tool: String,
        source: std::io::Error,
    },
    #[error("{0}")]
    Failed(String),
    #[error("Die Antwort von {tool} war unerwartet: {detail}")]
    Parse { tool: &'static str, detail: String },
    #[error("Abgebrochen.")]
    Cancelled,
    #[error("Zeitüberschreitung bei {0}.")]
    Timeout(String),
    #[error("Dateizugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),
    #[error("Ordner nicht gefunden: {0}")]
    NotFound(PathBuf),
}
