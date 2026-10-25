use crate::config::{AiConfig, BoardConfig, ForgeToml, ProjectMeta, SecretsConfig};
use crate::templates::Template;
use crate::{open, ProjectError, ProjectInfo, Result, CONFIG_FILE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    pub name: String,
    /// Ordner, in dem der Projektordner angelegt wird.
    pub parent: PathBuf,
    pub fqbn: String,
    pub template: Template,
    #[serde(default)]
    pub description: String,
    #[serde(default = "yes")]
    pub git: bool,
}

fn yes() -> bool {
    true
}

/// Arduino-kompatible Namen: Buchstaben, Ziffern, `_`, `-`, `.`; nicht mit Punkt beginnend; höchstens 63 Zeichen.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(ProjectError::InvalidName("Gib einen Namen ein.".into()));
    }
    if name.chars().count() > 63 {
        return Err(ProjectError::InvalidName(
            "Der Name darf höchstens 63 Zeichen haben.".into(),
        ));
    }
    if name.starts_with('.') {
        return Err(ProjectError::InvalidName(
            "Der Name darf nicht mit einem Punkt beginnen.".into(),
        ));
    }
    if let Some(bad) = name
        .chars()
        .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
    {
        let shown = if bad == ' ' {
            "Leerzeichen".to_string()
        } else {
            format!("„{bad}“")
        };
        return Err(ProjectError::InvalidName(format!(
            "{shown} ist im Namen nicht erlaubt. Erlaubt sind Buchstaben, Ziffern, _ - und .",
        )));
    }
    Ok(())
}

const GITIGNORE: &str = ".forge/\nbuild/\n.DS_Store\n";

