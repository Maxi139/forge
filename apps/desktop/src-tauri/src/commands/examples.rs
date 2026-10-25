use super::package;
use crate::state::{AppState, PackageSession};
use forge_project::{open, ProjectInfo};
use forge_toolchain::models::ExampleInfo;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Beispiele des ESP32-Cores und aller installierten Bibliotheken. Mit Board nur die passenden.
#[tauri::command]
pub async fn examples_list(
    state: State<'_, AppState>,
    fqbn: Option<String>,
) -> Result<Vec<ExampleInfo>, String> {
    state
        .toolchain
        .cli()?
        .examples(fqbn.as_deref())
        .await
        .map_err(msg)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)?.flatten() {
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Ordnername des Beispiels, bereinigt zu einem gültigen Projektnamen.
fn project_name(dir: &Path) -> String {
    let raw = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = name.trim_start_matches('.').to_string();
    if name.is_empty() {
        "Beispiel".into()
    } else {
        name
    }
}

/// Öffnet ein Beispiel als Kopie. Das Original im Core oder in der Bibliothek bleibt unberührt, die Kopie lebt
/// wie jeder Sketch als `.forgeproj` in `~/Documents/Forge/Beispiele`.
#[tauri::command]
pub async fn example_open(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<ProjectInfo, String> {
    if !path.is_dir() {
        return Err("Das Beispiel gibt es nicht mehr. Aktualisiere die Liste.".into());
    }
    package::end_session(&app).await;
    let dir = super::project::default_projects_dir(app.clone())?.join("Beispiele");
    std::fs::create_dir_all(&dir).map_err(msg)?;
    let ext = forge_package::EXTENSION;
    let base = project_name(&path);
    let taken = |n: &str| dir.join(format!("{n}.{ext}")).exists() || dir.join(n).exists();
    let mut name = base.clone();
    let mut i = 1;
    while taken(&name) {
        i += 1;
        name = format!("{base}_{i}");
    }

    let id = forge_project::new_id(&dir);
    let work = package::workspaces_dir(&app)?.join(&id);
    let root = work.join(&name);
    copy_dir(&path, &root).map_err(msg)?;
    // Der Hauptsketch heißt wie der Ordner. Bei abweichendem Namen passen wir ihn an.
    if !root.join(format!("{name}.ino")).is_file() {
        let first = std::fs::read_dir(&root)
            .map_err(msg)?
            .flatten()
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino")))
            .ok_or("Im Beispiel liegt keine .ino-Datei.")?;
        std::fs::rename(first, root.join(format!("{name}.ino"))).map_err(msg)?;
    }
    let mut info = open(&root).map_err(msg)?;
    let file = dir.join(format!("{}.{ext}", info.name));
    info.package = Some(file.clone());
    package::set_session(
        &state,
        PackageSession {
            package: file,
            root: info.path.clone(),
            id,
            libraries: vec![],
            cores: Default::default(),
        },
    );
    state.set_root(info.path.clone());
    state.watcher.watch(app.clone(), info.path.clone());
    package::save_now(&app).await?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_made_valid() {
        assert_eq!(project_name(Path::new("/x/Blink")), "Blink");
        assert_eq!(project_name(Path::new("/x/My Example!")), "My_Example_");
        assert_eq!(project_name(Path::new("/x/.hidden")), "hidden");
    }

    #[test]
    fn copies_subfolders() {
        let tmp = tempfile::tempdir().unwrap();
        let from = tmp.path().join("a");
        std::fs::create_dir_all(from.join("data")).unwrap();
        std::fs::write(from.join("a.ino"), "x").unwrap();
        std::fs::write(from.join("data/i.txt"), "y").unwrap();
        copy_dir(&from, &tmp.path().join("b")).unwrap();
        assert!(tmp.path().join("b/data/i.txt").is_file());
    }
}
