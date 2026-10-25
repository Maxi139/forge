//! `.forgeproj` im Betrieb. Eine geöffnete Projektdatei wird in einen Arbeitsordner entpackt, in dem der Rest von
//! Forge wie gewohnt arbeitet (Editor, Build, clangd). Änderungen wandern kurz nach dem Tippen zurück in die Datei.

use super::boardcfg;
use super::libraries::{self, Op};
use crate::state::{AppState, PackageSession};
use forge_package::libs::{self as pkglibs, Installed};
use forge_package::{
    AiInfo, Board, Kind, Library, LibrarySource, Manifest, PackRequest, Toolchain, EXTENSION,
};
use forge_project::{ForgeToml, ProjectInfo, CONFIG_FILE};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering::SeqCst;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

/// So lange wartet Forge nach der letzten Änderung, bevor es die Datei neu schreibt.
const SAVE_DELAY: Duration = Duration::from_millis(1500);
const MARKER: &str = ".synced";
const SOURCE_EXTS: &[&str] = &["ino", "cpp", "c", "cc", "h", "hpp"];

fn msg(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn workspaces_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(msg)?.join("workspaces");
    std::fs::create_dir_all(&dir).map_err(msg)?;
    Ok(dir)
}

/// Kennungen werden zu Ordnernamen. Eine Datei von außen darf dabei nichts anderes einschmuggeln.
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn marker(root: &Path) -> Option<PathBuf> {
    root.parent().map(|p| p.join(MARKER))
}

/// Entpackt (falls nötig) und öffnet eine `.forgeproj`-Datei.
pub async fn open_package(
    app: &AppHandle,
    state: &AppState,
    path: &Path,
) -> Result<ProjectInfo, String> {
    end_session(app).await;
    let package = path.canonicalize().map_err(msg)?;
    let work = workspaces_dir(app)?;

    let p = package.clone();
    let (manifest, digest) = tokio::task::spawn_blocking(move || {
        let manifest = forge_package::inspect(&p)?.manifest;
        let digest = forge_package::digest(&p)?;
        Ok::<_, forge_package::PackageError>((manifest, digest))
    })
    .await
    .map_err(msg)?
    .map_err(msg)?;

    if !safe_id(&manifest.id) || forge_project::validate_name(&manifest.name).is_err() {
        return Err(
            "Die Projektdatei enthält einen ungültigen Namen oder eine ungültige Kennung.".into(),
        );
    }
    let ws = work.join(&manifest.id);
    let dir = ws.join(&manifest.name);

    // Ist der Arbeitsordner noch der Stand dieser Datei, bleibt er. So gehen Build-Zwischenstände nicht verloren.
    let current = marker(&dir)
        .and_then(|m| std::fs::read_to_string(m).ok())
        .is_some_and(|d| d.trim() == digest)
        && dir.is_dir();
    if !current {
        let (p, d, ws2, dg) = (package.clone(), dir.clone(), ws.clone(), digest.clone());
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let _ = std::fs::remove_dir_all(&ws2);
            std::fs::create_dir_all(&d).map_err(msg)?;
            if let Err(e) = forge_package::unpack(&p, &d) {
                let _ = std::fs::remove_dir_all(&ws2);
                return Err(msg(e));
            }
            if let Some(m) = marker(&d) {
                std::fs::write(m, dg).map_err(msg)?;
            }
            Ok(())
        })
        .await
        .map_err(msg)??;
    }

    let dir = dir.canonicalize().map_err(msg)?;
    if manifest.kind == Kind::Sketch && (!current || boardcfg::sketch_board(app, &dir).is_none()) {
        boardcfg::store_sketch_board(
            app,
            &dir,
            &manifest.board.fqbn,
            manifest.board.options.clone(),
        )?;
    }
    let mut info = forge_project::open(&dir).map_err(msg)?;
    boardcfg::apply_sketch_board(app, &mut info);
    info.package = Some(package.clone());

    set_session(
        state,
        PackageSession {
            package,
            root: info.path.clone(),
            id: manifest.id.clone(),
            libraries: manifest.libraries.clone(),
            cores: manifest.toolchain.cores.clone(),
        },
    );
    state.set_root(info.path.clone());
    state.watcher.watch(app.clone(), info.path.clone());
    if current {
        // Der Arbeitsordner kann neuer sein als die Datei (Absturz vor dem Speichern).
        touch(app);
    }
    Ok(info)
}

