use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, ProjectError>;

/// Fehlertexte sind für Nutzer gedacht und sagen, was jetzt hilft.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Ordner nicht gefunden: {0}")]
    NotFound(PathBuf),
    #[error("In {0} liegt keine .ino-Datei. Wähle einen Sketch-Ordner oder lege eine Datei an.")]
    NoSketch(PathBuf),
    #[error("{0}")]
    InvalidName(String),
    #[error("{0} existiert bereits. Wähle einen anderen Namen oder Ort.")]
    AlreadyExists(PathBuf),
    #[error("Pfad liegt außerhalb des Projekts: {0}")]
    OutsideProject(PathBuf),
    #[error("forge.toml ist ungültig: {0}")]
    Config(String),
    #[error("Dateizugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),
}
