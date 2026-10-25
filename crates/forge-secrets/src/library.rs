//! Die Arduino-Bibliothek `ForgeSecrets`: `Secret("NAME")` liest Werte aus dem NVS des Boards.

use std::io;
use std::path::Path;

pub const VERSION: &str = "1.0.0";

const PROPERTIES: &str = "name=ForgeSecrets
version=1.0.0
author=Forge
maintainer=Forge
sentence=Liest WLAN-Passwörter und Tokens aus dem NVS des ESP32.
paragraph=Die Werte kommen aus dem macOS-Schlüsselbund, werden von Forge beim Flashen in die NVS-Partition geschrieben und stehen nie im Quelltext.
category=Data Storage
url=https://forge.local
architectures=esp32
includes=ForgeSecrets.h
";

const HEADER: &str = r#"#pragma once
// ForgeSecrets: liest Secrets, die Forge beim Flashen in den NVS-Namensraum „forge“ geschrieben hat.
// Aufruf erst in setup() oder später. Globale Variablen werden vor dem NVS-Start angelegt.

#include <Arduino.h>
#include <Preferences.h>

namespace forge_secrets {
inline bool &warned() {
  static bool w = false;
  return w;
}
}  // namespace forge_secrets

inline String Secret(const char *name) {
  Preferences prefs;
  if (!prefs.begin("forge", true)) {
    if (!forge_secrets::warned()) {
      forge_secrets::warned() = true;
      Serial.println("[Forge] Keine Secrets auf dem Board. Flashe das Projekt mit Forge, dann sind sie da.");
    }
    return String();
  }
  String value = prefs.getString(name, "");
  prefs.end();
  return value;
}

inline bool HasSecret(const char *name) {
  Preferences prefs;
  if (!prefs.begin("forge", true)) return false;
  bool has = prefs.isKey(name);
  prefs.end();
  return has;
}
"#;

/// Legt die Bibliothek im Bibliotheksordner an, falls sie fehlt oder veraltet ist.
/// Gibt `true` zurück, wenn etwas geschrieben wurde.
pub fn ensure_installed(user_libraries: &Path) -> io::Result<bool> {
    let dir = user_libraries.join("ForgeSecrets");
    let props = dir.join("library.properties");
    if std::fs::read_to_string(&props).is_ok_and(|t| t.contains(&format!("version={VERSION}")))
        && dir.join("src/ForgeSecrets.h").is_file()
    {
        return Ok(false);
    }
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::write(dir.join("src/ForgeSecrets.h"), HEADER)?;
    std::fs::write(props, PROPERTIES)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_once_and_updates_when_old() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(ensure_installed(tmp.path()).unwrap());
        assert!(!ensure_installed(tmp.path()).unwrap());
        assert!(tmp.path().join("ForgeSecrets/src/ForgeSecrets.h").is_file());
        std::fs::write(
            tmp.path().join("ForgeSecrets/library.properties"),
            "name=ForgeSecrets\nversion=0.1.0\n",
        )
        .unwrap();
        assert!(ensure_installed(tmp.path()).unwrap());
    }
}