pub fn set_session(state: &AppState, session: PackageSession) {
    state.package_gen.fetch_add(1, SeqCst);
    state.package_dirty.store(false, SeqCst);
    if let Ok(mut g) = state.package.lock() {
        *g = Some(session);
    }
}

/// Schreibt offene Änderungen und löst die Verbindung zur Datei.
pub async fn end_session(app: &AppHandle) {
    flush(app).await;
    let state = app.state::<AppState>();
    state.package_gen.fetch_add(1, SeqCst);
    let _ = state.package.lock().map(|mut g| *g = None);
}

/// Speichert, wenn es etwas zu speichern gibt.
pub async fn flush(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.package_dirty.load(SeqCst) {
        if let Err(e) = save_now(app).await {
            let _ = app.emit("package:error", e);
        }
    }
}

/// Meldet, dass sich etwas geändert hat. Gespeichert wird, sobald 1,5 Sekunden Ruhe sind.
pub fn touch(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.package.lock().map(|g| g.is_none()).unwrap_or(true) {
        return;
    }
    state.package_dirty.store(true, SeqCst);
    let generation = state.package_gen.fetch_add(1, SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DELAY).await;
        if app.state::<AppState>().package_gen.load(SeqCst) != generation {
            return;
        }
        if let Err(e) = save_now(&app).await {
            let _ = app.emit("package:error", e);
        }
    });
}

/// Schreibt die Projektdatei jetzt.
pub async fn save_now(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _one_at_a_time = state.package_save.lock().await;
    let Some(session) = state.package.lock().ok().and_then(|g| g.clone()) else {
        return Ok(());
    };
    // Vorher löschen: Änderungen während des Schreibens setzen das Zeichen neu.
    state.package_dirty.store(false, SeqCst);
    match write_package(app, &state, &session, &session.package, true).await {
        Ok(updated) => {
            // Nur übernehmen, wenn in der Zwischenzeit kein anderes Projekt geöffnet wurde.
            if let Ok(mut g) = state.package.lock() {
                if let Some(cur) = g.as_mut().filter(|c| c.package == session.package) {
                    cur.libraries = updated.libraries;
                    cur.cores = updated.cores;
                }
            }
            Ok(())
        }
        Err(e) => {
            state.package_dirty.store(true, SeqCst);
            Err(e)
        }
    }
}

struct Config {
    kind: Kind,
    id: String,
    name: String,
    description: String,
    board: Board,
    secrets: Vec<String>,
    ai_enabled: bool,
    declared: BTreeMap<String, String>,
}

fn read_config(app: &AppHandle, session: &PackageSession) -> Config {
    let root = &session.root;
    if let Ok(cfg) = ForgeToml::load(&root.join(CONFIG_FILE)) {
        return Config {
            kind: Kind::Project,
            id: forge_project::secrets_id(root),
            name: cfg.project.name,
            description: cfg.project.description,
            board: Board {
                fqbn: cfg.board.fqbn,
                options: cfg.board.options,
            },
            secrets: cfg.secrets.names,
            ai_enabled: cfg.ai.enabled,
            declared: cfg.libraries,
        };
    }
    let (fqbn, options) = boardcfg::sketch_board(app, root)
        .unwrap_or_else(|| (forge_project::BoardConfig::default().fqbn, BTreeMap::new()));
    Config {
        kind: Kind::Sketch,
        id: session.id.clone(),
        name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "sketch".into()),
        description: String::new(),
        board: Board { fqbn, options },
        secrets: vec![],
        ai_enabled: true,
        declared: BTreeMap::new(),
    }
}

fn read_sources(root: &Path) -> Vec<String> {
    forge_toolchain::hash::source_files(root)
        .into_iter()
        .filter(|p| {
            p.extension()
                .is_some_and(|e| SOURCE_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str()))
        })
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect()
}

/// Bibliotheken (Name zu Version) und Secret-Namen, die der Code eines Ordners braucht. Für `forge.toml`.
pub async fn project_facts(
    state: &AppState,
    root: &Path,
) -> (BTreeMap<String, String>, Vec<String>) {
    let (sources, secrets) = {
        let root = root.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let sources = read_sources(&root);
            let secrets: Vec<String> = sources
                .iter()
                .flat_map(|t| forge_secrets::scan::used_names(t))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            (sources, secrets)
        })
        .await
        .unwrap_or_default()
    };
    let mut libraries = BTreeMap::new();
    if let Ok(meta) = state.toolchain.meta().await {
        let installed: Vec<Installed> = meta
            .libraries
            .iter()
            .map(|l| Installed {
                name: l.name.clone(),
                version: l.version.clone(),
                user: l.location == "user",
                includes: l.provides_includes.clone(),
                dir: l.install_dir.clone(),
            })
            .collect();
        let headers = pkglibs::included_headers(sources.iter().map(|s| s.as_str()));
        for lib in pkglibs::used(&headers, &installed) {
            // ForgeSecrets legt Forge selbst an.
            if lib.name != "ForgeSecrets" {
                libraries.insert(lib.name.clone(), lib.version.clone());
            }
        }
    }
    (libraries, secrets)
}

