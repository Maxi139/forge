//! Kompilieren mit Fortschritt, Überspringen unveränderter Builds und Auswerten der Ergebnisse.

use crate::check::{project_cache, CheckRequest, Checker};
use crate::cli::ArduinoCli;
use crate::fbuild;
use crate::hash::{input_hash, short_key, HashInput};
use crate::process::{os, stream};
use crate::{CancellationToken, Error, Result};
use forge_core::{utf16_col, Diagnostic, DiagnosticSource, Severity};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

/// Die sechs Stufen der Anzeige. Kompilieren und Linken gehören zum Build, der Rest zum Flashen.
pub const STAGES: [&str; 6] = [
    "Kompilieren",
    "Linken",
    "Verbinden",
    "Löschen",
    "Schreiben",
    "Prüfen",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum BuildEvent {
    /// `stage` ist ein Index in `STAGES`, `progress` liegt zwischen 0 und 1.
    Phase {
        stage: usize,
        progress: f32,
        detail: String,
    },
    /// Eine Zeile für das Ausgabe-Panel. `level` ist `info`, `cmd`, `warn` oder `err`.
    Output { line: String, level: String },
}

#[derive(Debug, Clone)]
pub struct BuildRequest {
    pub root: PathBuf,
    /// Board samt Menüoptionen.
    pub fqbn: String,
    pub libraries: Vec<(String, String)>,
    pub core_version: String,
    pub cli_version: String,
    /// Auch bauen, wenn sich nichts geändert hat.
    pub force: bool,
    /// Basisordner für Zwischenstände (nicht der Projektordner).
    pub cache_base: PathBuf,
    /// fbuild, wenn es installiert und erlaubt ist. Fehlt es oder passt das Board nicht, baut arduino-cli.
    pub fbuild: Option<PathBuf>,
    /// Installationsordner der Bibliotheken des Nutzers, für fbuild.
    pub library_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SizeInfo {
    pub label: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartitionInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub subtype: String,
    pub offset: u64,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildStatus {
    Built,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildOutcome {
    pub status: BuildStatus,
    pub hash: String,
    pub duration_ms: u64,
    pub sizes: Vec<SizeInfo>,
    pub partitions: Vec<PartitionInfo>,
    pub build_dir: PathBuf,
    pub diagnostics: Vec<Diagnostic>,
    /// Fehler ohne Quelltextstelle, z. B. vom Linker.
    pub error: Option<String>,
    /// Womit gebaut wurde: `arduino-cli` oder `fbuild`.
    pub backend: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Meta {
    hash: String,
    sizes: Vec<SizeInfo>,
    partitions: Vec<PartitionInfo>,
    compile_steps: u32,
    #[serde(default)]
    backend: String,
}

pub fn build_dir(cache_base: &Path, root: &Path, fqbn: &str) -> PathBuf {
    project_cache(cache_base, root)
        .join("build")
        .join(short_key(&[fqbn]))
}

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("gültiges Muster"))
}

pub(crate) fn level_of(line: &str) -> &'static str {
    if line.contains("fatal error")
        || line.contains(": error:")
        || line.starts_with("Error")
        || line.contains("undefined reference")
        || line.contains("ld returned")
    {
        "err"
    } else if line.contains(": warning:") {
        "warn"
    } else {
        "info"
    }
}

/// Kürzt Werkzeugaufrufe auf das Wesentliche, damit das Ausgabe-Panel lesbar bleibt.
/// Interne Hilfsaufrufe (`env`, `cp`, `size` ...) erscheinen gar nicht.
fn shorten(line: &str) -> Option<(String, &'static str)> {
    let line = forge_core::strip_ansi(line);
    let line = line.trim_end();
    if line.is_empty() {
        return None;
    }
    let first = line.split_whitespace().next().unwrap_or("");
    let is_tool_call =
        first.starts_with('/') && line.len() > 160 || first.starts_with("/usr/bin/env");
    if !is_tool_call {
        return Some((line.to_string(), level_of(line)));
    }
    let tool = Path::new(first)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let toks: Vec<&str> = line.split_whitespace().collect();
    let name = |p: &str| {
        Path::new(p)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string())
    };
    let out = toks
        .iter()
        .position(|t| *t == "-o")
        .and_then(|i| toks.get(i + 1))
        .copied();
    let src = toks
        .iter()
        .position(|t| *t == "-o")
        .and_then(|i| i.checked_sub(1))
        .map(|i| toks[i]);
    if tool.ends_with("g++") || tool.ends_with("gcc") {
        if line.contains(" -c ") {
            return src.map(|s| (format!("Kompiliere {}", name(s)), "cmd"));
        }
        return out.map(|o| (format!("Linke {}", name(o)), "cmd"));
    }
    if line.contains("elf2image") {
        return Some(("Erzeuge Firmware-Image".into(), "cmd"));
    }
    None
}

/// Ziel eines Compiler- oder Linkeraufrufs (`-o <Datei>`), nur für echte Compiler-Zeilen.
fn tool_output(line: &str) -> Option<(&str, &str)> {
    let mut toks = line.split_whitespace();
    let tool = toks.next()?;
    if !(tool.ends_with("g++") || tool.ends_with("gcc")) {
        return None;
    }
    let rest: Vec<&str> = toks.collect();
    let out = rest
        .iter()
        .position(|t| *t == "-o")
        .and_then(|i| rest.get(i + 1))?;
    Some((tool, out))
}

pub fn parse_sizes(lines: &[String]) -> Vec<SizeInfo> {
    static PROG: OnceLock<Regex> = OnceLock::new();
    static DATA: OnceLock<Regex> = OnceLock::new();
    let prog = re(&PROG, r"Sketch uses (\d+) bytes.*?Maximum is (\d+) bytes");
    let data = re(
        &DATA,
        r"Global variables use (\d+) bytes.*?Maximum is (\d+) bytes",
    );
    let mut out = Vec::new();
    for l in lines {
        if let Some(c) = prog.captures(l) {
            out.push(SizeInfo {
                label: "Flash".into(),
                used: c[1].parse().unwrap_or(0),
                total: c[2].parse().unwrap_or(0),
            });
        } else if let Some(c) = data.captures(l) {
            out.push(SizeInfo {
                label: "RAM".into(),
                used: c[1].parse().unwrap_or(0),
                total: c[2].parse().unwrap_or(0),
            });
        }
    }
    out
}

fn parse_num(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).ok();
    }
    let (num, mult) = match s.chars().last()? {
        'K' | 'k' => (&s[..s.len() - 1], 1024),
        'M' | 'm' => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    num.trim().parse::<u64>().ok().and_then(|n| n.checked_mul(mult))
}

