//! fbuild als schneller Compiler für Boards, die wir damit geprüft haben. Alles andere und jeder Fehler
//! fällt auf arduino-cli zurück. fbuild bekommt ein eigenes Projekt im Cache: `platformio.ini`, eine Kopie der
//! Quelltexte in `src/` und die Bibliotheken als Verweise in `lib/`. So erkennt es Änderungen selbst.

use crate::build::{level_of, BuildEvent, PartitionInfo, SizeInfo};
use crate::process::{capture, os, stream};
use crate::{hash, CancellationToken, Error, Result};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Ein Board, das fbuild bei uns bauen darf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FbuildBoard {
    pub env: &'static str,
    pub pio_board: &'static str,
    pub bootloader_offset: u64,
    /// Flash-Eigenschaften des Boards (`build.flash_mode`, `build.flash_freq`, `upload.flash_size`).
    pub flash_mode: &'static str,
    pub flash_freq: &'static str,
    pub flash_size: &'static str,
}

/// Nur Boards, die wir am echten Gerät geflasht haben. Weitere kommen dazu, sobald sie geprüft sind.
const VERIFIED: &[(&str, FbuildBoard)] = &[(
    "esp32:esp32:esp32",
    FbuildBoard {
        env: "esp32dev",
        pio_board: "esp32dev",
        bootloader_offset: 0x1000,
        flash_mode: "dio",
        flash_freq: "40m",
        flash_size: "4MB",
    },
)];

/// Das Board für einen FQBN. Menüoptionen (`:FlashSize=...`) kennt fbuild bei uns nicht.
pub fn board_for(fqbn: &str) -> Option<FbuildBoard> {
    VERIFIED.iter().find(|(f, _)| *f == fqbn).map(|(_, b)| *b)
}

/// Wo `fbuild` liegt: in den eigenen Ordnern der App, sonst im PATH.
pub fn locate(own_dirs: &[PathBuf]) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = own_dirs
        .iter()
        .flat_map(|d| [d.join("fbuild-venv/bin/fbuild"), d.join("fbuild")])
        .collect();
    for d in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/usr/bin",
        "~/.local/bin",
    ] {
        let d = if let Some(rest) = d.strip_prefix("~/") {
            match std::env::var_os("HOME") {
                Some(h) => PathBuf::from(h).join(rest),
                None => continue,
            }
        } else {
            PathBuf::from(d)
        };
        candidates.push(d.join("fbuild"));
    }
    candidates.into_iter().find(|p| is_exec(p))
}