/// Bibliotheken und Core des Projekts. Ohne `arduino-cli` bleibt der Stand der letzten Speicherung.
async fn resolve_toolchain(
    state: &AppState,
    session: &PackageSession,
    cfg: &Config,
    sources: &[String],
) -> (
    Vec<Library>,
    BTreeMap<String, String>,
    Vec<(String, PathBuf)>,
) {
    let Ok(meta) = state.toolchain.meta().await else {
        return (session.libraries.clone(), session.cores.clone(), vec![]);
    };
    let installed: Vec<Installed> = meta
        .libraries
        .iter()
        .map(|l| Installed {
            name: l.name.clone(),
            version: l.version.clone(),
            user: l.location == "user",
            includes: l.provides_includes.clone(),
            dir: l.install_dir.clone(),
        })
        .collect();

    let headers = pkglibs::included_headers(sources.iter().map(|s| s.as_str()));
    let cli = state.toolchain.cli().ok();
    let mut libraries: Vec<Library> = Vec::new();
    let mut embedded: Vec<(String, PathBuf)> = Vec::new();
    for lib in pkglibs::used(&headers, &installed) {
        let known = session
            .libraries
            .iter()
            .find(|l| l.name == lib.name && l.version == lib.version);
        let source = if lib.name == "ForgeSecrets" {
            LibrarySource::Forge
        } else if let Some(k) = known {
            k.source
        } else {
            match &cli {
                Some(cli) => match cli.index_has(&lib.name, &lib.version).await {
                    Ok(true) => LibrarySource::Registry,
                    // Nicht erreichbar oder nicht im Verzeichnis: dann reist der Quelltext mit.
                    _ => LibrarySource::Embedded,
                },
                None => LibrarySource::Embedded,
            }
        };
        let dir = (source == LibrarySource::Embedded).then(|| pkglibs::dir_name(&lib.name));
        match (&source, &dir, &lib.dir) {
            (LibrarySource::Embedded, Some(d), Some(src)) => {
                embedded.push((d.clone(), src.clone()))
            }
            // Ohne Quelltext lässt sich nichts einbetten. Dann bleibt nur der Verweis.
            (LibrarySource::Embedded, _, None) => {
                libraries.push(Library {
                    name: lib.name.clone(),
                    version: lib.version.clone(),
                    source: LibrarySource::Registry,
                    dir: None,
                });
                continue;
            }
            _ => {}
        }
        libraries.push(Library {
            name: lib.name.clone(),
            version: lib.version.clone(),
            source,
            dir,
        });
    }
    // Was `forge.toml` ausdrücklich verlangt, gehört dazu, auch wenn der Code es (noch) nicht einbindet.
    for (name, version) in &cfg.declared {
        if !libraries.iter().any(|l| &l.name == name) {
            libraries.push(Library {
                name: name.clone(),
                version: version.clone(),
                source: LibrarySource::Registry,
                dir: None,
            });
        }
    }
    libraries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    let core_id = cfg
        .board
        .fqbn
        .splitn(3, ':')
        .take(2)
        .collect::<Vec<_>>()
        .join(":");
    let mut cores = BTreeMap::new();
    let version = meta.core_version(&cfg.board.fqbn);
    if !version.is_empty() {
        cores.insert(core_id, version);
    } else {
        cores = session.cores.clone();
    }
    (libraries, cores, embedded)
}

fn now_label() -> String {
    format!("Forge {}", env!("CARGO_PKG_VERSION"))
}

