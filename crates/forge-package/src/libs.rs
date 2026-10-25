//! Welche Bibliotheken ein Projekt braucht, aus seinen `#include`-Zeilen.

use std::collections::BTreeSet;
use std::path::PathBuf;

/// Eine installierte Bibliothek, so wie sie `arduino-cli lib list` meldet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub version: String,
    /// Aus dem Bibliotheksordner des Nutzers. Bibliotheken von Core und Arduino-IDE gehören zum Core.
    pub user: bool,
    pub includes: Vec<String>,
    pub dir: Option<PathBuf>,
}

/// Die Dateinamen aller eingebundenen Header: `#include <Wire.h>` und `#include "lib/Foo.h"` ergeben `Wire.h`
/// und `Foo.h`. Auskommentierte Zeilen zählen nicht.
pub fn included_headers<'a>(sources: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for text in sources {
        let mut in_block = false;
        for line in text.lines() {
            let mut rest = line;
            // Blockkommentare über mehrere Zeilen grob überspringen.
            if in_block {
                match rest.find("*/") {
                    Some(i) => {
                        rest = &rest[i + 2..];
                        in_block = false;
                    }
                    None => continue,
                }
            }
            let rest = rest.trim_start();
            if rest.starts_with("/*") && !rest.contains("*/") {
                in_block = true;
                continue;
            }
            let Some(after) = rest.strip_prefix('#') else {
                continue;
            };
            let Some(after) = after.trim_start().strip_prefix("include") else {
                continue;
            };
            let after = after.trim_start();
            let close = match after.chars().next() {
                Some('<') => '>',
                Some('"') => '"',
                _ => continue,
            };
            let inner = &after[1..];
            if let Some(end) = inner.find(close) {
                let path = &inner[..end];
                if let Some(file) = path.rsplit('/').next().filter(|f| !f.is_empty()) {
                    out.insert(file.to_string());
                }
            }
        }
    }
    out
}

/// Die Bibliotheken des Nutzers, die mindestens einen der Header liefern, nach Namen sortiert.
pub fn used<'a>(headers: &BTreeSet<String>, installed: &'a [Installed]) -> Vec<&'a Installed> {
    let mut v: Vec<&Installed> = installed
        .iter()
        .filter(|l| l.user && l.includes.iter().any(|i| headers.contains(i)))
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    v.dedup_by(|a, b| a.name == b.name);
    v
}

/// Ordnername für eine Bibliothek im Paket: nur Zeichen, die überall als Dateiname taugen.
pub fn dir_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_start_matches('.');
    if s.is_empty() {
        "library".into()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib(name: &str, user: bool, includes: &[&str]) -> Installed {
        Installed {
            name: name.into(),
            version: "1.0.0".into(),
            user,
            includes: includes.iter().map(|s| s.to_string()).collect(),
            dir: None,
        }
    }

    #[test]
    fn finds_includes_and_ignores_comments() {
        let src = "#include <Wire.h>\n  #  include \"src/Foo.h\"\n// #include <Nope.h>\n/* \n#include <AuchNicht.h>\n*/\n#include <Adafruit_NeoPixel.h> // Streifen\nint x;\n";
        let h = included_headers([src]);
        assert_eq!(
            h.into_iter().collect::<Vec<_>>(),
            vec!["Adafruit_NeoPixel.h", "Foo.h", "Wire.h"]
        );
    }

    #[test]
    fn only_user_libraries_that_provide_a_used_header_count() {
        let installed = vec![
            lib("Adafruit NeoPixel", true, &["Adafruit_NeoPixel.h"]),
            lib("Wire", false, &["Wire.h"]),
            lib("Unbenutzt", true, &["Unbenutzt.h"]),
        ];
        let h = included_headers(["#include <Wire.h>\n#include <Adafruit_NeoPixel.h>\n"]);
        let names: Vec<_> = used(&h, &installed)
            .iter()
            .map(|l| l.name.clone())
            .collect();
        assert_eq!(names, vec!["Adafruit NeoPixel"]);
    }

    #[test]
    fn dir_names_are_safe() {
        assert_eq!(dir_name("DHT sensor library"), "DHT_sensor_library");
        assert_eq!(dir_name("../x"), "_x");
        assert_eq!(dir_name("..."), "library");
        assert_eq!(dir_name("Ü/ber"), "__ber");
    }
}