/// Liest `partitions.csv` aus dem Build-Ordner. Fehlende Offsets ergeben sich aus der Reihenfolge.
pub fn parse_partitions(csv: &str) -> Vec<PartitionInfo> {
    let mut out = Vec::new();
    let mut next = 0x9000u64;
    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() < 5 {
            continue;
        }
        let size = parse_num(f[4]).unwrap_or(0);
        let offset = parse_num(f[3]).unwrap_or(next);
        next = offset.saturating_add(size);
        out.push(PartitionInfo {
            name: f[0].to_string(),
            kind: f[1].to_string(),
            subtype: f[2].to_string(),
            offset,
            size,
        });
    }
    out
}

/// Regeln von Arduino: Der Ordner muss wie die Hauptdatei heißen. Sonst wird ein Spiegel angelegt.
async fn sketch_dir(root: &Path, cache: &Path) -> Result<PathBuf> {
    let dir_name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if root.join(format!("{dir_name}.ino")).is_file() {
        return Ok(root.to_path_buf());
    }
    let mut inos: Vec<PathBuf> = std::fs::read_dir(root)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ino")))
        .collect();
    inos.sort();
    let main = inos
        .first()
        .ok_or_else(|| Error::Failed("Im Ordner liegt keine .ino-Datei.".into()))?;
    let stem = main
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mirror = cache.join("mirror").join(&stem);
    mirror_dir(root, &mirror)?;
    Ok(mirror)
}