/// Packt den Arbeitsordner nach `dest`. Mit `sync` merkt sich Forge den Stand der Datei, damit es beim nächsten
/// Öffnen weiß, dass der Arbeitsordner noch aktuell ist.
pub(crate) async fn write_package(
    app: &AppHandle,
    state: &AppState,
    session: &PackageSession,
    dest: &Path,
    sync: bool,
) -> Result<PackageSession, String> {
    let root = session.root.clone();
    let cfg = read_config(app, session);
    let sources = {
        let root = root.clone();
        tokio::task::spawn_blocking(move || read_sources(&root))
            .await
            .map_err(msg)?
    };
    let (libraries, cores, embedded) = resolve_toolchain(state, session, &cfg, &sources).await;
    let manifest = Manifest {
        format: forge_package::FORMAT,
        kind: cfg.kind,
        id: cfg.id,
        name: cfg.name,
        description: cfg.description,
        app: now_label(),
        saved_at: 0,
        board: cfg.board,
        toolchain: Toolchain {
            cores: cores.clone(),
        },
        libraries: libraries.clone(),
        secrets: cfg.secrets,
        ai: AiInfo {
            enabled: cfg.ai_enabled,
            cached: false,
        },
    };
    let dest = dest.to_path_buf();
    let package_marker = marker(&root);
    let previous = session.package.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let mut embedded = embedded;
        // Ohne arduino-cli kennen wir die Quellordner eingebetteter Bibliotheken nicht. Dann bleiben sie,
        // wie sie in der bisherigen Datei sind.
        let scratch = std::env::temp_dir().join(format!("forge-pack-{}", std::process::id()));
        for lib in manifest
            .libraries
            .iter()
            .filter(|l| l.source == LibrarySource::Embedded)
        {
            let dir = lib.dir.clone().unwrap_or_else(|| lib.name.clone());
            if !embedded.iter().any(|(d, _)| *d == dir) {
                if let Ok(p) = forge_package::extract_library(&previous, &dir, &scratch) {
                    embedded.push((dir, p));
                }
            }
        }
        let result = forge_package::pack(
            PackRequest {
                manifest,
                root: &root,
                embedded,
            },
            &dest,
        )
        .map_err(msg);
        let _ = std::fs::remove_dir_all(&scratch);
        result?;
        if sync {
            if let (Some(m), Ok(d)) = (package_marker, forge_package::digest(&dest)) {
                let _ = std::fs::write(m, d);
            }
        }
        Ok(())
    })
    .await
    .map_err(msg)??;
    Ok(PackageSession {
        libraries,
        cores,
        ..session.clone()
    })
}

