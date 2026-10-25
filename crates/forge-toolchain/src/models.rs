use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardEntry {
    pub name: String,
    pub fqbn: String,
    /// `esp32:esp32`
    pub platform: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardOptionValue {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardOption {
    pub id: String,
    pub label: String,
    pub values: Vec<BoardOptionValue>,
    /// Standardwert des Boards.
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreInfo {
    pub id: String,
    pub name: String,
    pub installed: Option<String>,
    pub latest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryInfo {
    pub name: String,
    pub version: String,
    pub author: String,
    pub sentence: String,
    pub website: String,
    pub location: String,
    /// Ordner der Installation.
    #[serde(skip)]
    pub install_dir: Option<std::path::PathBuf>,
    pub provides_includes: Vec<String>,
    /// Neueste verfügbare Version, falls neuer als installiert.
    pub latest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySearchHit {
    pub name: String,
    pub latest: String,
    pub author: String,
    pub sentence: String,
    pub website: String,
    pub versions: Vec<String>,
    pub installed: Option<String>,
}

/// Bis die Version `a` kleiner als `b` ist, vergleichen wir numerisch segmentweise.
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| -> Vec<u64> {
        s.split(|c: char| !c.is_ascii_digit())
            .filter(|x| !x.is_empty())
            .map(|x| x.parse().unwrap_or(0))
            .collect()
    };
    key(a).cmp(&key(b))
}

/// `esp32:esp32:esp32s3` und `FlashSize=16M` ergeben `esp32:esp32:esp32s3:FlashSize=16M`.
pub fn fqbn_with_options(fqbn: &str, options: &BTreeMap<String, String>) -> String {
    let opts: Vec<String> = options
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if opts.is_empty() {
        return fqbn.to_string();
    }
    let sep = if fqbn.split(':').count() >= 4 {
        ","
    } else {
        ":"
    };
    format!("{fqbn}{sep}{}", opts.join(","))
}

/* ---------- Rohformen aus arduino-cli --json ---------- */

#[derive(Deserialize, Default)]
pub(crate) struct RawBoardList {
    #[serde(default)]
    pub boards: Vec<RawBoard>,
}

#[derive(Deserialize)]
pub(crate) struct RawBoard {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub fqbn: String,
    #[serde(default)]
    pub platform: RawPlatformRef,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawPlatformRef {
    #[serde(default)]
    pub metadata: RawPlatformMeta,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawPlatformMeta {
    #[serde(default)]
    pub id: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawBoardDetails {
    #[serde(default)]
    pub config_options: Vec<RawOption>,
}

#[derive(Deserialize)]
pub(crate) struct RawOption {
    pub option: String,
    #[serde(default)]
    pub option_label: String,
    #[serde(default)]
    pub values: Vec<RawOptionValue>,
}

#[derive(Deserialize)]
pub(crate) struct RawOptionValue {
    pub value: String,
    #[serde(default)]
    pub value_label: String,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawCoreList {
    #[serde(default)]
    pub platforms: Vec<RawCore>,
}

#[derive(Deserialize)]
pub(crate) struct RawCore {
    pub id: String,
    #[serde(default)]
    pub installed_version: String,
    #[serde(default)]
    pub latest_version: String,
    #[serde(default)]
    pub releases: BTreeMap<String, RawCoreRelease>,
}

#[derive(Deserialize)]
pub(crate) struct RawCoreRelease {
    #[serde(default)]
    pub name: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawLibList {
    #[serde(default)]
    pub installed_libraries: Vec<RawInstalled>,
}

#[derive(Deserialize)]
pub(crate) struct RawInstalled {
    pub library: RawLib,
    #[serde(default)]
    pub release: Option<RawLib>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawLib {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub sentence: String,
    #[serde(default)]
    pub website: String,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub install_dir: String,
    #[serde(default)]
    pub provides_includes: Vec<String>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawLibSearch {
    #[serde(default)]
    pub libraries: Vec<RawSearchLib>,
}

#[derive(Deserialize)]
pub(crate) struct RawSearchLib {
    pub name: String,
    #[serde(default)]
    pub releases: BTreeMap<String, RawLib>,
    #[serde(default)]
    pub latest: Option<RawLib>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering::*;

    #[test]
    fn fqbn_gets_sorted_options() {
        let mut o = BTreeMap::new();
        assert_eq!(
            fqbn_with_options("esp32:esp32:esp32s3", &o),
            "esp32:esp32:esp32s3"
        );
        o.insert("PSRAM".into(), "opi".into());
        o.insert("FlashSize".into(), "16M".into());
        o.insert("Leer".into(), String::new());
        assert_eq!(
            fqbn_with_options("esp32:esp32:esp32s3", &o),
            "esp32:esp32:esp32s3:FlashSize=16M,PSRAM=opi"
        );
        assert_eq!(
            fqbn_with_options("esp32:esp32:esp32s3:USBMode=hwcdc", &o),
            "esp32:esp32:esp32s3:USBMode=hwcdc,FlashSize=16M,PSRAM=opi"
        );
    }

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(version_cmp("1.10.0", "1.9.9"), Greater);
        assert_eq!(version_cmp("3.3.12", "3.3.2"), Greater);
        assert_eq!(version_cmp("2.0", "2.0.0"), Less);
        assert_eq!(version_cmp("1.0.0", "1.0.0"), Equal);
    }
}