fn is_exec(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

/// Legt fbuild in einem eigenen Python-Umfeld unter `<dir>/fbuild-venv` an.
pub async fn install(
    dir: &Path,
    cancel: &CancellationToken,
    mut emit: impl FnMut(&str) + Send,
) -> Result<PathBuf> {
    let venv = dir.join("fbuild-venv");
    tokio::fs::create_dir_all(dir).await?;
    let python = [
        "/opt/homebrew/bin/python3",
        "/usr/local/bin/python3",
        "/usr/bin/python3",
    ]
    .iter()
    .map(PathBuf::from)
    .find(|p| p.is_file())
    .ok_or_else(|| {
        Error::Failed("Python 3 fehlt. Installiere es mit „brew install python“.".into())
    })?;
    emit("Lege Umgebung an");
    let code = stream(
        &python,
        &os(&["-m", "venv", &venv.to_string_lossy()]),
        None,
        "C",
        cancel,
        |_, l| emit(l),
    )
    .await?;
    if code != Some(0) {
        return Err(Error::Failed(
            "Die Python-Umgebung für fbuild ließ sich nicht anlegen.".into(),
        ));
    }
    emit("Lade fbuild");
    let pip = venv.join("bin/pip");
    let code = stream(
        &pip,
        &os(&[
            "install",
            "--upgrade",
            "--disable-pip-version-check",
            "fbuild",
        ]),
        None,
        "C",
        cancel,
        |_, l| emit(l),
    )
    .await?;
    if code != Some(0) {
        return Err(Error::Failed(
            "fbuild ließ sich nicht installieren. Prüfe die Internetverbindung.".into(),
        ));
    }
    locate(&[dir.to_path_buf()])
        .ok_or_else(|| Error::Failed("fbuild wurde installiert, ist aber nicht auffindbar.".into()))
}

/// Lädt Toolchain und Framework vor, damit der erste Build nicht Minuten mit Herunterladen verbringt.
pub async fn prefetch(
    fbuild: &Path,
    work: &Path,
    board: &FbuildBoard,
    cancel: &CancellationToken,
    mut emit: impl FnMut(&str) + Send,
) -> Result<()> {
    tokio::fs::create_dir_all(work).await?;
    write_if_changed(
        &work.join("platformio.ini"),
        platformio_ini(board).as_bytes(),
    )?;
    let args = os(&["install", "-e", board.env, &work.to_string_lossy()]);
    let mut tail = Vec::new();
    let code = stream(fbuild, &args, Some(work), "C", cancel, |_, l| {
        let l = forge_core::strip_ansi(l);
        tail.push(l.clone());
        emit(&l);
    })
    .await?;
    if code != Some(0) {
        return Err(Error::Failed(format!(
            "fbuild konnte Toolchain und Framework nicht laden: {}",
            tail.last().cloned().unwrap_or_default()
        )));
    }
    Ok(())
}

pub async fn version(fbuild: &Path) -> Option<String> {
    let out = capture(
        fbuild,
        &os(&["--version"]),
        None,
        "C",
        Some(Duration::from_secs(15)),
    )
    .await
    .ok()?;
    out.success()
        .then(|| out.stdout.trim().replace("fbuild ", ""))
}

/// Dateien, mit denen wir fbuild nicht zuverlässig zum gleichen Ergebnis wie arduino-cli bringen.
const NEEDS_ARDUINO_CLI: &[&str] = &[
    "partitions.csv",
    "sdkconfig",
    "build_opt.h",
    "boards.local.txt",
];

/// `Ok`, wenn der Sketch einfach genug ist. Sonst der Grund, warum arduino-cli übernimmt.
pub fn sketch_supported(root: &Path) -> std::result::Result<(), String> {
    let files = hash::source_files(root);
    let inos = files
        .iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino")))
        .count();
    if inos != 1 {
        return Err("mehrere .ino-Dateien".into());
    }
    for f in &files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if NEEDS_ARDUINO_CLI.contains(&name.as_str()) {
            return Err(format!("{name} im Projekt"));
        }
    }
    Ok(())
}

/// Bibliotheken mit verschachteltem `src/src` (z. B. HomeSpan) übersetzt fbuild nur teilweise: die Dateien
/// in `src/` fehlen dann beim Linken. Diese Fälle übernimmt arduino-cli.
pub fn libraries_supported(dirs: &[PathBuf]) -> std::result::Result<(), String> {
    for d in dirs {
        if d.join("src/src").is_dir() {
            let name = d.file_name().unwrap_or_default().to_string_lossy();
            return Err(format!("Bibliothek {name} mit verschachteltem src/src"));
        }
    }
    Ok(())
}

fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<bool> {
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(false);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)?;
    Ok(true)
}

fn sync_sources(root: &Path, src: &Path) -> std::io::Result<()> {
    let mut keep: BTreeSet<PathBuf> = BTreeSet::new();
    for file in hash::source_files(root) {
        let Ok(rel) = file.strip_prefix(root) else {
            continue;
        };
        write_if_changed(&src.join(rel), &std::fs::read(&file)?)?;
        keep.insert(src.join(rel));
    }
    fn prune(dir: &Path, keep: &BTreeSet<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                prune(&p, keep);
            } else if !keep.contains(&p) {
                let _ = std::fs::remove_file(&p);
            }
        }
    }
    prune(src, &keep);
    Ok(())
}

