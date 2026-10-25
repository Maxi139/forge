use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Project,
    Sketch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub fqbn: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

/// Woher eine Bibliothek kommt, wenn jemand das Projekt öffnet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LibrarySource {
    /// Aus dem Bibliotheksverwalter, genau in dieser Version.
    Registry,
    /// Nicht im Bibliotheksverwalter. Der Quelltext liegt in der Datei unter `libraries/<dir>`.
    Embedded,
    /// Kommt von Forge selbst (`ForgeSecrets`) und wird bei Bedarf automatisch angelegt.
    Forge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    pub version: String,
    pub source: LibrarySource,
    /// Nur bei `Embedded`: Ordnername unter `libraries/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    /// Installierte Cores, z. B. `esp32:esp32` zu `3.3.12`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cores: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiInfo {
    pub enabled: bool,
    /// Ob die Datei gespeicherte Prüfergebnisse enthält.
    #[serde(default)]
    pub cached: bool,
}

/// `manifest.json`: alles, was man über das Projekt wissen muss, ohne die Dateien zu entpacken.
/// Unbekannte Felder werden beim Lesen ignoriert, damit ältere Versionen neuere Dateien öffnen können,
/// solange sich `format` nicht erhöht hat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format: u32,
    pub kind: Kind,
    /// Stabile Kennung des Projekts. Unter ihr liegen die Secrets im Schlüsselbund.
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Zum Beispiel `Forge 0.1.0`.
    pub app: String,
    /// Sekunden seit 1970.
    pub saved_at: u64,
    pub board: Board,
    #[serde(default)]
    pub toolchain: Toolchain,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub libraries: Vec<Library>,
    /// Nur die Namen. Die Werte liegen im Schlüsselbund und gehören nie in die Datei.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
    pub ai: AiInfo,
}