/// Spiegelt Dateien und Unterordner (etwa `src/`) und kopiert nur, was sich geändert hat.
/// Versteckte Einträge und `build` bleiben außen vor. Im Spiegel verwaiste Einträge (umbenannte oder
/// gelöschte Quellen) werden entfernt, sonst würden sie weiter mitkompiliert. Der Spiegelordner
/// gehört allein dieser Funktion, Build-Ausgaben liegen woanders.
fn mirror_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    let mut keep: std::collections::HashSet<std::ffi::OsString> = Default::default();
    for entry in std::fs::read_dir(from)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if name.starts_with('.') || name == "build" {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        // Symlinks auf Ordner nicht verfolgen (Endlosschleife); Symlinks auf Dateien werden kopiert.
        let is_dir = if ft.is_symlink() {
            if path.is_dir() {
                continue;
            }
            false
        } else {
            ft.is_dir()
        };
        let target = to.join(&name);
        keep.insert(entry.file_name());
        // Typwechsel (Datei wurde Ordner oder umgekehrt): altes Ziel erst entfernen.
        if let Ok(tm) = std::fs::symlink_metadata(&target) {
            if tm.is_dir() != is_dir || tm.file_type().is_symlink() {
                remove_entry(&target);
            }
        }
        if is_dir {
            mirror_dir(&path, &target)?;
            continue;
        }
        let same = match (std::fs::metadata(&path), std::fs::metadata(&target)) {
            (Ok(a), Ok(b)) if a.len() == b.len() => {
                matches!((std::fs::read(&path), std::fs::read(&target)), (Ok(x), Ok(y)) if x == y)
            }
            _ => false,
        };
        if !same {
            std::fs::copy(&path, &target)?;
        }
    }
    for entry in std::fs::read_dir(to)?.flatten() {
        if !keep.contains(&entry.file_name()) {
            remove_entry(&entry.path());
        }
    }
    Ok(())
}

fn remove_entry(p: &Path) {
    let is_dir = std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir());
    let _ = if is_dir {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
}

fn text_diagnostics(lines: &[String], root: &Path, mirror: &Path) -> Vec<Diagnostic> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = re(
        &RE,
        r"^(.+?):(\d+):(\d+): (fatal error|error|warning): (.*)$",
    );
    let mut out = Vec::new();
    // Jede Datei nur einmal lesen, auch bei vielen Meldungen.
    let mut cache: HashMap<String, String> = HashMap::new();
    for l in lines {
        let Some(c) = re.captures(l) else { continue };
        let file = c[1].replace(
            mirror.to_string_lossy().as_ref(),
            root.to_string_lossy().as_ref(),
        );
        if !Path::new(&file).starts_with(root) {
            continue;
        }
        let line_no: u32 = c[2].parse().unwrap_or(1);
        let col: usize = c[3].parse().unwrap_or(1);
        let content = cache
            .entry(file.clone())
            .or_insert_with(|| std::fs::read_to_string(&file).unwrap_or_default());
        let text = content
            .split('\n')
            .nth((line_no as usize).saturating_sub(1))
            .map(str::to_string)
            .unwrap_or_default();
        let from = utf16_col(&text, col.saturating_sub(1));
        out.push(Diagnostic {
            id: format!("compiler:{file}:{line_no}:{from}"),
            file,
            line: line_no,
            from,
            to: from + 1,
            severity: if &c[4] == "warning" {
                Severity::Warning
            } else {
                Severity::Error
            },
            message: crate::translate::translate(&c[5]),
            source: DiagnosticSource::Compiler,
            fix: None,
            action: None,
        });
    }
    out
}

fn linker_error(lines: &[String]) -> Option<String> {
    if lines
        .iter()
        .any(|l| l.contains("Sketch too big") || l.contains("exceeds available space"))
    {
        let flash = parse_sizes(lines).into_iter().find(|s| s.label == "Flash");
        let detail = flash
            .filter(|s| s.total > 0)
            .map(|s| {
                format!(
                    " ({} von {} Bytes, {} %)",
                    s.used,
                    s.total,
                    s.used * 100 / s.total
                )
            })
            .unwrap_or_default();
        return Some(format!(
            "Das Programm ist zu groß für den Flash-Speicher{detail}. Wähle in den Board-Optionen ein größeres Partitionsschema, z. B. „Huge APP“."
        ));
    }
    let l = lines.iter().find(|l| {
        l.contains("undefined reference to")
            || l.contains("multiple definition of")
            || l.contains("region `") && l.contains("overflowed")
    })?;
    let l = l.trim();
    if let Some(i) = l.find("undefined reference to") {
        let sym = l[i + "undefined reference to".len()..]
            .trim()
            .trim_matches(|c| c == '`' || c == '\'');
        return Some(format!("Linker: „{sym}“ ist nirgends definiert."));
    }
    if l.contains("overflowed") {
        return Some(
            "Das Programm ist zu groß für den Flash-Speicher. Wähle ein größeres Partitionsschema."
                .into(),
        );
    }
    Some(l.to_string())
}

