use crate::models::*;
use crate::process::{capture, os, stream, Captured, Stream};
use crate::{CancellationToken, Error, Result};
use serde::de::DeserializeOwned;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Dünner, typisierter Wrapper um `arduino-cli`. Er nutzt dieselben Cores und Bibliotheken wie die Arduino IDE.
#[derive(Debug, Clone)]
pub struct ArduinoCli {
    pub path: PathBuf,
}

impl ArduinoCli {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    async fn run(&self, args: &[&str], timeout: Duration) -> Result<Captured> {
        capture(&self.path, &os(args), None, "C", Some(timeout)).await
    }

    async fn json<T: DeserializeOwned>(&self, args: &[&str], timeout: Duration) -> Result<T> {
        let out = self.run(args, timeout).await?;
        if !out.success() {
            return Err(Error::Failed(clean_error(&out)));
        }
        serde_json::from_str(&out.stdout).map_err(|e| Error::Parse {
            tool: "arduino-cli",
            detail: e.to_string(),
        })
    }

    pub async fn version(&self) -> Result<String> {
        #[derive(serde::Deserialize)]
        struct V {
            #[serde(rename = "VersionString", default)]
            version: String,
        }
        let v: V = self
            .json(&["version", "--json"], Duration::from_secs(10))
            .await?;
        Ok(v.version)
    }

