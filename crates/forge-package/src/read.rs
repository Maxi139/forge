use crate::{
    Manifest, PackageError, Result, AI_CACHE, AI_CACHE_FILE, AI_THREADS, AI_THREADS_FILE, FILES,
    FORMAT, LIBRARIES, MANIFEST, MAX_BYTES, MAX_ENTRIES,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

type Archive = ZipArchive<std::io::BufReader<std::fs::File>>;

fn open_archive(src: &Path) -> Result<Archive> {
    let file = std::fs::File::open(src)?;
    ZipArchive::new(std::io::BufReader::new(file))
        .map_err(|_| PackageError::NotAPackage(src.to_path_buf()))
}

fn read_manifest(zip: &mut Archive, src: &Path) -> Result<Manifest> {
    let mut entry = zip
        .by_name(MANIFEST)
        .map_err(|_| PackageError::NotAPackage(src.to_path_buf()))?;
    let mut text = Vec::new();
    entry.by_ref().take(1024 * 1024).read_to_end(&mut text)?;
    // Die Versionsprüfung kommt zuerst. Eine neuere Datei darf Felder haben, die wir nicht kennen.
    #[derive(serde::Deserialize)]
    struct Version {
        format: u32,
    }
    let v: Version =
        serde_json::from_slice(&text).map_err(|_| PackageError::NotAPackage(src.to_path_buf()))?;
    if v.format > FORMAT {
        return Err(PackageError::TooNew(v.format));
    }
    Ok(serde_json::from_slice(&text)?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    Manifest,
    File,
    Library,
    Cache,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Entry {
    /// Pfad im Archiv, so wie ihn [`read_entry`] erwartet.
    pub name: String,
    pub area: Area,
    pub size: u64,
    pub packed: u64,
}

#[derive(Debug, Clone)]
pub struct Overview {
    pub manifest: Manifest,
    pub entries: Vec<Entry>,
}

fn area(name: &str) -> Area {
    if name == MANIFEST {
        Area::Manifest
    } else if name.starts_with(FILES) {
        Area::File
    } else if name.starts_with(LIBRARIES) {
        Area::Library
    } else {
        Area::Cache
    }
}

/// Liest Beschreibung und Dateiliste, ohne etwas zu entpacken.
pub fn inspect(src: &Path) -> Result<Overview> {
    let mut zip = open_archive(src)?;
    let manifest = read_manifest(&mut zip, src)?;
    let mut entries = Vec::new();
    for i in 0..zip.len() {
        let e = zip.by_index_raw(i)?;
        if e.is_dir() {
            continue;
        }
        let name = e.name()?.into_owned();
        entries.push(Entry {
            area: area(&name),
            name,
            size: e.size(),
            packed: e.compressed_size(),
        });
    }
    Ok(Overview { manifest, entries })
}

/// Eine Datei aus dem Archiv, z. B. `files/blink.ino`.
pub fn read_entry(src: &Path, name: &str) -> Result<Vec<u8>> {
    let mut zip = open_archive(src)?;
    let mut entry = zip
        .by_name(name)
        .map_err(|_| PackageError::MissingEntry(name.to_string()))?;
    let mut buf = Vec::new();
    entry.by_ref().take(MAX_BYTES).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Prüft einen Pfad aus dem Archiv und gibt ihn relativ zurück. Absolute Pfade und `..` sind verboten.
fn safe_relative(raw: &str) -> Result<PathBuf> {
    let p = Path::new(raw);
    let ok = !raw.is_empty()
        && !raw.contains('\\')
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
    if ok {
        Ok(p.to_path_buf())
    } else {
        Err(PackageError::UnsafePath(raw.to_string()))
    }
}

fn is_symlink(mode: Option<u32>) -> bool {
    mode.is_some_and(|m| m & 0o170000 == 0o120000)
}

fn write_entry(zip: &mut Archive, index: usize, target: &Path, budget: &mut u64) -> Result<()> {
    let mut entry = zip.by_index(index)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(target)?;
    // Begrenzt nach der Wirklichkeit, nicht nach der Angabe im Archiv.
    let copied = std::io::copy(&mut entry.by_ref().take(*budget + 1), &mut out)?;
    if copied > *budget {
        return Err(PackageError::TooLarge(MAX_BYTES / 1024 / 1024));
    }
    *budget -= copied;
    #[cfg(unix)]
    if entry.unix_mode().is_some_and(|m| m & 0o111 != 0) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

/// Entpackt den Projektinhalt und den KI-Zwischenspeicher nach `dest`. Eingebettete Bibliotheken
/// bleiben im Archiv (siehe [`extract_library`]).
pub fn unpack(src: &Path, dest: &Path) -> Result<Manifest> {
    let mut zip = open_archive(src)?;
    let manifest = read_manifest(&mut zip, src)?;
    if zip.len() > MAX_ENTRIES {
        return Err(PackageError::TooLarge(MAX_BYTES / 1024 / 1024));
    }
    let mut budget = MAX_BYTES;
    for i in 0..zip.len() {
        let (name, dir, mode) = {
            let e = zip.by_index_raw(i)?;
            (e.name()?.into_owned(), e.is_dir(), e.unix_mode())
        };
        if dir {
            continue;
        }
        if is_symlink(mode) {
            return Err(PackageError::UnsafePath(name));
        }
        let target = if let Some(rel) = name.strip_prefix(FILES) {
            dest.join(safe_relative(rel)?)
        } else if name == AI_CACHE {
            dest.join(AI_CACHE_FILE)
        } else if name == AI_THREADS {
            dest.join(AI_THREADS_FILE)
        } else {
            continue;
        };
        write_entry(&mut zip, i, &target, &mut budget)?;
    }
    Ok(manifest)
}

/// Entpackt eine eingebettete Bibliothek nach `<dest_parent>/<dir>` und gibt den Ordner zurück.
pub fn extract_library(src: &Path, dir: &str, dest_parent: &Path) -> Result<PathBuf> {
    let dir_rel = safe_relative(dir)?;
    if dir_rel.components().count() != 1 {
        return Err(PackageError::UnsafePath(dir.to_string()));
    }
    let mut zip = open_archive(src)?;
    let prefix = format!("{LIBRARIES}{dir}/");
    let dest = dest_parent.join(&dir_rel);
    let mut budget = MAX_BYTES;
    let mut found = false;
    for i in 0..zip.len() {
        let (name, is_dir, mode) = {
            let e = zip.by_index_raw(i)?;
            (e.name()?.into_owned(), e.is_dir(), e.unix_mode())
        };
        let Some(rel) = name.strip_prefix(&prefix) else {
            continue;
        };
        if is_dir {
            continue;
        }
        if is_symlink(mode) {
            return Err(PackageError::UnsafePath(name));
        }
        found = true;
        write_entry(&mut zip, i, &dest.join(safe_relative(rel)?), &mut budget)?;
    }
    if found {
        Ok(dest)
    } else {
        Err(PackageError::MissingEntry(prefix))
    }
}

/// SHA-256 der Datei. Damit erkennt Forge, ob ein bereits entpackter Arbeitsordner noch aktuell ist.
pub fn digest(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
