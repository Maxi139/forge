use forge_analysis::fingerprint;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const CODE_EXT: &[&str] = &[
    "ino", "cpp", "c", "cc", "h", "hpp", "hh", "s", "ld", "csv", "yaml", "yml",
];
const CODE_NAMES: &[&str] = &[
    "sdkconfig",
    "build_opt.h",
    "boards.local.txt",
    "partitions.csv",
];
const SKIP_DIRS: &[&str] = &[".git", ".forge", "build", "data", "node_modules", ".vscode"];
const FINGERPRINTED: &[&str] = &["ino", "cpp", "c", "cc", "h", "hpp", "hh"];

/// Alles, was die erzeugte Firmware beeinflusst.
pub struct HashInput<'a> {
    pub root: &'a Path,
    /// Board samt Menüoptionen, z. B. `esp32:esp32:esp32s3:FlashSize=16M`.
    pub fqbn: &'a str,
    pub cli_version: &'a str,
    /// Installierte Bibliotheken mit Version.
    pub libraries: &'a [(String, String)],
    pub core_version: &'a str,
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                collect(&path, out);
            }
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if CODE_EXT.contains(&ext.as_str()) || CODE_NAMES.contains(&name.as_str()) {
            out.push(path);
        }
    }
}

/// Alle Quelldateien eines Projekts, sortiert.
pub fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(root, &mut files);
    files.sort();
    files
}

/// Hash über Quellen, Board, Optionen, Bibliotheken und Werkzeugversionen.
/// Kommentare und Leerraum zählen bei Quelltext nicht, denn sie ändern die Firmware nicht.
pub fn input_hash(input: &HashInput) -> String {
    let mut h = Sha256::new();
    let mut put = |tag: &str, value: &[u8]| {
        h.update(tag.as_bytes());
        h.update((value.len() as u64).to_le_bytes());
        h.update(value);
    };
    put("fqbn", input.fqbn.as_bytes());
    put("cli", input.cli_version.as_bytes());
    put("core", input.core_version.as_bytes());
    let libs: BTreeMap<_, _> = input.libraries.iter().cloned().collect();
    for (name, version) in libs {
        put("lib", format!("{name}@{version}").as_bytes());
    }
    for file in source_files(input.root) {
        let rel = file
            .strip_prefix(input.root)
            .unwrap_or(&file)
            .to_string_lossy()
            .into_owned();
        let Ok(bytes) = std::fs::read(&file) else {
            continue;
        };
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        put("file", rel.as_bytes());
        match (
            FINGERPRINTED.contains(&ext.as_str()),
            std::str::from_utf8(&bytes),
        ) {
            (true, Ok(text)) => put("code", fingerprint(text).as_bytes()),
            _ => put("raw", &bytes),
        }
    }
    format!("{:x}", h.finalize())
}

/// Kurzer Schlüssel für Ordnernamen, abhängig vom Board und seinen Optionen.
pub fn short_key(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0]);
    }
    format!("{:x}", h.finalize())[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(root: &Path, fqbn: &str) -> String {
        input_hash(&HashInput {
            root,
            fqbn,
            cli_version: "1.5.1",
            libraries: &[("Lib".into(), "1.0.0".into())],
            core_version: "3.3.12",
        })
    }

    #[test]
    fn only_firmware_relevant_changes_alter_the_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("a.ino"), "void setup() {}\nvoid loop() {}\n").unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        std::fs::write(root.join("data/index.html"), "<p>").unwrap();
        let base = hash(root, "esp32:esp32:esp32s3");

        // Kommentar und Leerraum: gleicher Hash
        std::fs::write(
            root.join("a.ino"),
            "// neu\nvoid setup() {\n}\n\nvoid loop() {}\n",
        )
        .unwrap();
        assert_eq!(hash(root, "esp32:esp32:esp32s3"), base);
        // Dateien im data-Ordner und forge.toml gehören nicht zur Firmware
        std::fs::write(root.join("data/index.html"), "<p>geändert").unwrap();
        std::fs::write(root.join("forge.toml"), "x").unwrap();
        assert_eq!(hash(root, "esp32:esp32:esp32s3"), base);

        // Code, Board und Optionen ändern den Hash
        std::fs::write(
            root.join("a.ino"),
            "void setup() { delay(1); }\nvoid loop() {}\n",
        )
        .unwrap();
        let changed = hash(root, "esp32:esp32:esp32s3");
        assert_ne!(changed, base);
        assert_ne!(hash(root, "esp32:esp32:esp32s3:FlashSize=16M"), changed);
        assert_ne!(hash(root, "esp32:esp32:esp32"), changed);
    }
}
