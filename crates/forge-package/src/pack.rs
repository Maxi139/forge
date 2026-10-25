use crate::{
    Manifest, PackageError, Result, AI_CACHE, AI_CACHE_FILE, AI_THREADS, AI_THREADS_FILE, FILES,
    FORMAT, LIBRARIES, MANIFEST, MAX_BYTES, MAX_ENTRIES,
};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

/// Namen, die nie ins Paket gehören: Verlauf, Build-Ausgaben, Systemmüll.
const SKIP: &[&str] = &[".git", ".forge", "build", ".DS_Store", "node_modules"];
const SKIP_LIB: &[&str] = &[".git", ".github", ".DS_Store"];
/// Formate, die schon komprimiert sind. Deflate würde sie nur vergrößern.
const INCOMPRESSIBLE: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "zip", "gz", "bz2", "xz", "zst", "7z", "woff",
    "woff2", "mp3", "m4a", "mp4", "mov", "ogg", "jar",
];

pub struct PackRequest<'a> {
    pub manifest: Manifest,
    /// Der Projektordner.
    pub root: &'a Path,
    /// Eingebettete Bibliotheken: Ordnername im Paket und Quellordner.
    pub embedded: Vec<(String, PathBuf)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackReport {
    pub files: usize,
    /// Größe aller Dateien unkomprimiert.
    pub bytes: u64,
    /// Größe der fertigen Datei.
    pub packed: u64,
}

struct Item {
    name: String,
    path: PathBuf,
    size: u64,
    exec: bool,
}

fn walk(dir: &Path, prefix: &str, skip: &[&str], out: &mut Vec<Item>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            return Err(PackageError::BadName(
                file_name.to_string_lossy().into_owned(),
            ));
        };
        // Zwischenstände von Forge und der Zwischenspeicher der KI gehören nicht zum Quelltext.
        if skip.contains(&name) || name.ends_with(".forge-tmp") {
            continue;
        }
        // Links werden nie verfolgt. Sonst könnte ein Link auf fremde Dateien sie ins Paket ziehen.
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            continue;
        }
        let rel = format!("{prefix}{name}");
        if kind.is_dir() {
            walk(&entry.path(), &format!("{rel}/"), skip, out)?;
        } else if kind.is_file() {
            let meta = entry.metadata()?;
            #[cfg(unix)]
            let exec = {
                use std::os::unix::fs::PermissionsExt;
                meta.permissions().mode() & 0o111 != 0
            };
            #[cfg(not(unix))]
            let exec = false;
            out.push(Item {
                name: rel,
                path: entry.path(),
                size: meta.len(),
                exec,
            });
        }
    }
    Ok(())
}

fn options(name: &str, exec: bool) -> SimpleFileOptions {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let base = SimpleFileOptions::default()
        // Feste Zeit, damit gleiche Inhalte gleiche Bytes ergeben.
        .last_modified_time(DateTime::default())
        .unix_permissions(if exec { 0o755 } else { 0o644 });
    if INCOMPRESSIBLE.contains(&ext.as_str()) {
        base.compression_method(CompressionMethod::Stored)
    } else {
        base.compression_method(CompressionMethod::Deflated)
            .compression_level(Some(9))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Schreibt das Paket nach `dest`, über eine Zwischendatei. Ein Absturz hinterlässt nie eine halbe Datei.
pub fn pack(req: PackRequest, dest: &Path) -> Result<PackReport> {
    let mut manifest = req.manifest;
    manifest.format = FORMAT;
    manifest.saved_at = now();

    let mut items = Vec::new();
    walk(req.root, FILES, SKIP, &mut items)?;
    let cache = req.root.join(AI_CACHE_FILE);
    // Ein leerer Zwischenspeicher (`{}`) ist keiner.
    let has_cache = std::fs::metadata(&cache).is_ok_and(|m| m.is_file() && m.len() > 2);
    manifest.ai.cached = has_cache;
    if has_cache {
        items.push(Item {
            name: AI_CACHE.into(),
            size: std::fs::metadata(&cache)?.len(),
            path: cache,
            exec: false,
        });
    }
    let threads = req.root.join(AI_THREADS_FILE);
    if let Ok(m) = std::fs::metadata(&threads) {
        if m.is_file() && m.len() > 2 {
            items.push(Item {
                name: AI_THREADS.into(),
                size: m.len(),
                path: threads,
                exec: false,
            });
        }
    }
    for lib in manifest
        .libraries
        .iter()
        .filter(|l| l.source == crate::LibrarySource::Embedded)
    {
        let dir = lib.dir.clone().unwrap_or_else(|| lib.name.clone());
        let src = req
            .embedded
            .iter()
            .find(|(d, _)| *d == dir)
            .map(|(_, p)| p)
            .filter(|p| p.is_dir())
            .ok_or_else(|| PackageError::MissingLibrarySource(lib.name.clone()))?;
        walk(src, &format!("{LIBRARIES}{dir}/"), SKIP_LIB, &mut items)?;
    }
    items.sort_by(|a, b| a.name.cmp(&b.name));

    let bytes: u64 = items.iter().map(|i| i.size).sum();
    if bytes > MAX_BYTES || items.len() > MAX_ENTRIES {
        return Err(PackageError::TooLarge(MAX_BYTES / 1024 / 1024));
    }

    let tmp = dest.with_extension("forgeproj.tmp");
    let result = (|| -> Result<()> {
        let mut zip = ZipWriter::new(std::io::BufWriter::new(std::fs::File::create(&tmp)?));
        zip.start_file(MANIFEST, options(MANIFEST, false))?;
        zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        for item in &items {
            zip.start_file(&item.name, options(&item.name, item.exec))?;
            let mut f = std::fs::File::open(&item.path)?;
            std::io::copy(&mut f, &mut zip)?;
        }
        zip.finish()?.flush()?;
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(PackReport {
        files: items.len(),
        bytes,
        packed: std::fs::metadata(dest)?.len(),
    })
}