pub async fn build(
    cli: &ArduinoCli,
    checker: &Checker,
    req: &BuildRequest,
    cancel: &CancellationToken,
    mut emit: impl FnMut(BuildEvent) + Send,
) -> Result<BuildOutcome> {
    let started = Instant::now();
    let cache = project_cache(&req.cache_base, &req.root);
    let dir = build_dir(&req.cache_base, &req.root, &req.fqbn);
    tokio::fs::create_dir_all(&dir).await?;
    let meta_path = dir.join("forge-build.json");

    let hash = {
        let (root, fqbn, libs, core, cliv) = (
            req.root.clone(),
            req.fqbn.clone(),
            req.libraries.clone(),
            req.core_version.clone(),
            req.cli_version.clone(),
        );
        tokio::task::spawn_blocking(move || {
            input_hash(&HashInput {
                root: &root,
                fqbn: &fqbn,
                cli_version: &cliv,
                libraries: &libs,
                core_version: &core,
            })
        })
        .await
        .map_err(|e| Error::Failed(e.to_string()))?
    };

    let meta: Option<Meta> = std::fs::read(&meta_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let has_artifacts = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .any(|e| e.file_name().to_string_lossy().ends_with(".bin"))
        })
        .unwrap_or(false);
    if let (Some(m), true, false) = (&meta, has_artifacts, req.force) {
        if m.hash == hash {
            emit(BuildEvent::Output {
                line: "Unverändert. Der letzte Build wird verwendet.".into(),
                level: "info".into(),
            });
            emit(BuildEvent::Phase {
                stage: 1,
                progress: 1.0,
                detail: "Unverändert".into(),
            });
            return Ok(BuildOutcome {
                status: BuildStatus::Skipped,
                hash,
                duration_ms: started.elapsed().as_millis() as u64,
                sizes: m.sizes.clone(),
                partitions: m.partitions.clone(),
                build_dir: dir,
                diagnostics: vec![],
                error: None,
                backend: if m.backend.is_empty() {
                    "arduino-cli".into()
                } else {
                    m.backend.clone()
                },
            });
        }
    }

    // Erst prüfen: Fehler im Quelltext kosten so Sekunden statt eines halben Builds.
    emit(BuildEvent::Phase {
        stage: 0,
        progress: 0.0,
        detail: "Prüfe Quelltext".into(),
    });
    let check = checker
        .check(
            &CheckRequest {
                root: req.root.clone(),
                fqbn: req.fqbn.clone(),
                overrides: HashMap::new(),
                libraries_signature: libs_signature(&req.libraries),
            },
            cancel,
        )
        .await;
    if let Ok(c) = &check {
        if c.ran && c.diagnostics.iter().any(|d| d.severity == Severity::Error) {
            let errors = c
                .diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count();
            emit(BuildEvent::Output {
                line: format!("{errors} Fehler im Quelltext. Der Build wurde nicht gestartet."),
                level: "err".into(),
            });
            return Ok(BuildOutcome {
                status: BuildStatus::Failed,
                hash,
                duration_ms: started.elapsed().as_millis() as u64,
                sizes: vec![],
                partitions: vec![],
                build_dir: dir,
                diagnostics: c.diagnostics.clone(),
                error: None,
                backend: "arduino-cli".into(),
            });
        }
    }

    // fbuild, wenn es das Board kann. Jeder Fehler dort führt zu arduino-cli, das die genauen Meldungen liefert.
    if let (Some(fb), Some(board)) = (&req.fbuild, fbuild::board_for(&req.fqbn)) {
        match fbuild::sketch_supported(&req.root)
            .and_then(|()| fbuild::libraries_supported(&req.library_dirs))
        {
            Err(why) => emit(BuildEvent::Output {
                line: format!("fbuild übersprungen ({why}). Weiter mit arduino-cli."),
                level: "info".into(),
            }),
            Ok(()) => {
                // Ab hier können halbe Binärdateien entstehen: alte Meta nicht mehr als "unverändert" gelten lassen.
                let _ = std::fs::remove_file(&meta_path);
                let result = fbuild::compile(
                    &fbuild::FbuildRequest {
                        fbuild: fb.clone(),
                        root: req.root.clone(),
                        work: cache.join("fbuild"),
                        out: dir.clone(),
                        board,
                        library_dirs: req.library_dirs.clone(),
                    },
                    cancel,
                    &mut emit,
                )
                .await;
                match result {
                    Ok(out) => {
                        let meta = Meta {
                            hash: hash.clone(),
                            sizes: out.sizes.clone(),
                            partitions: out.partitions.clone(),
                            compile_steps: 0,
                            backend: "fbuild".into(),
                        };
                        let _ = std::fs::write(
                            &meta_path,
                            serde_json::to_vec(&meta).unwrap_or_default(),
                        );
                        return Ok(BuildOutcome {
                            status: BuildStatus::Built,
                            hash,
                            duration_ms: started.elapsed().as_millis() as u64,
                            sizes: out.sizes,
                            partitions: out.partitions,
                            build_dir: dir,
                            diagnostics: vec![],
                            error: None,
                            backend: "fbuild".into(),
                        });
                    }
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(e) => emit(BuildEvent::Output {
                        line: format!("fbuild hat nicht gebaut ({e}). Weiter mit arduino-cli."),
                        level: "warn".into(),
                    }),
                }
            }
        }
    }

    // Kompilieren. Meta erst nach Erfolg neu schreiben, ein Abbruch darf nicht "unverändert" melden.
    let _ = std::fs::remove_file(&meta_path);
    let sketch = sketch_dir(&req.root, &cache).await?;
    let total_steps = meta
        .as_ref()
        .map(|m| m.compile_steps)
        .filter(|n| *n > 0)
        .unwrap_or(70) as f32;
    let args = os(&[
        "compile",
        "-b",
        &req.fqbn,
        "--build-path",
        &dir.to_string_lossy(),
        "-v",
        &sketch.to_string_lossy(),
    ]);
    let mut lines: Vec<String> = Vec::new();
    let mut steps = 0u32;
    let mut linking = false;
    let code = stream(&cli.path, &args, None, "C", cancel, |_, l| {
        lines.push(forge_core::strip_ansi(l));
        let target = tool_output(l).map(|(_, o)| o);
        let is_compile = target.is_some_and(|o| o.ends_with(".o")) && l.contains(" -c ");
        if is_compile {
            steps += 1;
            if !linking {
                emit(BuildEvent::Phase {
                    stage: 0,
                    progress: (steps as f32 / total_steps).min(0.98),
                    detail: "Kompiliere".into(),
                });
            }
        }
        if !linking && target.is_some_and(|o| o.ends_with(".elf")) {
            linking = true;
            emit(BuildEvent::Phase {
                stage: 0,
                progress: 1.0,
                detail: "Kompiliere".into(),
            });
            emit(BuildEvent::Phase {
                stage: 1,
                progress: 0.3,
                detail: "Linke".into(),
            });
        }
        if linking
            && (l.contains("elf2image") || l.contains("merge-bin") || l.contains("merge_bin"))
        {
            emit(BuildEvent::Phase {
                stage: 1,
                progress: 0.8,
                detail: "Erzeuge Image".into(),
            });
        }
        if l.starts_with("Sketch uses") {
            emit(BuildEvent::Phase {
                stage: 1,
                progress: 1.0,
                detail: "Fertig".into(),
            });
        }
        if let Some((line, level)) = shorten(l) {
            emit(BuildEvent::Output {
                line,
                level: level.into(),
            });
        }
    })
    .await?;

    if code != Some(0) {
        let mut diagnostics = match &check {
            Ok(c) if c.ran => c.diagnostics.clone(),
            _ => Vec::new(),
        };
        if !diagnostics.iter().any(|d| d.severity == Severity::Error) {
            diagnostics = text_diagnostics(&lines, &req.root, &sketch);
        }
        let error = if diagnostics.iter().any(|d| d.severity == Severity::Error) {
            None
        } else {
            linker_error(&lines)
                .or_else(|| lines.iter().rev().find(|l| !l.trim().is_empty()).cloned())
        };
        return Ok(BuildOutcome {
            status: BuildStatus::Failed,
            hash,
            duration_ms: started.elapsed().as_millis() as u64,
            sizes: vec![],
            partitions: vec![],
            build_dir: dir,
            diagnostics,
            error,
            backend: "arduino-cli".into(),
        });
    }

    let sizes = parse_sizes(&lines);
    let partitions = std::fs::read_to_string(dir.join("partitions.csv"))
        .map(|t| parse_partitions(&t))
        .unwrap_or_default();
    let meta = Meta {
        hash: hash.clone(),
        sizes: sizes.clone(),
        partitions: partitions.clone(),
        compile_steps: steps,
        backend: "arduino-cli".into(),
    };
    let _ = std::fs::write(&meta_path, serde_json::to_vec(&meta).unwrap_or_default());

    Ok(BuildOutcome {
        status: BuildStatus::Built,
        hash,
        duration_ms: started.elapsed().as_millis() as u64,
        sizes,
        partitions,
        build_dir: dir,
        diagnostics: vec![],
        error: None,
        backend: "arduino-cli".into(),
    })
}

