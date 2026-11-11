use super::boardcfg::apply_sketch_board;
use super::package;
use crate::state::{AppState, PackageSession};
use forge_project::{
    create_project, create_sketch_avoiding, list_tree, open, CreateRequest, FileNode, ProjectInfo,
};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Standardordner für neue Projekte und Sketches: `~/Documents/Forge`.
#[tauri::command]
pub fn default_projects_dir(app: AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().document_dir().map_err(msg)?.join("Forge");
    std::fs::create_dir_all(&dir).map_err(msg)?;
    Ok(dir)
}

/// Öffnet einen Ordner, eine `.ino`-Datei oder eine `.forgeproj`-Datei.
#[tauri::command]
pub async fn project_open(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> Result<ProjectInfo, String> {
    if forge_package::is_package(&path) && path.is_file() {
        return package::open_package(&app, &state, &path).await;
    }
    // Erst prüfen, dann die alte Sitzung beenden: scheitert das Öffnen, bleibt das alte Projekt speicherbar.
    let mut info = open(&path).map_err(msg)?;
    package::end_session(&app).await;
    apply_sketch_board(&app, &mut info);
    state.set_root(info.path.clone());
    state.watcher.watch(app, info.path.clone());
    Ok(info)
}

/// Legt ein neues Projekt an. Es lebt als `<Name>.forgeproj` im gewählten Ordner, so wie jeder Sketch.
#[tauri::command]
pub async fn project_create(
    app: AppHandle,
    state: State<'_, AppState>,
    request: CreateRequest,
) -> Result<ProjectInfo, String> {
    // Die alte Sitzung beendet `open_package`, sobald die neue Datei sicher geöffnet wird.
    forge_project::validate_name(&request.name).map_err(msg)?;
    let file = request
        .parent
        .join(format!("{}.{}", request.name, forge_package::EXTENSION));
    if file.exists() || request.parent.join(&request.name).exists() {
        return Err(format!("{} existiert bereits.", file.display()));
    }
    std::fs::create_dir_all(&request.parent).map_err(msg)?;

    // Aufgebaut wird in einem Arbeitsordner. Die Projektdatei entsteht daraus und wird dann wie jede andere geöffnet.
    let tmp = package::workspaces_dir(&app)?
        .join(format!("new-{}", forge_project::new_id(&request.parent)));
    std::fs::create_dir_all(&tmp).map_err(msg)?;
    let result = async {
        // Ein Git-Repository gehört nicht in die Datei. Wer eines will, entpackt das Projekt als Ordner.
        let req = CreateRequest {
            parent: tmp.clone(),
            git: false,
            ..request.clone()
        };
        let info = create_project(&req).map_err(msg)?;
        let draft = PackageSession {
            package: file.clone(),
            id: forge_project::secrets_id(&info.path),
            root: info.path,
            libraries: vec![],
            cores: Default::default(),
        };
        package::write_package(&app, &state, &draft, &file, false).await?;
        Ok::<(), String>(())
    }
    .await;
    let _ = std::fs::remove_dir_all(&tmp);
    result?;
    package::open_package(&app, &state, &file).await
}

/// Legt einen neuen Sketch an. Er lebt als `.forgeproj` in `~/Documents/Forge/Sketches`.
#[tauri::command]
pub async fn sketch_create(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProjectInfo, String> {
    let sketches = default_projects_dir(app.clone())?.join("Sketches");
    std::fs::create_dir_all(&sketches).map_err(msg)?;
    let id = forge_project::new_id(&sketches);
    let work = package::workspaces_dir(&app)?.join(&id);
    std::fs::create_dir_all(&work).map_err(msg)?;

    let ext = forge_package::EXTENSION;
    let mut info = create_sketch_avoiding(&work, |name| {
        sketches.join(format!("{name}.{ext}")).exists() || sketches.join(name).exists()
    })
    .map_err(msg)?;
    let file = sketches.join(format!("{}.{ext}", info.name));
    info.package = Some(file.clone());
    // Erst jetzt steht der neue Sketch fest, also endet die alte Sitzung erst jetzt.
    package::end_session(&app).await;
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
    // Die Datei soll sofort existieren, nicht erst nach der ersten Änderung.
    package::save_now(&app).await?;
    Ok(info)
}

#[tauri::command(async)]
pub fn project_tree(state: State<AppState>) -> Result<Vec<FileNode>, String> {
    list_tree(&state.root()?).map_err(msg)
}

#[tauri::command]
pub fn project_validate_name(name: String) -> Option<String> {
    forge_project::validate_name(&name)
        .err()
        .map(|e| e.to_string())
}

#[tauri::command]
pub async fn project_close(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    package::end_session(&app).await;
    state.watcher.stop();
    if let Ok(mut g) = state.root.lock() {
        *g = None;
    }
    Ok(())
}

/// Der Pfad, mit dem die App gestartet wurde, falls es einen gibt. Gilt nur einmal.
#[tauri::command]
pub fn take_pending_open(state: State<AppState>) -> Option<PathBuf> {
    state.pending_open.lock().ok().and_then(|mut g| g.take())
}

/// Macht aus dem geöffneten Sketch ein Projekt (mit `forge.toml`). Lebt er in einem normalen Ordner, entsteht
/// zusätzlich neben dem Ordner eine `.forgeproj`-Datei, und Forge öffnet sie. Der Ordner bleibt unverändert
/// (bis auf die neue `forge.toml`) als Kopie liegen.
#[tauri::command]
pub async fn project_convert(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ProjectInfo, String> {
    let root = state.root()?;
    let session = state.package.lock().ok().and_then(|g| g.clone());
    let has_config = root.join(forge_project::CONFIG_FILE).exists();
    if session.is_some() && has_config {
        return Err("Das ist schon ein Projekt.".into());
    }
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let target = root.with_file_name(format!("{name}.{}", forge_package::EXTENSION));
    if session.is_none() && target.exists() {
        return Err(format!(
            "{} existiert bereits. Benenne eine der beiden Dateien um.",
            target.display()
        ));
    }

    if !has_config {
        // Die Kennung des Sketches bleibt, damit seine Secrets im Schlüsselbund erhalten bleiben.
        let id = forge_project::secrets_id(&root);
        let board = super::boardcfg::sketch_board(&app, &root)
            .map(|(fqbn, options)| forge_project::BoardConfig { fqbn, options })
            .unwrap_or_default();
        let (libraries, secrets) = package::project_facts(&state, &root).await;
        let r = root.clone();
        tokio::task::spawn_blocking(move || {
            forge_project::convert_to_project(&r, id, board, libraries, secrets)
        })
        .await
        .map_err(msg)?
        .map_err(msg)?;
    }

    if let Some(session) = session {
        // Eine Projektdatei wechselt nur von „Sketch“ zu „Projekt“.
        let mut info = open(&root).map_err(msg)?;
        info.package = Some(session.package);
        package::touch(&app);
        return Ok(info);
    }

    // Ein Ordner wird zur Projektdatei, und Forge arbeitet ab jetzt in ihr.
    let draft = PackageSession {
        package: target.clone(),
        id: forge_project::secrets_id(&root),
        root,
        libraries: vec![],
        cores: Default::default(),
    };
    package::write_package(&app, &state, &draft, &target, false).await?;
    package::open_package(&app, &state, &target).await
}

/// Benennt das geöffnete Projekt um: Ordner, Haupt-`.ino`, Name in `forge.toml` und, wenn es in einer
/// `.forgeproj`-Datei lebt und die Datei so hieß wie das Projekt, auch die Datei.
#[tauri::command]
pub async fn project_rename(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<ProjectInfo, String> {
    let name = name.trim().to_string();
    let old_root = state.root()?;
    let old_name = old_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name == old_name {
        let mut info = open(&old_root).map_err(msg)?;
        apply_sketch_board(&app, &mut info);
        info.package = state
            .package
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.package.clone()));
        return Ok(info);
    }

    let info = {
        // Solange umgebaut wird, darf keine Speicherung laufen.
        let _no_save = state.package_save.lock().await;
        let session = state.package.lock().ok().and_then(|g| g.clone());

        // Die Datei zieht nur um, wenn sie nach dem Projekt hieß. Wer sie anders genannt hat, behält ihren Namen.
        let new_package = session.as_ref().and_then(|s| {
            let stem = s.package.file_stem()?.to_string_lossy().into_owned();
            (stem == old_name).then(|| {
                s.package
                    .with_file_name(format!("{name}.{}", forge_package::EXTENSION))
            })
        });
        if let Some(target) = &new_package {
            if target.exists() {
                return Err(format!(
                    "{} existiert bereits. Wähle einen anderen Namen.",
                    target.display()
                ));
            }
        }

        // Bei Sketches hängt die Kennung der Secrets am Pfad und ändert sich mit ihm.
        let old_id = forge_project::secrets_id(&old_root);
        let (declared, used) = super::secrets::project_secret_names(&old_root);
        let secret_names: Vec<String> = declared.into_iter().chain(used).collect();

        let (r, n) = (old_root.clone(), name.clone());
        let new_root = tokio::task::spawn_blocking(move || forge_project::rename_dir(&r, &n))
            .await
            .map_err(msg)?
            .map_err(msg)?;

        if let (Some(s), Some(target)) = (&session, &new_package) {
            if let Err(e) = std::fs::rename(&s.package, target) {
                let _ = forge_project::rename_dir(&new_root, &old_name);
                return Err(format!(
                    "Die Projektdatei konnte nicht umbenannt werden: {e}"
                ));
            }
        }

        super::boardcfg::move_sketch_board(&app, &old_root, &new_root);
        let new_id = forge_project::secrets_id(&new_root);
        if new_id != old_id {
            let migrated = tokio::task::spawn_blocking(move || {
                forge_secrets::keychain::migrate_project(
                    &super::secrets::store(),
                    &old_id,
                    &new_id,
                    &secret_names,
                )
            })
            .await;
            // Umbenennen gelingt trotzdem, die Secrets bleiben dann unter der alten Kennung.
            match migrated {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => log::warn!("Secrets ließen sich nicht umziehen: {e}"),
                Err(e) => log::warn!("Secrets ließen sich nicht umziehen: {e}"),
            }
        }

        if let Ok(mut g) = state.package.lock() {
            if let Some(cur) = g.as_mut() {
                cur.root = new_root.clone();
                if let Some(target) = new_package {
                    cur.package = target;
                }
            }
        }
        state.set_root(new_root.clone());
        state.watcher.watch(app.clone(), new_root.clone());

        let mut info = open(&new_root).map_err(msg)?;
        apply_sketch_board(&app, &mut info);
        info.package = state
            .package
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|s| s.package.clone()));
        info
    };
    // Der Name steht auch im Manifest der Projektdatei.
    if info.package.is_some() {
        package::save_now(&app).await?;
    }
    Ok(info)
}

/// Schaltet Einträge der Menüleiste ein oder aus, je nachdem, was das geöffnete Projekt erlaubt.
#[tauri::command]
pub fn menu_set_enabled(app: AppHandle, states: std::collections::HashMap<String, bool>) {
    let states: Vec<(String, bool)> = states.into_iter().collect();
    crate::menu::set_enabled(&app, &states);
}
