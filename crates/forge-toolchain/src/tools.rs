use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Gefundene externe Werkzeuge.
#[derive(Debug, Clone)]
pub struct Tools {
    pub arduino_cli: PathBuf,
}

fn is_exec(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn which(name: &str) -> Option<PathBuf> {
    ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"]
        .iter()
        .map(|d| Path::new(d).join(name))
        .find(|p| is_exec(p))
}

impl Tools {
    /// Sucht `arduino-cli`: zuerst in den eigenen Ordnern der App, dann an den üblichen Orten.
    pub fn locate(own_dirs: &[PathBuf]) -> Result<Self> {
        if let Some(p) = std::env::var_os("FORGE_ARDUINO_CLI")
            .map(PathBuf::from)
            .filter(|p| is_exec(p))
        {
            return Ok(Self { arduino_cli: p });
        }
        for d in own_dirs {
            let p = d.join("arduino-cli");
            if is_exec(&p) {
                return Ok(Self { arduino_cli: p });
            }
        }
        which("arduino-cli")
            .map(|arduino_cli| Self { arduino_cli })
            .ok_or(Error::ArduinoCliMissing)
    }
}

/// Neuester `esptool` im ESP32-Core unter `<data>/packages/esp32/tools/esptool_py/<version>/esptool`.
pub fn find_esptool(data_dir: &Path) -> Option<PathBuf> {
    let base = data_dir.join("packages/esp32/tools/esptool_py");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(base)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    versions.sort_by(|a, b| version_key(b).cmp(&version_key(a)));
    versions
        .into_iter()
        .map(|v| v.join("esptool"))
        .find(|p| is_exec(p))
}

fn version_key(p: &Path) -> Vec<u32> {
    p.file_name()
        .map(|n| {
            n.to_string_lossy()
                .split(|c: char| !c.is_ascii_digit())
                .filter_map(|x| x.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_newest_esptool() {
        let tmp = tempfile::tempdir().unwrap();
        for v in ["4.9.dev3", "5.0.2", "5.10.0"] {
            let d = tmp.path().join("packages/esp32/tools/esptool_py").join(v);
            std::fs::create_dir_all(&d).unwrap();
            let f = d.join("esptool");
            std::fs::write(&f, "#!/bin/sh\n").unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let found = find_esptool(tmp.path()).unwrap();
        assert!(found.to_string_lossy().contains("5.10.0"), "{found:?}");
    }
}
