//! Projektmodell von Forge: Sketches (ein Ordner mit `.ino`) und Projekte (zusätzlich `forge.toml`).

mod config;
mod create;
mod error;
mod rename;
mod templates;
mod tree;

pub use config::{AiConfig, BoardConfig, ForgeToml, ProjectMeta, SecretsConfig};
pub use create::{
    convert_to_project, create_project, create_sketch, create_sketch_avoiding, new_id,
    validate_name, CreateRequest,
};
pub use error::{ProjectError, Result};
pub use rename::rename_dir;
pub use templates::Template;
pub use tree::{list_tree, FileNode};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CONFIG_FILE: &str = "forge.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Project,
    Sketch,
}

/// Was die Oberfläche über ein geöffnetes Projekt wissen muss.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInfo {
    pub kind: ProjectKind,
    pub name: String,
    pub path: PathBuf,
    /// Pfad der Haupt-`.ino`-Datei.
    pub main_file: PathBuf,
    pub config: ForgeToml,
    /// Die `.forgeproj`-Datei, in der das Projekt gespeichert wird. `path` ist dann der Arbeitsordner.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<PathBuf>,
}

/// Öffnet einen Ordner oder eine `.ino`-Datei.
///
/// Ein Ordner mit `forge.toml` ist ein Projekt. Ein Ordner mit `<name>/<name>.ino` oder genau einer `.ino`
/// ist ein Sketch. Eine `.ino`-Datei öffnet ihren Ordner.
pub fn open(path: &Path) -> Result<ProjectInfo> {
    let dir = if path.is_file() {
        path.parent()
            .ok_or_else(|| ProjectError::NotFound(path.to_path_buf()))?
            .to_path_buf()
    } else {
        path.to_path_buf()
    };
    if !dir.is_dir() {
        return Err(ProjectError::NotFound(dir));
    }
    // macOS meldet Dateiänderungen mit aufgelösten Pfaden (`/private/tmp` statt `/tmp`). Wir arbeiten durchgehend damit.
    let dir = dir.canonicalize().unwrap_or(dir);
    let path = if path.is_file() {
        dir.join(path.file_name().unwrap_or_default())
    } else {
        dir.clone()
    };
    let path = path.as_path();

    let has_config = dir.join(CONFIG_FILE).is_file();
    let main_file = find_main_file(&dir, path)?;
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sketch".into());

    let config = if has_config {
        ForgeToml::load(&dir.join(CONFIG_FILE))?
    } else {
        ForgeToml::for_sketch(&name)
    };
    Ok(ProjectInfo {
        kind: if has_config {
            ProjectKind::Project
        } else {
            ProjectKind::Sketch
        },
        name: config.project.name.clone(),
        path: dir,
        main_file,
        config,
        package: None,
    })
}

pub(crate) fn find_main_file(dir: &Path, opened: &Path) -> Result<PathBuf> {
    if opened.is_file() && is_ino(opened) {
        return Ok(opened.to_path_buf());
    }
    let dir_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let expected = dir.join(format!("{dir_name}.ino"));
    if expected.is_file() {
        return Ok(expected);
    }
    let mut inos: Vec<PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_ino(p))
        .collect();
    inos.sort();
    inos.into_iter()
        .next()
        .ok_or_else(|| ProjectError::NoSketch(dir.to_path_buf()))
}

fn is_ino(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino"))
}

/// Verhindert, dass Dateioperationen aus dem Projektordner ausbrechen.
pub fn resolve_inside(root: &Path, relative_or_absolute: &Path) -> Result<PathBuf> {
    let joined = if relative_or_absolute.is_absolute() {
        relative_or_absolute.to_path_buf()
    } else {
        root.join(relative_or_absolute)
    };
    let mut normalized = PathBuf::new();
    for c in joined.components() {
        use std::path::Component::*;
        match c {
            ParentDir => {
                normalized.pop();
            }
            CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.starts_with(root) {
        Ok(normalized)
    } else {
        Err(ProjectError::OutsideProject(joined))
    }
}

/// Kennung für die Secrets eines Ordners: bei Projekten aus `forge.toml` (wird bei Bedarf angelegt),
/// bei Sketches aus dem Pfad.
pub fn secrets_id(root: &Path) -> String {
    let cfg_path = root.join(CONFIG_FILE);
    if let Ok(mut cfg) = ForgeToml::load(&cfg_path) {
        if let Some(id) = cfg.project.id.clone() {
            return id;
        }
        let id = new_id(root);
        cfg.project.id = Some(id.clone());
        let _ = cfg.save(&cfg_path);
        return id;
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut h);
    format!("path-{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_resolves_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("echt");
        std::fs::create_dir_all(real.join("proj")).unwrap();
        std::fs::write(
            real.join("proj/proj.ino"),
            "void setup(){}\nvoid loop(){}\n",
        )
        .unwrap();
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let info = open(&link.join("proj")).unwrap();
        assert_eq!(info.path, real.join("proj").canonicalize().unwrap());
        assert!(info.main_file.starts_with(&info.path));
        // Auch eine .ino-Datei über den Link öffnet den aufgelösten Ordner.
        let again = open(&link.join("proj/proj.ino")).unwrap();
        assert_eq!(again.path, info.path);
    }

    #[test]
    fn resolve_inside_blocks_escape() {
        let root = Path::new("/tmp/proj");
        assert!(resolve_inside(root, Path::new("../etc/passwd")).is_err());
        assert!(resolve_inside(root, Path::new("a/../../x")).is_err());
        assert_eq!(
            resolve_inside(root, Path::new("a/./b.ino")).unwrap(),
            Path::new("/tmp/proj/a/b.ino")
        );
        assert!(resolve_inside(root, Path::new("/tmp/proj/src/x.cpp")).is_ok());
        assert!(resolve_inside(root, Path::new("/tmp/other")).is_err());
    }
}
