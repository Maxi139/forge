//! Zuordnung zwischen dem zusammengesetzten Sketch, den clangd sieht, und den Originaldateien.
//! Die `#line`-Marken, die `combine` setzt, sind die Quelle der Wahrheit.

use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct LineMap {
    pub files: Vec<String>,
    /// Zeile im zusammengesetzten Text (ab 0) zu Datei und Originalzeile (ab 1).
    virt_to_orig: Vec<Option<(usize, u32)>>,
    /// Originalzeile zur Zeile im zusammengesetzten Text. Bei Mehrdeutigkeit gilt die letzte: der echte Text,
    /// nicht der erzeugte Prototyp davor.
    orig_to_virt: HashMap<(usize, u32), u32>,
}

fn unescape(s: &str) -> String {
    s.replace("\\\"", "\"").replace("\\\\", "\\")
}

fn directive(line: &str) -> Option<(u32, String)> {
    let rest = line.strip_prefix("#line ")?;
    let (n, path) = rest.split_once(' ')?;
    let path = path.trim().strip_prefix('"')?.strip_suffix('"')?;
    Some((n.parse().ok()?, unescape(path)))
}

impl LineMap {
    pub fn parse(text: &str) -> Self {
        let mut files: Vec<String> = Vec::new();
        let mut virt_to_orig = Vec::new();
        let mut orig_to_virt = HashMap::new();
        let mut current: Option<(usize, u32)> = None;
        for (v, line) in text.split('\n').enumerate() {
            if let Some((n, path)) = directive(line) {
                let idx = files.iter().position(|f| *f == path).unwrap_or_else(|| {
                    files.push(path);
                    files.len() - 1
                });
                current = Some((idx, n));
                virt_to_orig.push(None);
                continue;
            }
            match current {
                Some((f, n)) => {
                    virt_to_orig.push(Some((f, n)));
                    orig_to_virt.insert((f, n), v as u32);
                    current = Some((f, n + 1));
                }
                None => virt_to_orig.push(None),
            }
        }
        Self {
            files,
            virt_to_orig,
            orig_to_virt,
        }
    }

    /// Original (Datei, Zeile ab 1) zur Zeile im zusammengesetzten Text.
    pub fn to_virtual(&self, file: &str, line: u32) -> Option<u32> {
        let f = self.files.iter().position(|f| f == file)?;
        self.orig_to_virt.get(&(f, line)).copied()
    }

    /// Zeile im zusammengesetzten Text zu (Datei, Zeile ab 1). `None` für erzeugte Zeilen.
    pub fn to_original(&self, virt: u32) -> Option<(&str, u32)> {
        let (f, n) = (*self.virt_to_orig.get(virt as usize)?)?;
        // Prototypen sind doppelt abgebildet. Zählen darf nur der echte Text.
        if self.orig_to_virt.get(&(f, n)) != Some(&virt) {
            return None;
        }
        Some((self.files[f].as_str(), n))
    }

    pub fn has_file(&self, file: &str) -> bool {
        self.files.iter().any(|f| f == file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMBINED: &str = "#include <Arduino.h>\n#line 1 \"/p/blink.ino\"\nint z = 0;\n\n#line 3 \"/p/blink.ino\"\nvoid setup();\n#line 8 \"/p/blink.ino\"\nvoid loop();\n#line 3 \"/p/blink.ino\"\nvoid setup() {\n  Serial.begin(115200);\n}\n";

    #[test]
    fn maps_both_ways_and_prefers_real_text() {
        let m = LineMap::parse(COMBINED);
        assert_eq!(m.to_original(2), Some(("/p/blink.ino", 1)));
        // Zeile 3 gibt es als Prototyp (virtuell 5) und als Definition (virtuell 9).
        assert_eq!(m.to_virtual("/p/blink.ino", 3), Some(9));
        assert_eq!(m.to_original(9), Some(("/p/blink.ino", 3)));
        assert_eq!(
            m.to_original(5),
            None,
            "Prototypzeile gehört nicht zum Text"
        );
        assert_eq!(m.to_original(10), Some(("/p/blink.ino", 4)));
        assert_eq!(m.to_original(0), None, "die erzeugte include-Zeile");
        assert_eq!(m.to_virtual("/p/other.ino", 1), None);
    }

    #[test]
    fn several_files_and_escaped_paths() {
        let text = "#include <Arduino.h>\n#line 1 \"/p/a \\\"x\\\".ino\"\nint a;\n#line 1 \"/p/b.ino\"\nint b;\n";
        let m = LineMap::parse(text);
        assert_eq!(
            m.files,
            vec!["/p/a \"x\".ino".to_string(), "/p/b.ino".to_string()]
        );
        assert_eq!(m.to_original(4), Some(("/p/b.ino", 1)));
        assert!(m.has_file("/p/b.ino"));
    }
}
