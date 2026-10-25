use crate::{ProjectError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const DEFAULT_FQBN: &str = "esp32:esp32:esp32s3";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMeta {
    /// Stabile Kennung, unter der das Projekt seine Secrets im Schlüsselbund findet. Überlebt Umbenennen und Verschieben.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardConfig {
    pub fqbn: String,
    /// Menüoptionen des Boards, z. B. `FlashSize = "16M"`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

impl Default for BoardConfig {
    fn default() -> Self {
        Self {
            fqbn: DEFAULT_FQBN.into(),
            options: BTreeMap::new(),
        }
    }
}

/// Namen der Secrets. Die Werte liegen ausschließlich im Schlüsselbund.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretsConfig {
    #[serde(default)]
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConfig {
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl Default for AiConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForgeToml {
    pub project: ProjectMeta,
    #[serde(default)]
    pub board: BoardConfig,
    /// Bibliotheksname zu Version.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub libraries: BTreeMap<String, String>,
    #[serde(default)]
    pub secrets: SecretsConfig,
    #[serde(default)]
    pub ai: AiConfig,
}

impl ForgeToml {
    /// Standardwerte für einen Sketch ohne eigene `forge.toml`.
    pub fn for_sketch(name: &str) -> Self {
        Self {
            project: ProjectMeta {
                id: None,
                name: name.into(),
                description: String::new(),
            },
            board: BoardConfig::default(),
            libraries: BTreeMap::new(),
            secrets: SecretsConfig::default(),
            ai: AiConfig::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        toml::from_str(&text).map_err(|e| ProjectError::Config(e.message().to_string()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).map_err(|e| ProjectError::Config(e.to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = ForgeToml::for_sketch("demo");
        c.board.options.insert("FlashSize".into(), "16M".into());
        c.libraries
            .insert("Adafruit NeoPixel".into(), "1.12.3".into());
        c.secrets.names = vec!["WIFI_SSID".into(), "WIFI_PASS".into()];
        let text = toml::to_string_pretty(&c).unwrap();
        assert_eq!(toml::from_str::<ForgeToml>(&text).unwrap(), c);
    }

    #[test]
    fn minimal_file_gets_defaults() {
        let c: ForgeToml = toml::from_str("[project]\nname = \"x\"\n").unwrap();
        assert_eq!(c.board.fqbn, DEFAULT_FQBN);
        assert!(c.ai.enabled);
        assert!(c.secrets.names.is_empty());
    }
}