pub fn libs_signature(libs: &[(String, String)]) -> String {
    let mut v: Vec<String> = libs.iter().map(|(n, ver)| format!("{n}@{ver}")).collect();
    v.sort();
    v.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_come_from_arduino_cli_text() {
        let lines = vec![
            "Sketch uses 288462 bytes (22%) of program storage space. Maximum is 1310720 bytes.".to_string(),
            "Global variables use 22128 bytes (6%) of dynamic memory, leaving 305552 bytes for local variables. Maximum is 327680 bytes.".to_string(),
        ];
        let s = parse_sizes(&lines);
        assert_eq!(
            s[0],
            SizeInfo {
                label: "Flash".into(),
                used: 288462,
                total: 1310720
            }
        );
        assert_eq!(
            s[1],
            SizeInfo {
                label: "RAM".into(),
                used: 22128,
                total: 327680
            }
        );
    }

    #[test]
    fn partitions_parse_hex_decimal_and_units() {
        let csv = "# Name, Type, SubType, Offset, Size, Flags\nnvs,data,nvs,0x9000,0x5000,\notadata,data,ota,0xe000,0x2000,\napp0,app,ota_0,0x10000,1280K,\nspiffs,data,spiffs,,0x300000,\n";
        let p = parse_partitions(csv);
        assert_eq!(p.len(), 4);
        assert_eq!(
            (p[0].name.as_str(), p[0].offset, p[0].size),
            ("nvs", 0x9000, 0x5000)
        );
        assert_eq!(p[2].size, 1280 * 1024);
        assert_eq!(p[3].offset, 0x10000 + 1280 * 1024);
    }

    #[test]
    fn only_real_compiler_lines_count() {
        let cc = "/x/bin/xtensa-esp32s3-elf-g++ -MMD -c @/f/cpp_flags /s/a.cpp -o /b/a.cpp.o";
        assert_eq!(tool_output(cc).unwrap().1, "/b/a.cpp.o");
        let link = "/x/bin/xtensa-esp32s3-elf-g++ -Wl,--Map=x @/f/ld_libs -o /b/blink.ino.elf";
        assert_eq!(tool_output(link).unwrap().1, "/b/blink.ino.elf");
        // Die erste Zeile eines Builds ist ein bash-Aufruf, der .elf und -o erwähnt.
        assert!(tool_output("/usr/bin/env bash -c \"cp -o x bootloader_qio_80m.elf\"").is_none());
    }

    #[test]
    fn long_tool_calls_are_shortened() {
        let cmd = format!("/Users/x/xtensa-esp32s3-elf-g++ -MMD -c @/a/b/cpp_flags {} /core/esp32/main.cpp -o /build/core/main.cpp.o", "-DFOO=1 ".repeat(30));
        let (line, level) = shorten(&cmd).unwrap();
        assert_eq!((line.as_str(), level), ("Kompiliere main.cpp", "cmd"));
        assert_eq!(
            shorten("Sketch uses 1 bytes").unwrap().0,
            "Sketch uses 1 bytes"
        );
        // Farben verschwinden, Hilfsaufrufe ebenfalls.
        assert_eq!(
            shorten("\u{1b}[92mUsed library\u{1b}[0m  Version")
                .unwrap()
                .0,
            "Used library  Version"
        );
        assert!(shorten("/usr/bin/env bash -c \"cp -f a b\"").is_none());
        assert!(shorten(&format!("/x/xtensa-esp32s3-elf-size {}", "-A ".repeat(80))).is_none());
    }

    #[test]
    fn text_errors_map_back_to_the_project() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("a.ino");
        std::fs::write(&f, "void loop() { delay(1) }\n").unwrap();
        let line = format!(
            "{}:1:22: error: expected ';' before '}}' token",
            f.display()
        );
        let d = text_diagnostics(&[line], tmp.path(), Path::new("/nope"));
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].line, d[0].from), (1, 21));
        assert!(d[0].message.starts_with("Semikolon erwartet"));
    }

    #[test]
    fn line_zero_does_not_underflow() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("a.ino");
        std::fs::write(&f, "int x;\n").unwrap();
        let line = format!("{}:0:1: error: expected ';'", f.display());
        assert_eq!(
            text_diagnostics(&[line], tmp.path(), Path::new("/nope")).len(),
            1
        );
    }

    #[test]
    fn mirror_copies_subfolders_but_not_hidden_or_build() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("a"), tmp.path().join("b"));
        std::fs::create_dir_all(from.join("src/inner")).unwrap();
        std::fs::create_dir_all(from.join("build")).unwrap();
        std::fs::create_dir_all(from.join(".git")).unwrap();
        std::fs::write(from.join("a.ino"), "x").unwrap();
        std::fs::write(from.join("src/util.cpp"), "y").unwrap();
        std::fs::write(from.join("src/inner/deep.h"), "z").unwrap();
        std::fs::write(from.join("build/o.bin"), "no").unwrap();
        std::fs::write(from.join(".git/HEAD"), "no").unwrap();
        mirror_dir(&from, &to).unwrap();
        assert!(to.join("a.ino").is_file());
        assert_eq!(
            std::fs::read_to_string(to.join("src/inner/deep.h")).unwrap(),
            "z"
        );
        assert!(!to.join("build").exists() && !to.join(".git").exists());
    }

    #[test]
    fn mirror_prunes_removed_sources() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, to) = (tmp.path().join("a"), tmp.path().join("b"));
        std::fs::create_dir_all(from.join("src")).unwrap();
        std::fs::write(from.join("a.ino"), "x").unwrap();
        std::fs::write(from.join("alt.cpp"), "y").unwrap();
        std::fs::write(from.join("src/m.cpp"), "z").unwrap();
        mirror_dir(&from, &to).unwrap();
        std::fs::remove_file(from.join("alt.cpp")).unwrap();
        std::fs::remove_dir_all(from.join("src")).unwrap();
        std::fs::write(from.join("neu.cpp"), "n").unwrap();
        mirror_dir(&from, &to).unwrap();
        assert!(to.join("a.ino").is_file() && to.join("neu.cpp").is_file());
        assert!(!to.join("alt.cpp").exists() && !to.join("src").exists());
    }

    #[test]
    fn linker_messages_are_readable() {
        let l = vec!["/x/ld: sketch.o:(.text+0x10): undefined reference to `foo(int)'".to_string()];
        assert_eq!(
            linker_error(&l).unwrap(),
            "Linker: „foo(int)“ ist nirgends definiert."
        );
    }

    #[test]
    fn oversized_sketch_points_to_the_partition_scheme() {
        let l = vec![
            "Sketch too big; see https://support.arduino.cc/hc/en-us/articles/360013825179 for tips on reducing it.".to_string(),
            "Sketch uses 1416964 bytes (108%) of program storage space. Maximum is 1310720 bytes.".to_string(),
            "Error during build: text section exceeds available space in board".to_string(),
        ];
        let e = linker_error(&l).unwrap();
        assert!(e.contains("1416964 von 1310720 Bytes, 108 %"), "{e}");
        assert!(e.contains("Huge APP"));
    }
}