fn sync_libraries(lib: &Path, dirs: &[PathBuf]) -> std::io::Result<()> {
    std::fs::create_dir_all(lib)?;
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for d in dirs {
        let Some(name) = d.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        wanted.insert(name.clone());
        let link = lib.join(&name);
        if std::fs::read_link(&link).ok().as_deref() == Some(d.as_path()) {
            continue;
        }
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(d, &link)?;
    }
    for e in std::fs::read_dir(lib)?.flatten() {
        if !wanted.contains(e.file_name().to_string_lossy().as_ref()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
    Ok(())
}

fn platformio_ini(board: &FbuildBoard) -> String {
    format!(
        "[env:{}]\nplatform = espressif32\nboard = {}\nframework = arduino\n",
        board.env, board.pio_board
    )
}

pub struct FbuildRequest {
    pub fbuild: PathBuf,
    pub root: PathBuf,
    /// Eigenes Projekt für fbuild im Cache.
    pub work: PathBuf,
    /// Ordner, aus dem später geflasht wird.
    pub out: PathBuf,
    pub board: FbuildBoard,
    pub library_dirs: Vec<PathBuf>,
}

pub struct FbuildOutput {
    pub sizes: Vec<SizeInfo>,
    pub partitions: Vec<PartitionInfo>,
    pub lines: Vec<String>,
}

/// Liest die Partitionstabelle aus `partitions.bin` (32 Byte je Eintrag, Kennung `AA 50`).
pub fn parse_partition_table(bin: &[u8]) -> Vec<PartitionInfo> {
    let mut out = Vec::new();
    for e in bin.as_chunks::<32>().0 {
        if e[0] != 0xAA || e[1] != 0x50 {
            break;
        }
        let kind = e[2];
        let sub = e[3];
        let offset = u32::from_le_bytes([e[4], e[5], e[6], e[7]]) as u64;
        let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as u64;
        let name = String::from_utf8_lossy(&e[12..28])
            .trim_end_matches('\0')
            .to_string();
        let (kind_name, subtype) = match (kind, sub) {
            (0, 0x00) => ("app", "factory".to_string()),
            (0, 0x20) => ("app", "test".to_string()),
            (0, s @ 0x10..=0x1f) => ("app", format!("ota_{}", s - 0x10)),
            (0, s) => ("app", format!("{s:#x}")),
            (1, 0x00) => ("data", "ota".into()),
            (1, 0x01) => ("data", "phy".into()),
            (1, 0x02) => ("data", "nvs".into()),
            (1, 0x03) => ("data", "coredump".into()),
            (1, 0x04) => ("data", "nvs_keys".into()),
            (1, 0x05) => ("data", "efuse".into()),
            (1, 0x81) => ("data", "fat".into()),
            (1, 0x82) => ("data", "spiffs".into()),
            (1, 0x83) => ("data", "littlefs".into()),
            (_, s) => ("data", format!("{s:#x}")),
        };
        out.push(PartitionInfo {
            name,
            kind: kind_name.to_string(),
            subtype,
            offset,
            size,
        });
    }
    out
}

fn flash_args(board: &FbuildBoard) -> String {
    format!(
        "--flash-mode {} --flash-freq {} --flash-size {}\n{:#x} bootloader.bin\n0x8000 partitions.bin\n0xe000 boot_app0.bin\n0x10000 firmware.bin\n",
        board.flash_mode, board.flash_freq, board.flash_size, board.bootloader_offset
    )
}

fn sizes_from_fingerprint(text: &str) -> Vec<SizeInfo> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return vec![];
    };
    let s = &v["size_info"];
    let get = |k: &str| s[k].as_u64().unwrap_or(0);
    if get("max_flash") == 0 {
        return vec![];
    }
    vec![
        SizeInfo {
            label: "Flash".into(),
            used: get("total_flash"),
            total: get("max_flash"),
        },
        SizeInfo {
            label: "RAM".into(),
            used: get("total_ram"),
            total: get("max_ram"),
        },
    ]
}

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).expect("gültiges Muster"))
}

