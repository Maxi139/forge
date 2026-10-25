use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, PackageError>;

/// Fehlertexte sind für Nutzer gedacht und sagen, was jetzt hilft.
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error(
        "{0} ist keine Forge-Projektdatei oder ist beschädigt. Lass dir die Datei erneut schicken."
    )]
    NotAPackage(PathBuf),
    #[error("Die Datei wurde mit einer neueren Forge-Version gespeichert (Format {0}). Aktualisiere Forge, um sie zu öffnen.")]
    TooNew(u32),
    #[error("Die Projektdatei enthält einen unsicheren Pfad: {0}")]
    UnsafePath(String),
    #[error("„{0}“ steckt nicht in der Projektdatei.")]
    MissingEntry(String),
    #[error("Das Projekt ist größer als {0} MB. Entferne große Dateien (Build-Ausgaben, Daten) und versuche es erneut.")]
    TooLarge(u64),
    #[error("Der Dateiname „{0}“ ist kein gültiger Text und kann nicht gespeichert werden.")]
    BadName(String),
    #[error("Die Bibliothek „{0}“ ist als eingebettet vermerkt, aber ihr Ordner fehlt.")]
    MissingLibrarySource(String),
    #[error("Dateizugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),
    #[error("Die Projektdatei konnte nicht geschrieben oder gelesen werden: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Die Beschreibung in der Projektdatei ist ungültig: {0}")]
    Manifest(#[from] serde_json::Error),
}