#[cfg(test)]
mod integration {
    //! Echter Build gegen arduino-cli und den ESP32-Core. Dauert etwa eine halbe Minute:
    //! `cargo test -p forge-toolchain -- --ignored build_flow`
    use super::*;
    use crate::tools::Tools;

    #[tokio::test]
    #[ignore = "dauert lange, braucht den ESP32-Core"]
    async fn build_flow_skips_unchanged_and_blocks_on_errors() {
        let Ok(tools) = Tools::locate(&[]) else {
            return;
        };
        let cli = ArduinoCli::new(tools.arduino_cli);
        let fqbn = "esp32:esp32:esp32s3";
        if cli.board_options(fqbn).await.is_err() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("flow");
        std::fs::create_dir_all(&root).unwrap();
        let ino = root.join("flow.ino");
        let write = |s: &str| std::fs::write(&ino, s).unwrap();
        write("void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(100); }\n");

        let checker = Checker::new(cli.clone(), tmp.path().join("cache"));
        let cancel = CancellationToken::new();
        let req = BuildRequest {
            root: root.clone(),
            fqbn: fqbn.into(),
            libraries: vec![],
            core_version: "x".into(),
            cli_version: cli.version().await.unwrap(),
            force: false,
            cache_base: tmp.path().join("cache"),
            fbuild: None,
            library_dirs: vec![],
        };
        let events = std::sync::Mutex::new(Vec::new());
        let run = |label: &'static str| {
            let events = &events;
            let (cli, checker, req, cancel) = (&cli, &checker, &req, &cancel);
            async move {
                let t = Instant::now();
                let r = build(cli, checker, req, cancel, |e| {
                    events.lock().unwrap().push(e)
                })
                .await
                .unwrap();
                eprintln!("{label}: {:?} in {:?}", r.status, t.elapsed());
                r
            }
        };