    /// Wirksamer Wert einer Einstellung, einschließlich der Standardwerte. Listen ergeben mehrere Zeilen.
    async fn config_get(&self, key: &str) -> Result<Vec<String>> {
        let out = self
            .run(&["config", "get", key], Duration::from_secs(10))
            .await?;
        if !out.success() {
            return Err(Error::Failed(clean_error(&out)));
        }
        Ok(out
            .stdout
            .lines()
            .map(|l| l.trim().trim_start_matches("- ").trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    /// Datenordner von arduino-cli, meist `~/Library/Arduino15`.
    pub async fn data_dir(&self) -> Result<PathBuf> {
        self.config_get("directories.data")
            .await?
            .into_iter()
            .next()
            .map(PathBuf::from)
            .ok_or_else(|| Error::Failed("arduino-cli kennt keinen Datenordner.".into()))
    }

    /// Trägt eine zusätzliche Board-Manager-URL ein, falls sie fehlt. Gibt `true` zurück, wenn sie neu ist.
    pub async fn ensure_board_url(&self, url: &str) -> Result<bool> {
        if self
            .config_get("board_manager.additional_urls")
            .await?
            .iter()
            .any(|u| u == url)
        {
            return Ok(false);
        }
        let out = self
            .run(
                &["config", "add", "board_manager.additional_urls", url],
                Duration::from_secs(10),
            )
            .await?;
        if out.success() {
            Ok(true)
        } else {
            Err(Error::Failed(clean_error(&out)))
        }
    }

    /// Ordner für eigene Bibliotheken, meist `~/Documents/Arduino/libraries`.
    pub async fn user_libraries_dir(&self) -> Result<PathBuf> {
        let user = self
            .config_get("directories.user")
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Failed("arduino-cli kennt keinen Bibliotheksordner.".into()))?;
        Ok(PathBuf::from(user).join("libraries"))
    }

    pub async fn boards(&self, query: &str) -> Result<Vec<BoardEntry>> {
        let mut args = vec!["board", "listall", "--json"];
        if !query.is_empty() {
            args.insert(2, query);
        }
        let raw: RawBoardList = self.json(&args, Duration::from_secs(30)).await?;
        Ok(raw
            .boards
            .into_iter()
            .filter(|b| !b.fqbn.is_empty())
            .map(|b| BoardEntry {
                name: b.name,
                fqbn: b.fqbn,
                platform: b.platform.metadata.id,
            })
            .collect())
    }

    pub async fn board_options(&self, fqbn: &str) -> Result<Vec<BoardOption>> {
        let raw: RawBoardDetails = self
            .json(
                &["board", "details", "-b", fqbn, "--json"],
                Duration::from_secs(30),
            )
            .await?;
        Ok(raw
            .config_options
            .into_iter()
            .map(|o| {
                let default = o
                    .values
                    .iter()
                    .find(|v| v.selected)
                    .or(o.values.first())
                    .map(|v| v.value.clone())
                    .unwrap_or_default();
                BoardOption {
                    id: o.option,
                    label: o.option_label,
                    values: o
                        .values
                        .into_iter()
                        .map(|v| BoardOptionValue {
                            label: if v.value_label.is_empty() {
                                v.value.clone()
                            } else {
                                v.value_label
                            },
                            value: v.value,
                        })
                        .collect(),
                    default,
                }
            })
            .collect())
    }

    /// Build-Eigenschaften des Boards, z. B. `build.mcu=esp32s3` und `upload.speed=921600`.
    pub async fn board_properties(
        &self,
        fqbn: &str,
    ) -> Result<std::collections::HashMap<String, String>> {
        #[derive(serde::Deserialize, Default)]
        struct D {
            #[serde(default)]
            build_properties: Vec<String>,
        }
        let d: D = self
            .json(
                &["board", "details", "-b", fqbn, "--json"],
                Duration::from_secs(30),
            )
            .await?;
        Ok(d.build_properties
            .iter()
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect())
    }

    pub async fn cores(&self) -> Result<Vec<CoreInfo>> {
        let raw: RawCoreList = self
            .json(
                &["core", "list", "--all", "--json"],
                Duration::from_secs(60),
            )
            .await?;
        Ok(raw
            .platforms
            .into_iter()
            .map(|c| {
                let name = c
                    .releases
                    .get(&c.latest_version)
                    .or_else(|| c.releases.values().next())
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| c.id.clone());
                CoreInfo {
                    id: c.id,
                    name,
                    installed: Some(c.installed_version).filter(|v| !v.is_empty()),
                    latest: Some(c.latest_version).filter(|v| !v.is_empty()),
                }
            })
            .collect())
    }

    pub async fn libraries(&self) -> Result<Vec<LibraryInfo>> {
        let raw: RawLibList = self
            .json(&["lib", "list", "--json"], Duration::from_secs(60))
            .await?;
        Ok(raw
            .installed_libraries
            .into_iter()
            .map(|i| {
                let latest = i
                    .release
                    .map(|r| r.version)
                    .filter(|v| !v.is_empty() && version_cmp(v, &i.library.version).is_gt());
                LibraryInfo {
                    name: i.library.name,
                    version: i.library.version,
                    author: i.library.author,
                    sentence: i.library.sentence,
                    website: i.library.website,
                    location: i.library.location,
                    install_dir: Some(i.library.install_dir.clone())
                        .filter(|d| !d.is_empty())
                        .map(Into::into),
                    provides_includes: i.library.provides_includes,
                    latest,
                }
            })
            .collect())
    }

    /// Ob der Bibliotheksverwalter genau diese Version der Bibliothek anbietet. Nur dann lässt sie sich auf einem
    /// anderen Rechner nachinstallieren, sonst muss ihr Quelltext mitgegeben werden.
    pub async fn index_has(&self, name: &str, version: &str) -> Result<bool> {
        let query = format!("name=\"{}\"", name.replace('"', ""));
        let hits = self.search_libraries(&query, &[]).await?;
        Ok(hits
            .iter()
            .any(|h| h.name == name && h.versions.iter().any(|v| v == version)))
    }

    pub async fn search_libraries(
        &self,
        query: &str,
        installed: &[LibraryInfo],
    ) -> Result<Vec<LibrarySearchHit>> {
        let raw: RawLibSearch = self
            .json(&["lib", "search", query, "--json"], Duration::from_secs(60))
            .await?;
        let mut hits: Vec<LibrarySearchHit> = raw
            .libraries
            .into_iter()
            .map(|l| {
                let mut versions: Vec<String> = l.releases.keys().cloned().collect();
                versions.sort_by(|a, b| version_cmp(b, a));
                let latest = l
                    .latest
                    .as_ref()
                    .map(|x| x.version.clone())
                    .filter(|v| !v.is_empty())
                    .or_else(|| versions.first().cloned())
                    .unwrap_or_default();
                let meta = l.latest.or_else(|| {
                    l.releases.get(&latest).map(|r| RawLib {
                        name: r.name.clone(),
                        version: r.version.clone(),
                        author: r.author.clone(),
                        sentence: r.sentence.clone(),
                        website: r.website.clone(),
                        location: String::new(),
                        install_dir: String::new(),
                        provides_includes: vec![],
                    })
                });
                let meta = meta.unwrap_or_default();
                let installed = installed
                    .iter()
                    .find(|i| i.name == l.name)
                    .map(|i| i.version.clone());
                LibrarySearchHit {
                    name: l.name,
                    latest,
                    author: meta.author,
                    sentence: meta.sentence,
                    website: meta.website,
                    versions,
                    installed,
                }
            })
            .collect();
        hits.sort_by(|a, b| {
            relevance(query, &a.name)
                .cmp(&relevance(query, &b.name))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(hits)
    }

    pub async fn update_index(
        &self,
        cancel: &CancellationToken,
        on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        self.streamed(&["update"], cancel, on_line).await
    }

    pub async fn install_core(
        &self,
        id: &str,
        cancel: &CancellationToken,
        on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        self.streamed(&["core", "install", id], cancel, on_line)
            .await
    }

    pub async fn install_library(
        &self,
        name: &str,
        version: Option<&str>,
        cancel: &CancellationToken,
        on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        let spec = version.map_or_else(|| name.to_string(), |v| format!("{name}@{v}"));
        self.streamed(&["lib", "install", &spec], cancel, on_line)
            .await
    }

    pub async fn uninstall_library(
        &self,
        name: &str,
        cancel: &CancellationToken,
        on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        self.streamed(&["lib", "uninstall", name], cancel, on_line)
            .await
    }

    pub async fn upgrade_library(
        &self,
        name: &str,
        cancel: &CancellationToken,
        on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        self.streamed(&["lib", "upgrade", name], cancel, on_line)
            .await
    }

    async fn streamed(
        &self,
        args: &[&str],
        cancel: &CancellationToken,
        mut on_line: impl FnMut(Stream, &str),
    ) -> Result<()> {
        let mut errors = Vec::new();
        let code = stream(&self.path, &os(args), None, "C", cancel, |s, l| {
            if s == Stream::Stderr {
                errors.push(l.to_string());
            }
            on_line(s, l);
        })
        .await?;
        if code == Some(0) {
            Ok(())
        } else {
            Err(Error::Failed(errors.last().cloned().unwrap_or_else(|| {
                format!("{} ist mit Fehler beendet.", args.join(" "))
            })))
        }
    }

    pub fn args(parts: &[&str]) -> Vec<OsString> {
        os(parts)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Kleiner ist besser: genaue Treffer zuerst, dann Namen, die mit der Suche beginnen, dann der Rest.
pub fn relevance(query: &str, name: &str) -> u8 {
    let (q, n) = (normalize(query), normalize(name));
    if n == q {
        0
    } else if n.starts_with(&q) {
        1
    } else if n.contains(&q) {
        2
    } else {
        3
    }
}

/// Fehlermeldung aus der Ausgabe von arduino-cli herausziehen.
pub fn clean_error(out: &Captured) -> String {
    let text = if out.stderr.trim().is_empty() {
        &out.stdout
    } else {
        &out.stderr
    };
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("arduino-cli ist mit einem Fehler beendet.");
    line.trim().trim_start_matches("Error: ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_library_names_come_first_even_with_underscores() {
        assert_eq!(relevance("Adafruit_NeoPixel", "Adafruit NeoPixel"), 0);
        assert_eq!(relevance("neopixel", "NeoPixel Painter"), 1);
        assert_eq!(relevance("neopixel", "Adafruit NeoPixel"), 2);
        assert_eq!(relevance("neopixel", "FastLED"), 3);
    }

    fn cli() -> Option<ArduinoCli> {
        crate::tools::Tools::locate(&[])
            .ok()
            .map(|t| ArduinoCli::new(t.arduino_cli))
    }

    #[tokio::test]
    async fn reads_boards_and_options_from_the_installed_core() {
        let Some(cli) = cli() else { return };
        let boards = cli.boards("esp32:esp32").await.unwrap_or_default();
        if boards.is_empty() {
            return; // ESP32-Core nicht installiert: nichts zu prüfen.
        }
        assert!(boards.iter().any(|b| b.fqbn == "esp32:esp32:esp32s3"));
        let opts = cli.board_options("esp32:esp32:esp32s3").await.unwrap();
        assert!(opts
            .iter()
            .any(|o| o.id == "FlashSize" && !o.values.is_empty()));
    }

    #[tokio::test]
    async fn knows_its_directories() {
        let Some(cli) = cli() else { return };
        let data = cli.data_dir().await.unwrap();
        assert!(data.is_absolute(), "{data:?}");
        let libs = cli.user_libraries_dir().await.unwrap();
        assert!(libs.ends_with("libraries"), "{libs:?}");
    }

    #[tokio::test]
    async fn lists_libraries_and_cores() {
        let Some(cli) = cli() else { return };
        assert!(cli.version().await.is_ok());
        let libs = cli.libraries().await.unwrap();
        for l in &libs {
            assert!(!l.name.is_empty());
        }
        let cores = cli.cores().await.unwrap();
        assert!(cores.iter().all(|c| !c.id.is_empty()));
    }
}