/// Was auf diesem Rechner fehlt, damit das Projekt so baut wie beim Absender.
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Needs {
    pub libraries: Vec<Need>,
    pub core: Option<CoreNeed>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Need {
    pub name: String,
    pub version: String,
    /// `registry` (aus dem Bibliotheksverwalter) oder `embedded` (aus der Projektdatei).
    pub source: String,
    /// Installierte Version, falls eine andere vorhanden ist. Forge ersetzt sie nicht von selbst.
    pub installed: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreNeed {
    pub id: String,
    pub wanted: String,
    pub installed: Option<String>,
}

fn compare(session: &PackageSession, meta: &crate::state::Meta) -> Needs {
    let mut needs = Needs::default();
    for lib in &session.libraries {
        if lib.source == LibrarySource::Forge {
            continue;
        }
        let have = meta.libraries.iter().find(|i| i.name == lib.name);
        if have.is_some_and(|h| h.version == lib.version) {
            continue;
        }
        needs.libraries.push(Need {
            name: lib.name.clone(),
            version: lib.version.clone(),
            source: match lib.source {
                LibrarySource::Embedded => "embedded",
                _ => "registry",
            }
            .into(),
            installed: have.map(|h| h.version.clone()),
        });
    }
    for (id, wanted) in &session.cores {
        let installed = meta.core_versions.get(id).cloned();
        if installed.as_deref() != Some(wanted.as_str()) {
            needs.core = Some(CoreNeed {
                id: id.clone(),
                wanted: wanted.clone(),
                installed,
            });
        }
    }
    needs
}

#[tauri::command]
pub async fn package_needs(state: State<'_, AppState>) -> Result<Needs, String> {
    let Some(session) = state.package.lock().ok().and_then(|g| g.clone()) else {
        return Ok(Needs::default());
    };
    // Ohne arduino-cli zeigt Forge ohnehin die Einrichtung. Hier gibt es dann nichts zu vergleichen.
    let Ok(meta) = state.toolchain.meta().await else {
        return Ok(Needs::default());
    };
    Ok(compare(&session, &meta))
}

/// Installiert die fehlenden Bibliotheken. Aus dem Verzeichnis in der gespeicherten Version, eingebettete aus der Datei.
#[tauri::command]
pub async fn package_install_needs(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let Some(session) = state.package.lock().ok().and_then(|g| g.clone()) else {
        return Ok(());
    };
    let meta = state.toolchain.meta().await?;
    for need in compare(&session, &meta)
        .libraries
        .into_iter()
        .filter(|n| n.installed.is_none())
    {
        if need.source == "embedded" {
            let dir = session
                .libraries
                .iter()
                .find(|l| l.name == need.name)
                .and_then(|l| l.dir.clone())
                .ok_or_else(|| {
                    format!("Die Bibliothek {} fehlt in der Projektdatei.", need.name)
                })?;
            let target = state
                .toolchain
                .cli()?
                .user_libraries_dir()
                .await
                .map_err(msg)?;
            let package = session.package.clone();
            tokio::task::spawn_blocking(move || {
                std::fs::create_dir_all(&target).map_err(msg)?;
                forge_package::extract_library(&package, &dir, &target).map_err(msg)
            })
            .await
            .map_err(msg)??;
            state.toolchain.invalidate();
            let _ = app.emit("library:changed", &need.name);
        } else {
            libraries::run(
                &app,
                &state,
                need.name.clone(),
                Op::Install(Some(need.version.clone())),
            )
            .await
            .map_err(|e| format!("{} {}: {e}", need.name, need.version))?;
        }
    }
    Ok(())
}

fn with_extension(path: PathBuf) -> PathBuf {
    if forge_package::is_package(&path) {
        path
    } else {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(".{EXTENSION}"));
        path.with_file_name(name)
    }
}

/// Speichert das geöffnete Projekt unter einem neuen Namen oder Ort als `.forgeproj`. Forge arbeitet danach in der neuen
/// Datei weiter. Ein normaler Ordner wird dabei zur Projektdatei, der Ordner bleibt unverändert liegen.
#[tauri::command]
pub async fn package_save_as(
    app: AppHandle,
    state: State<'_, AppState>,
    dest: PathBuf,
) -> Result<forge_project::ProjectInfo, String> {
    let dest = with_extension(dest);
    let existing = state.package.lock().ok().and_then(|g| g.clone());
    let session = match &existing {
        Some(s) => {
            // Die neue Datei soll den aktuellen Stand haben.
            flush(&app).await;
            s.clone()
        }
        None => {
            let root = state.root()?;
            PackageSession {
                package: dest.clone(),
                id: forge_project::secrets_id(&root),
                root,
                libraries: vec![],
                cores: BTreeMap::new(),
            }
        }
    };
    // Bei einer Projektdatei bleibt der Arbeitsordner derselbe. Dann soll Forge ihn beim Öffnen wiederverwenden.
    write_package(&app, &state, &session, &dest, existing.is_some()).await?;
    open_package(&app, &state, &dest).await
}

/// Entpackt das geöffnete Projekt in einen normalen Ordner `<parent>/<Name>`.
#[tauri::command]
pub async fn package_export_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    parent: PathBuf,
) -> Result<PathBuf, String> {
    let Some(session) = state.package.lock().ok().and_then(|g| g.clone()) else {
        return Err("Dieses Projekt ist schon ein normaler Ordner.".into());
    };
    flush(&app).await;
    let name = session
        .root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "projekt".into());
    let dest = parent.join(&name);
    if dest.exists() {
        return Err(format!(
            "{} existiert bereits. Wähle einen anderen Ort.",
            dest.display()
        ));
    }
    let (package, d) = (session.package, dest.clone());
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&d).map_err(msg)?;
        forge_package::unpack(&package, &d).map_err(|e| {
            let _ = std::fs::remove_dir_all(&d);
            msg(e)
        })
    })
    .await
    .map_err(msg)??;
    Ok(dest)
}

/// Pfad der Vorschau-Erweiterung im App-Bundle, ausgehend von der laufenden Programmdatei.
fn appex_path(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    let appex = contents.join("PlugIns").join("ForgeQuickLook.appex");
    appex.is_dir().then_some(appex)
}