        let first = run("erster Build").await;
        assert_eq!(
            first.status,
            BuildStatus::Built,
            "{:?} {:?}",
            first.error,
            first.diagnostics
        );
        assert!(
            first
                .sizes
                .iter()
                .any(|s| s.label == "Flash" && s.used > 100_000),
            "{:?}",
            first.sizes
        );
        assert!(
            first.partitions.iter().any(|p| p.name == "app0"),
            "{:?}",
            first.partitions
        );
        let seen: Vec<String> = events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| {
                if let BuildEvent::Phase {
                    stage,
                    progress,
                    detail,
                } = e
                {
                    Some(format!("{stage}:{progress:.2}:{detail}"))
                } else {
                    None
                }
            })
            .collect();
        assert!(
            seen.iter()
                .any(|s| s.starts_with("0:0.") && !s.starts_with("0:0.00")),
            "kein Zwischenstand beim Kompilieren: {seen:?}"
        );

        assert_eq!(run("unverändert").await.status, BuildStatus::Skipped);

        write("// nur ein Kommentar\nvoid setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH);   delay(100); }\n");
        assert_eq!(run("nur Kommentar").await.status, BuildStatus::Skipped);

        write("void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH); delay(200); }\n");
        assert_eq!(run("Codeänderung").await.status, BuildStatus::Built);

        write("void setup() { pinMode(LED_BUILTIN, OUTPUT); }\nvoid loop() { digitalWrite(LED_BUILTIN, HIGH) delay(200); }\n");
        let bad = run("Syntaxfehler").await;
        assert_eq!(bad.status, BuildStatus::Failed);
        let d = bad
            .diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .expect("Fehlerdiagnose");
        assert_eq!(d.line, 2);
        assert!(d.fix.is_some());
        assert!(
            bad.duration_ms < 15_000,
            "der Build wurde nicht abgekürzt: {} ms",
            bad.duration_ms
        );
    }
}