/// Zufällige Kennung aus Zeit, Prozess und Pfad.
pub fn new_id(seed: &Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
        .hash(&mut h);
    std::process::id().hash(&mut h);
    seed.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn create_project(req: &CreateRequest) -> Result<ProjectInfo> {
    validate_name(&req.name)?;
    let dir = req.parent.join(&req.name);
    if dir.exists() {
        return Err(ProjectError::AlreadyExists(dir));
    }
    std::fs::create_dir_all(&dir)?;

    let rendered = req.template.render(&req.name);
    std::fs::write(dir.join(format!("{}.ino", req.name)), rendered.main)?;
    std::fs::write(dir.join(".gitignore"), GITIGNORE)?;

    let config = ForgeToml {
        project: ProjectMeta {
            id: Some(new_id(&dir)),
            name: req.name.clone(),
            description: req.description.clone(),
        },
        board: BoardConfig {
            fqbn: req.fqbn.clone(),
            options: BTreeMap::new(),
        },
        libraries: BTreeMap::new(),
        secrets: SecretsConfig {
            names: rendered.secrets.iter().map(|s| s.to_string()).collect(),
        },
        ai: AiConfig::default(),
    };
    config.save(&dir.join(CONFIG_FILE))?;

    if req.git {
        // Git ist optional. Fehlt es, bleibt das Projekt trotzdem vollständig.
        let _ = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    open(&dir)
}

/// Macht aus einem Sketch ein Projekt: schreibt `forge.toml` in seinen Ordner. Der Code bleibt unberührt.
/// `id` ist die bisherige Kennung des Sketches, damit seine Secrets im Schlüsselbund erhalten bleiben.
pub fn convert_to_project(
    root: &Path,
    id: String,
    board: BoardConfig,
    libraries: BTreeMap<String, String>,
    secrets: Vec<String>,
) -> Result<ProjectInfo> {
    let config_path = root.join(CONFIG_FILE);
    if config_path.exists() {
        return Err(ProjectError::AlreadyExists(config_path));
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "sketch".into());
    ForgeToml {
        project: ProjectMeta {
            id: Some(id),
            name,
            description: String::new(),
        },
        board,
        libraries,
        secrets: SecretsConfig { names: secrets },
        ai: AiConfig::default(),
    }
    .save(&config_path)?;
    let ignore = root.join(".gitignore");
    if !ignore.exists() {
        std::fs::write(ignore, GITIGNORE)?;
    }
    open(root)
}

/// Legt einen Sketch im Arduino-Stil an (`sketch_okt25a/sketch_okt25a.ino`), ohne Rückfragen.
pub fn create_sketch(parent: &Path) -> Result<ProjectInfo> {
    create_sketch_avoiding(parent, |_| false)
}

/// Wie [`create_sketch`], überspringt aber zusätzlich Namen, die `taken` als belegt meldet. So vergibt Forge keinen
/// Namen doppelt, wenn der Sketch an einem anderen Ort als `parent` gespeichert wird.
pub fn create_sketch_avoiding(parent: &Path, taken: impl Fn(&str) -> bool) -> Result<ProjectInfo> {
    let (month, day) = today();
    let base = format!("sketch_{month}{day:02}");
    let mut dir = None;
    for letter in 'a'..='z' {
        let name = format!("{base}{letter}");
        let candidate = parent.join(&name);
        if !candidate.exists() && !taken(&name) {
            dir = Some(candidate);
            break;
        }
    }
    let dir = dir.ok_or_else(|| ProjectError::AlreadyExists(parent.join(&base)))?;
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join(format!("{name}.ino")),
        Template::Blank.render(&name).main,
    )?;
    open(&dir)
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Monat (Arduino-Kürzel) und Tag des Monats in UTC.
fn today() -> (&'static str, u32) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    civil_from_days((secs / 86_400) as i64)
}

fn civil_from_days(z: i64) -> (&'static str, u32) {
    // Howard Hinnants Algorithmus.
    let z = z + 719_468;
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as usize;
    (MONTHS[m - 1], d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectKind;

    fn req(parent: &Path, name: &str, template: Template) -> CreateRequest {
        CreateRequest {
            name: name.into(),
            parent: parent.into(),
            fqbn: "esp32:esp32:esp32s3".into(),
            template,
            description: String::new(),
            git: false,
        }
    }

    #[test]
    fn names() {
        assert!(validate_name("neopixel-sensor").is_ok());
        assert!(validate_name("v1.2_test").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name(".hidden").is_err());
        assert!(validate_name("mit leerzeichen")
            .unwrap_err()
            .to_string()
            .contains("Leerzeichen"));
        assert!(validate_name("umlaut-ä").is_err());
        assert!(validate_name(&"a".repeat(64)).is_err());
    }

    #[test]
    fn creates_project_with_secrets_for_wifi_template() {
        let tmp = tempfile::tempdir().unwrap();
        let info = create_project(&req(tmp.path(), "wetter", Template::Wifi)).unwrap();
        assert_eq!(info.kind, ProjectKind::Project);
        assert_eq!(info.config.secrets.names, vec!["WIFI_SSID", "WIFI_PASS"]);
        assert!(info.main_file.ends_with("wetter/wetter.ino"));
        let code = std::fs::read_to_string(&info.main_file).unwrap();
        assert!(code.contains("Secret(\"WIFI_PASS\")"));
        assert!(!code.contains("password"));
    }

    #[test]
    fn refuses_existing_folder() {
        let tmp = tempfile::tempdir().unwrap();
        create_project(&req(tmp.path(), "a", Template::Blank)).unwrap();
        assert!(matches!(
            create_project(&req(tmp.path(), "a", Template::Blank)),
            Err(ProjectError::AlreadyExists(_))
        ));
    }

    #[test]
    fn sketches_get_increasing_letters() {
        let tmp = tempfile::tempdir().unwrap();
        let a = create_sketch(tmp.path()).unwrap();
        let b = create_sketch(tmp.path()).unwrap();
        assert_eq!(a.kind, ProjectKind::Sketch);
        assert_ne!(a.path, b.path);
        assert!(a.name.ends_with('a') && b.name.ends_with('b'));
    }

    #[test]
    fn sketch_becomes_project_and_keeps_its_code_and_id() {
        let tmp = tempfile::tempdir().unwrap();
        let sketch = create_sketch(tmp.path()).unwrap();
        let before = std::fs::read(&sketch.main_file).unwrap();
        let info = convert_to_project(
            &sketch.path,
            "path-0123".into(),
            BoardConfig {
                fqbn: "esp32:esp32:esp32".into(),
                options: BTreeMap::new(),
            },
            BTreeMap::from([("DHT sensor library".to_string(), "1.4.7".to_string())]),
            vec!["WIFI_PASS".into()],
        )
        .unwrap();
        assert_eq!(info.kind, crate::ProjectKind::Project);
        assert_eq!(info.name, sketch.name);
        assert_eq!(info.config.board.fqbn, "esp32:esp32:esp32");
        assert_eq!(info.config.project.id.as_deref(), Some("path-0123"));
        assert_eq!(crate::secrets_id(&sketch.path), "path-0123");
        assert_eq!(std::fs::read(&sketch.main_file).unwrap(), before);
        assert!(sketch.path.join(".gitignore").is_file());
        // Ein zweites Mal geht nicht.
        assert!(matches!(
            convert_to_project(
                &sketch.path,
                "x".into(),
                BoardConfig::default(),
                BTreeMap::new(),
                vec![]
            ),
            Err(ProjectError::AlreadyExists(_))
        ));
    }

    #[test]
    fn civil_date_is_correct() {
        // 2026-10-25 liegt 20 751 Tage nach 1970-01-01.
        assert_eq!(civil_from_days(20_751), ("oct", 25));
        assert_eq!(civil_from_days(0), ("jan", 1));
    }
}
