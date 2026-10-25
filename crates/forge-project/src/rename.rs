use crate::{find_main_file, validate_name, ForgeToml, ProjectError, Result, CONFIG_FILE};
use std::path::{Path, PathBuf};

/// Benennt einen Sketch- oder Projektordner um und zieht alles mit, was an seinem Namen hängt: die Haupt-`.ino`
/// (Arduino verlangt, dass sie wie der Ordner heißt) und den Namen in `forge.toml`. Gibt den neuen Ordner zurück.
pub fn rename_dir(root: &Path, new_name: &str) -> Result<PathBuf> {
    validate_name(new_name)?;
    let old_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| ProjectError::NotFound(root.to_path_buf()))?;
    if old_name == new_name {
        return Ok(root.to_path_buf());
    }
    let parent = root
        .parent()
        .ok_or_else(|| ProjectError::NotFound(root.to_path_buf()))?;
    let new_root = parent.join(new_name);
    // Nur Groß-/Kleinschreibung zu ändern ist erlaubt, obwohl das Dateisystem beide Namen für denselben Ordner hält.
    if new_root.exists() && !old_name.eq_ignore_ascii_case(new_name) {
        return Err(ProjectError::AlreadyExists(new_root));
    }

    let main = find_main_file(root, root)?;
    let new_main = root.join(format!("{new_name}.ino"));
    let config_path = root.join(CONFIG_FILE);
    let old_config = std::fs::read(&config_path).ok();

    if let Ok(mut cfg) = ForgeToml::load(&config_path) {
        cfg.project.name = new_name.to_string();
        cfg.save(&config_path)?;
    }
    let undo = |config: &Option<Vec<u8>>| {
        if let Some(bytes) = config {
            let _ = std::fs::write(&config_path, bytes);
        }
    };
    if main != new_main {
        if let Err(e) = std::fs::rename(&main, &new_main) {
            undo(&old_config);
            return Err(e.into());
        }
    }
    if let Err(e) = std::fs::rename(root, &new_root) {
        if main != new_main {
            let _ = std::fs::rename(&new_main, &main);
        }
        undo(&old_config);
        return Err(e.into());
    }
    Ok(new_root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_sketch, open, ProjectKind};

    #[test]
    fn sketch_folder_and_main_file_follow_the_new_name() {
        let tmp = tempfile::tempdir().unwrap();
        let s = create_sketch(tmp.path()).unwrap();
        std::fs::write(s.path.join("helfer.cpp"), "int x;").unwrap();
        let new_root = rename_dir(&s.path, "wetterstation").unwrap();
        assert_eq!(new_root, s.path.parent().unwrap().join("wetterstation"));
        assert!(!s.path.exists());
        assert!(new_root.join("wetterstation.ino").is_file());
        assert!(new_root.join("helfer.cpp").is_file());
        let info = open(&new_root).unwrap();
        assert_eq!(info.kind, ProjectKind::Sketch);
        assert_eq!(info.name, "wetterstation");
    }

    #[test]
    fn project_name_in_forge_toml_is_updated() {
        let tmp = tempfile::tempdir().unwrap();
        let s = create_sketch(tmp.path()).unwrap();
        let mut cfg = ForgeToml::for_sketch(&s.name);
        cfg.project.id = Some("fest".into());
        cfg.save(&s.path.join(CONFIG_FILE)).unwrap();
        let new_root = rename_dir(&s.path, "neu").unwrap();
        let info = open(&new_root).unwrap();
        assert_eq!(info.kind, ProjectKind::Project);
        assert_eq!(info.config.project.name, "neu");
        assert_eq!(info.config.project.id.as_deref(), Some("fest"));
    }

    #[test]
    fn refuses_bad_and_taken_names_and_changes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let a = create_sketch(tmp.path()).unwrap();
        let b = create_sketch(tmp.path()).unwrap();
        assert!(matches!(
            rename_dir(&a.path, "mit leerzeichen"),
            Err(ProjectError::InvalidName(_))
        ));
        assert!(matches!(
            rename_dir(&a.path, &b.name),
            Err(ProjectError::AlreadyExists(_))
        ));
        assert!(a.path.join(format!("{}.ino", a.name)).is_file());
        // Derselbe Name ist kein Fehler und tut nichts.
        assert_eq!(rename_dir(&a.path, &a.name).unwrap(), a.path);
    }

    #[test]
    fn only_changing_the_case_works() {
        let tmp = tempfile::tempdir().unwrap();
        let s = create_sketch(tmp.path()).unwrap();
        let first = rename_dir(&s.path, "abc").unwrap();
        let second = rename_dir(&first, "ABC").unwrap();
        assert!(second.join("ABC.ino").is_file());
    }
}