pub async fn compile(
    req: &FbuildRequest,
    cancel: &CancellationToken,
    mut emit: impl FnMut(BuildEvent) + Send,
) -> Result<FbuildOutput> {
    let (root, work, libs) = (req.root.clone(), req.work.clone(), req.library_dirs.clone());
    let ini = platformio_ini(&req.board);
    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        std::fs::create_dir_all(&work)?;
        write_if_changed(&work.join("platformio.ini"), ini.as_bytes())?;
        sync_sources(&root, &work.join("src"))?;
        sync_libraries(&work.join("lib"), &libs)
    })
    .await
    .map_err(|e| Error::Failed(e.to_string()))??;

    let mut lines: Vec<String> = Vec::new();
    let args = os(&[
        "build",
        "-e",
        req.board.env,
        "--no-timestamp",
        &req.work.to_string_lossy(),
    ]);
    let mut linking = false;
    let code = stream(&req.fbuild, &args, Some(&req.work), "C", cancel, |_, l| {
        let line = forge_core::strip_ansi(l);
        let line = line.trim_end();
        if line.is_empty() || line.starts_with("====") || line.starts_with("=   ") {
            return;
        }
        lines.push(line.to_string());
        static STEP: OnceLock<Regex> = OnceLock::new();
        if let Some(c) = re(&STEP, r"^Compiled (\d+)/(\d+) files").captures(line) {
            let (n, m): (f32, f32) = (c[1].parse().unwrap_or(0.0), c[2].parse().unwrap_or(1.0));
            if !linking {
                emit(BuildEvent::Phase {
                    stage: 0,
                    progress: (n / m.max(1.0)).min(0.98),
                    detail: "Kompiliere".into(),
                });
            }
        } else if line.starts_with("Building firmware") && !linking {
            linking = true;
            emit(BuildEvent::Phase {
                stage: 0,
                progress: 1.0,
                detail: "Kompiliere".into(),
            });
            emit(BuildEvent::Phase {
                stage: 1,
                progress: 0.5,
                detail: "Linke".into(),
            });
        }
        emit(BuildEvent::Output {
            line: line.to_string(),
            level: level_of(line).into(),
        });
    })
    .await?;

    if code != Some(0) {
        let reason = lines
            .iter()
            .rev()
            .find(|l| l.contains("error") || l.contains("Error"))
            .or_else(|| lines.last())
            .cloned()
            .unwrap_or_else(|| "fbuild meldete einen Fehler.".into());
        return Err(Error::Failed(reason));
    }

    // Ergebnisse dorthin legen, wo der Flash-Schritt sie erwartet.
    let release = req
        .work
        .join(".fbuild/build")
        .join(req.board.env)
        .join("release");
    std::fs::create_dir_all(&req.out)?;
    for name in [
        "bootloader.bin",
        "partitions.bin",
        "boot_app0.bin",
        "firmware.bin",
    ] {
        let src = release.join(name);
        if !src.is_file() {
            return Err(Error::Failed(format!("fbuild hat {name} nicht erzeugt.")));
        }
        std::fs::copy(&src, req.out.join(name))?;
    }
    std::fs::write(req.out.join("flash_args"), flash_args(&req.board))?;
    let partitions = std::fs::read(req.out.join("partitions.bin"))
        .map(|b| parse_partition_table(&b))
        .unwrap_or_default();
    let sizes = std::fs::read_to_string(release.join("build_fingerprint.json"))
        .map(|t| sizes_from_fingerprint(&t))
        .unwrap_or_default();
    emit(BuildEvent::Phase {
        stage: 1,
        progress: 1.0,
        detail: "Fertig".into(),
    });
    Ok(FbuildOutput {
        sizes,
        partitions,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_verified_boards_without_options() {
        assert_eq!(board_for("esp32:esp32:esp32").unwrap().env, "esp32dev");
        assert!(board_for("esp32:esp32:esp32:FlashSize=16M").is_none());
        assert!(board_for("esp32:esp32:esp32s3").is_none());
    }

    #[test]
    fn flash_args_come_from_board_properties() {
        let mut b = board_for("esp32:esp32:esp32").unwrap();
        b.flash_freq = "80m";
        b.flash_size = "16MB";
        let a = flash_args(&b);
        assert!(a.starts_with(
            "--flash-mode dio --flash-freq 80m --flash-size 16MB\n0x1000 bootloader.bin"
        ));
    }

    #[test]
    fn reads_the_default_partition_table() {
        let bin = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/esp32-default-partitions.bin"
        ))
        .unwrap();
        let p = parse_partition_table(&bin);
        let names: Vec<&str> = p.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            ["nvs", "otadata", "app0", "app1", "spiffs", "coredump"]
        );
        assert_eq!(
            (p[0].offset, p[0].size, p[0].subtype.as_str()),
            (0x9000, 0x5000, "nvs")
        );
        assert_eq!(p[2].subtype, "ota_0");
        assert_eq!(p[2].kind, "app");
    }

    #[test]
    fn sizes_come_from_the_fingerprint() {
        let s = sizes_from_fingerprint(
            r#"{"size_info":{"text":1,"total_flash":277833,"total_ram":22249,"max_flash":1310720,"max_ram":327680}}"#,
        );
        assert_eq!(
            s[0],
            SizeInfo {
                label: "Flash".into(),
                used: 277833,
                total: 1310720
            }
        );
        assert_eq!(s[1].total, 327680);
        assert!(sizes_from_fingerprint("kaputt").is_empty());
    }

    #[test]
    fn sources_and_libraries_are_mirrored() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("proj");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("proj.ino"), "void setup(){}\nvoid loop(){}\n").unwrap();
        std::fs::write(root.join("sub/x.h"), "#pragma once\n").unwrap();
        let src = tmp.path().join("work/src");
        sync_sources(&root, &src).unwrap();
        assert!(src.join("proj.ino").is_file() && src.join("sub/x.h").is_file());
        // Gelöschte Dateien verschwinden auch in der Kopie.
        std::fs::remove_file(root.join("sub/x.h")).unwrap();
        sync_sources(&root, &src).unwrap();
        assert!(!src.join("sub/x.h").exists());

        let libdir = tmp.path().join("libs/Foo");
        std::fs::create_dir_all(&libdir).unwrap();
        let lib = tmp.path().join("work/lib");
        sync_libraries(&lib, std::slice::from_ref(&libdir)).unwrap();
        assert_eq!(std::fs::read_link(lib.join("Foo")).unwrap(), libdir);
        sync_libraries(&lib, &[]).unwrap();
        assert!(!lib.join("Foo").exists());
    }

    #[test]
    fn complex_sketches_stay_with_arduino_cli() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("a.ino"), "").unwrap();
        assert!(sketch_supported(tmp.path()).is_ok());
        std::fs::write(tmp.path().join("partitions.csv"), "").unwrap();
        assert!(sketch_supported(tmp.path())
            .unwrap_err()
            .contains("partitions.csv"));
        std::fs::remove_file(tmp.path().join("partitions.csv")).unwrap();
        std::fs::write(tmp.path().join("b.ino"), "").unwrap();
        assert!(sketch_supported(tmp.path()).is_err());
    }

    #[test]
    fn nested_src_libraries_stay_with_arduino_cli() {
        let tmp = tempfile::tempdir().unwrap();
        let flat = tmp.path().join("Flat");
        let nested = tmp.path().join("HomeSpan");
        std::fs::create_dir_all(flat.join("src")).unwrap();
        std::fs::create_dir_all(nested.join("src/src/extras")).unwrap();
        assert!(libraries_supported(std::slice::from_ref(&flat)).is_ok());
        assert!(libraries_supported(&[flat, nested])
            .unwrap_err()
            .contains("HomeSpan"));
    }
}