/// Meldet die Vorschau-Erweiterung bei macOS an, damit Finder und die Leertaste `.forgeproj`-Dateien zeigen.
/// Läuft nur, wenn sich die App geändert hat, und im Hintergrund.
#[cfg(target_os = "macos")]
pub fn register_quicklook(app: &AppHandle) {
    let Some(appex) = std::env::current_exe().ok().and_then(|e| appex_path(&e)) else {
        return;
    };
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let stamp = format!(
        "{}:{}",
        appex.display(),
        std::fs::metadata(&appex)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let marker = dir.join("quicklook-registered");
    if std::fs::read_to_string(&marker).is_ok_and(|s| s == stamp) {
        return;
    }
    std::thread::spawn(move || {
        use std::process::{Command, Stdio};
        let run = |cmd: &str, args: &[&str]| {
            Command::new(cmd)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        let ok = run("/usr/bin/pluginkit", &["-a", &appex.to_string_lossy()])
            && run(
                "/usr/bin/pluginkit",
                &["-e", "use", "-i", "cc.forge.ide.quicklook"],
            );
        // Alte Vorschauen aus dem Zwischenspeicher von Quick Look verwerfen.
        let _ = run("/usr/bin/qlmanage", &["-r", "cache"]);
        if ok {
            let _ = std::fs::create_dir_all(&dir);
            let _ = std::fs::write(marker, stamp);
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn register_quicklook(_app: &AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_cannot_escape_the_workspace_folder() {
        assert!(safe_id("1f2e3d4c5b6a7988"));
        assert!(safe_id("path-0123"));
        assert!(!safe_id(""));
        assert!(!safe_id(".."));
        assert!(!safe_id("../x"));
        assert!(!safe_id("a/b"));
        assert!(!safe_id(&"a".repeat(65)));
    }

    fn session(libs: Vec<Library>, cores: &[(&str, &str)]) -> PackageSession {
        PackageSession {
            package: PathBuf::from("/x/p.forgeproj"),
            root: PathBuf::from("/w/p"),
            id: "abc".into(),
            libraries: libs,
            cores: cores
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }

    fn installed(name: &str, version: &str) -> forge_toolchain::models::LibraryInfo {
        forge_toolchain::models::LibraryInfo {
            name: name.into(),
            version: version.into(),
            author: String::new(),
            sentence: String::new(),
            website: String::new(),
            location: "user".into(),
            install_dir: None,
            provides_includes: vec![],
            latest: None,
        }
    }

    fn lib(name: &str, version: &str, source: LibrarySource) -> Library {
        Library {
            name: name.into(),
            version: version.into(),
            source,
            dir: (source == LibrarySource::Embedded).then(|| name.to_string()),
        }
    }

    #[test]
    fn only_missing_or_different_things_are_reported() {
        let meta = crate::state::Meta {
            cli_version: "1.5.1".into(),
            core_versions: [("esp32:esp32".to_string(), "3.3.10".to_string())].into(),
            libraries: vec![installed("A", "1.0.0"), installed("B", "2.0.0")],
        };
        let s = session(
            vec![
                lib("A", "1.0.0", LibrarySource::Registry),
                lib("B", "2.1.0", LibrarySource::Registry),
                lib("C", "0.3.0", LibrarySource::Embedded),
                lib("ForgeSecrets", "1.0.0", LibrarySource::Forge),
            ],
            &[("esp32:esp32", "3.3.12")],
        );
        let needs = compare(&s, &meta);
        let summary: Vec<_> = needs
            .libraries
            .iter()
            .map(|n| (n.name.as_str(), n.source.as_str(), n.installed.as_deref()))
            .collect();
        assert_eq!(
            summary,
            vec![("B", "registry", Some("2.0.0")), ("C", "embedded", None)]
        );
        let core = needs.core.unwrap();
        assert_eq!(
            (core.wanted.as_str(), core.installed.as_deref()),
            ("3.3.12", Some("3.3.10"))
        );

        let same = crate::state::Meta {
            core_versions: [("esp32:esp32".to_string(), "3.3.12".to_string())].into(),
            ..meta
        };
        assert!(
            compare(&session(vec![], &[("esp32:esp32", "3.3.12")]), &same)
                .core
                .is_none()
        );
    }

    #[test]
    fn extension_is_appended_once() {
        assert_eq!(
            with_extension(PathBuf::from("/x/demo")),
            PathBuf::from("/x/demo.forgeproj")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/demo.forgeproj")),
            PathBuf::from("/x/demo.forgeproj")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/v1.2")),
            PathBuf::from("/x/v1.2.forgeproj")
        );
    }

    #[test]
    fn quicklook_is_found_next_to_the_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let exe = tmp.path().join("Forge.app/Contents/MacOS/forge-desktop");
        assert!(appex_path(&exe).is_none());
        std::fs::create_dir_all(
            tmp.path()
                .join("Forge.app/Contents/PlugIns/ForgeQuickLook.appex"),
        )
        .unwrap();
        assert_eq!(
            appex_path(&exe).unwrap(),
            tmp.path()
                .join("Forge.app/Contents/PlugIns/ForgeQuickLook.appex")
        );
    }
}
