//! Flashen mit esptool. Wir rufen esptool direkt auf (statt über arduino-cli und Python), lesen die
//! Regionen aus `flash_args` des Builds und können weitere Regionen anhängen, etwa die NVS-Partition
//! mit den Secrets.

use crate::build::BuildEvent;
use crate::process::{os, stream};
use crate::{CancellationToken, Error, Result};
use forge_core::esptool::EsptoolStyle;
use regex::Regex;
use serde::Serialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct FlashRequest {
    pub esptool: PathBuf,
    /// `esp32s3`, aus `build.mcu` des Boards.
    pub chip: String,
    pub port: String,
    pub baud: u32,
    pub build_dir: PathBuf,
    /// Weitere Regionen (Offset, Datei), z. B. die NVS-Partition.
    pub extra: Vec<(u64, PathBuf)>,
    /// Vor dem Schreiben den gesamten Flash löschen.
    pub erase_all: bool,
    /// Nur die zusätzlichen Regionen schreiben, die Firmware aus dem Build bleibt unberührt.
    pub skip_firmware: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashOutcome {
    pub duration_ms: u64,
    pub bytes: u64,
    pub chip: Option<String>,
    pub mac: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FlashArgs {
    /// Optionen wie `--flash-mode dio`.
    pub options: Vec<String>,
    pub regions: Vec<(String, String)>,
}

/// Liest `flash_args`: erste Zeile Optionen, danach `offset datei`.
pub fn parse_flash_args(text: &str) -> FlashArgs {
    let mut options = Vec::new();
    let mut regions = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with("--") {
            options.extend(line.split_whitespace().map(str::to_string));
        } else if let Some((off, file)) = line.split_once(char::is_whitespace) {
            regions.push((off.to_string(), file.trim().to_string()));
        }
    }
    FlashArgs { options, regions }
}

fn re(cell: &'static OnceLock<Regex>, p: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(p).expect("gültiges Muster"))
}

/// Übersetzt esptool-Fehler in einen Satz, der sagt, was jetzt hilft.
pub fn explain_failure(lines: &[String]) -> String {
    let all = lines.join("\n");
    let has = |s: &str| all.contains(s);
    if has("No such file or directory") && has("could not open port") {
        return "Der Port ist nicht mehr da. Prüfe das Kabel und suche die Ports neu.".into();
    }
    if has("Resource busy")
        || has("port is busy")
        || has("Permission denied")
        || has("Operation not permitted")
    {
        return "Der Port ist belegt. Schließe den Monitor und andere Programme, die ihn nutzen."
            .into();
    }
    if has("Failed to connect") || has("No serial data received") || has("Wrong boot mode") {
        return "Das Board antwortet nicht. Halte BOOT gedrückt, tippe RESET kurz an und flashe erneut.".into();
    }
    if has("Timed out waiting for packet header")
        || has("Write timeout")
        || has("chip stopped responding")
    {
        return "Die Verbindung brach ab. Probiere ein anderes Kabel oder stelle die Upload-Geschwindigkeit auf 115200.".into();
    }
    if has("Invalid head of packet") {
        return "Das Board sendet unlesbare Daten. Prüfe das Kabel und die Baudrate.".into();
    }
    static FATAL: OnceLock<Regex> = OnceLock::new();
    if let Some(c) = re(&FATAL, r"A fatal error occurred: (.+)").captures(&all) {
        return c[1].trim().to_string();
    }
    lines
        .iter()
        .rev()
        .find(|l| !l.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| "Flashen ist fehlgeschlagen.".into())
}

pub async fn flash(
    req: &FlashRequest,
    cancel: &CancellationToken,
    mut emit: impl FnMut(BuildEvent) + Send,
) -> Result<FlashOutcome> {
    let started = Instant::now();
    let text = tokio::fs::read_to_string(req.build_dir.join("flash_args"))
        .await
        .map_err(|_| {
            Error::Failed("Es gibt noch keinen Build zum Flashen. Kompiliere zuerst.".into())
        })?;
    let fa = parse_flash_args(&text);
    if fa.regions.is_empty() && !req.skip_firmware {
        return Err(Error::Failed(
            "Der Build enthält keine Dateien zum Flashen. Kompiliere neu.".into(),
        ));
    }

    let mut regions: Vec<(String, String)> = if req.skip_firmware {
        Vec::new()
    } else {
        fa.regions.clone()
    };
    for (off, file) in &req.extra {
        regions.push((format!("0x{off:x}"), file.to_string_lossy().into_owned()));
    }
    let total_regions = regions.len().max(1);

    // esptool 4 und 5 unterscheiden sich in der Schreibweise, daher einmal die Version erfragen.
    let tool = req.esptool.clone();
    let style = tokio::task::spawn_blocking(move || EsptoolStyle::detect(&tool))
        .await
        .unwrap_or_default();
    let mut base: Vec<OsString> = os(&[
        "--chip",
        &req.chip,
        "--port",
        &req.port,
        "--baud",
        &req.baud.to_string(),
        "--before",
        &style.word("default-reset"),
        "--after",
        &style.word("hard-reset"),
    ]);
    let mut lines: Vec<String> = Vec::new();
    let (mut chip, mut mac, mut bytes) = (None, None, 0u64);
    let (mut written, mut verified) = (0usize, 0usize);

    static WRITING: OnceLock<Regex> = OnceLock::new();
    static WROTE: OnceLock<Regex> = OnceLock::new();
    static CHIP: OnceLock<Regex> = OnceLock::new();
    static MAC: OnceLock<Regex> = OnceLock::new();
    let writing = re(&WRITING, r"Writing at 0x[0-9a-fA-F]+.*?(\d+(?:\.\d+)?)\s*%");
    let wrote = re(&WROTE, r"Wrote (\d+) bytes");
    let chip_re = re(&CHIP, r"Chip type:\s+(\S+)|Chip is (ESP32\S*)");
    let mac_re = re(&MAC, r"MAC:\s+([0-9a-fA-F:]{17})");

    emit(BuildEvent::Phase {
        stage: 2,
        progress: 0.0,
        detail: "Verbinde".into(),
    });

    // Optional: alles löschen. Das ist ein eigener Lauf, damit die Stufe „Löschen“ echt ist.
    if req.erase_all {
        let mut args = base.clone();
        args.push(style.word("erase-flash").into());
        let code = stream(
            &req.esptool,
            &args,
            Some(&req.build_dir),
            "en_US.UTF-8",
            cancel,
            |_, l| {
                lines.push(l.to_string());
                if let Some(c) = chip_re.captures(l) {
                    chip = c.get(1).or(c.get(2)).map(|m| m.as_str().to_string());
                    emit(BuildEvent::Phase {
                        stage: 2,
                        progress: 1.0,
                        detail: "Verbunden".into(),
                    });
                    emit(BuildEvent::Phase {
                        stage: 3,
                        progress: 0.4,
                        detail: "Lösche Flash".into(),
                    });
                }
                emit(BuildEvent::Output {
                    line: l.to_string(),
                    level: "info".into(),
                });
            },
        )
        .await?;
        if code != Some(0) {
            return Err(Error::Failed(explain_failure(&lines)));
        }
        emit(BuildEvent::Phase {
            stage: 3,
            progress: 1.0,
            detail: "Gelöscht".into(),
        });
        lines.clear();
    }

    if regions.is_empty() {
        return Err(Error::Failed("Es gibt nichts zu schreiben.".into()));
    }
    base.push(style.word("write-flash").into());
    base.push("-z".into());
    base.extend(fa.options.iter().map(|o| {
        if o.starts_with("--") {
            OsString::from(style.word(o))
        } else {
            OsString::from(o)
        }
    }));
    for (off, file) in &regions {
        base.push(off.into());
        base.push(file.into());
    }

    let mut connected = req.erase_all;
    let code = stream(
        &req.esptool,
        &base,
        Some(&req.build_dir),
        "en_US.UTF-8",
        cancel,
        |_, l| {
            lines.push(l.to_string());
            if l.contains("Connecting") && !connected {
                emit(BuildEvent::Phase {
                    stage: 2,
                    progress: 0.3,
                    detail: "Verbinde".into(),
                });
            }
            if let Some(c) = chip_re.captures(l) {
                chip = c.get(1).or(c.get(2)).map(|m| m.as_str().to_string());
                if !connected {
                    connected = true;
                    emit(BuildEvent::Phase {
                        stage: 2,
                        progress: 1.0,
                        detail: "Verbunden".into(),
                    });
                    if !req.erase_all {
                        emit(BuildEvent::Phase {
                            stage: 3,
                            progress: 1.0,
                            detail: "Kein Löschen nötig".into(),
                        });
                    }
                }
            }
            if let Some(c) = mac_re.captures(l) {
                mac = Some(c[1].to_lowercase());
            }
            if let Some(c) = writing.captures(l) {
                let pct: f32 = c[1].parse().unwrap_or(0.0);
                emit(BuildEvent::Phase {
                    stage: 4,
                    progress: ((written as f32 + pct / 100.0) / total_regions as f32).min(1.0),
                    detail: "Schreibe".into(),
                });
            }
            if let Some(c) = wrote.captures(l) {
                written += 1;
                bytes += c[1].parse::<u64>().unwrap_or(0);
                emit(BuildEvent::Phase {
                    stage: 4,
                    progress: (written as f32 / total_regions as f32).min(1.0),
                    detail: "Schreibe".into(),
                });
            }
            if l.contains("Hash of data verified") {
                verified += 1;
                emit(BuildEvent::Phase {
                    stage: 5,
                    progress: (verified as f32 / total_regions as f32).min(1.0),
                    detail: "Prüfe".into(),
                });
            }
            if l.contains("Hard resetting") {
                emit(BuildEvent::Phase {
                    stage: 5,
                    progress: 1.0,
                    detail: "Neustart".into(),
                });
            }
            emit(BuildEvent::Output {
                line: l.to_string(),
                level: if l.contains("rror") {
                    "err".into()
                } else {
                    "info".into()
                },
            });
        },
    )
    .await?;

    if code != Some(0) {
        return Err(Error::Failed(explain_failure(&lines)));
    }
    Ok(FlashOutcome {
        duration_ms: started.elapsed().as_millis() as u64,
        bytes,
        chip,
        mac,
    })
}

/// Ordner und Dateien, die zum Flashen gehören: nur zur Anzeige.
pub fn region_files(build_dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(build_dir.join("flash_args"))
        .map(|t| parse_flash_args(&t).regions)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_args_are_split_into_options_and_regions() {
        let fa = parse_flash_args("--flash-mode dio --flash-freq 80m --flash-size 4MB\n0x0 blink.ino.bootloader.bin\n0x8000 blink.ino.partitions.bin\n0xe000 boot_app0.bin\n0x10000 blink.ino.bin\n");
        assert_eq!(
            fa.options,
            [
                "--flash-mode",
                "dio",
                "--flash-freq",
                "80m",
                "--flash-size",
                "4MB"
            ]
        );
        assert_eq!(fa.regions.len(), 4);
        assert_eq!(
            fa.regions[3],
            ("0x10000".to_string(), "blink.ino.bin".to_string())
        );
    }

    fn l(s: &str) -> Vec<String> {
        s.lines().map(str::to_string).collect()
    }

    #[test]
    fn failures_explain_what_to_do() {
        assert!(explain_failure(&l(
            "A fatal error occurred: Failed to connect to ESP32-S3: No serial data received."
        ))
        .contains("BOOT gedrückt"));
        assert!(explain_failure(&l("A fatal error occurred: Could not open /dev/cu.x, the port is busy or doesn't exist.\n([Errno 16] could not open port /dev/cu.x: [Errno 16] Resource busy: '/dev/cu.x')")).contains("belegt"));
        assert!(explain_failure(&l("([Errno 2] could not open port /dev/cu.x: [Errno 2] No such file or directory: '/dev/cu.x')")).contains("nicht mehr da"));
        assert_eq!(
            explain_failure(&l("A fatal error occurred: Something odd")),
            "Something odd"
        );
    }

    /// Ein Skript, das esptool nachahmt, prüft Fortschritt und Auswertung ohne Hardware.
    #[tokio::test]
    async fn progress_is_derived_from_esptool_output() {
        run_fake(
            "esptool v5.3.1",
            "write-flash",
            "write_flash",
            "--flash_mode",
        )
        .await;
    }

    /// esptool 4 kennt nur die Schreibweise mit Unterstrichen, das Skript lehnt die andere ab.
    #[tokio::test]
    async fn esptool_v4_gets_underscore_spelling() {
        run_fake(
            "esptool.py v4.7.0",
            "write_flash",
            "write-flash",
            "--flash-mode",
        )
        .await;
    }

    async fn run_fake(version: &str, accepts: &str, rejects: &str, bad_opt: &str) {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("flash_args"),
            "--flash-mode dio\n0x0 a.bin\n0x10000 b.bin\n",
        )
        .unwrap();
        let fake = tmp.path().join("esptool");
        std::fs::write(
            &fake,
            format!("#!/bin/sh\ncase \"$1\" in version) echo '{version}'; exit 0;; esac\nfor a in \"$@\"; do\n if [ \"$a\" = '{rejects}' ]; then echo \"A fatal error occurred: invalid choice $a\"; exit 2; fi\n if [ \"$a\" = '{bad_opt}' ]; then echo 'A fatal error occurred: falsche Option'; exit 2; fi\ndone\ncase \" $* \" in *' {accepts} '*) ;; *) echo 'A fatal error occurred: Befehl fehlt'; exit 2;; esac\necho 'Connecting....'\necho 'Chip type:          ESP32-S3 (QFN56) (revision v0.2)'\necho 'MAC:                7c:df:a1:12:34:56'\nfor r in 1 2; do\n printf 'Writing at 0x00000000 [==>    ]  50.0%% 10/20 bytes...\\r'\n printf 'Writing at 0x00000000 [=====>] 100.0%% 20/20 bytes...\\n'\n echo 'Wrote 20 bytes (15 compressed) at 0x00000000 in 0.1 seconds'\n echo 'Hash of data verified.'\ndone\necho 'Hard resetting via RTS pin...'\n"),
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut events = Vec::new();
        let out = flash(
            &FlashRequest {
                esptool: fake,
                chip: "esp32s3".into(),
                port: "/dev/cu.fake".into(),
                baud: 921600,
                build_dir: tmp.path().into(),
                extra: vec![],
                erase_all: false,
                skip_firmware: false,
            },
            &CancellationToken::new(),
            |e| events.push(e),
        )
        .await
        .unwrap();
        assert_eq!(out.chip.as_deref(), Some("ESP32-S3"));
        assert_eq!(out.mac.as_deref(), Some("7c:df:a1:12:34:56"));
        assert_eq!(out.bytes, 40);
        let phases: Vec<(usize, f32)> = events
            .iter()
            .filter_map(|e| {
                if let BuildEvent::Phase {
                    stage, progress, ..
                } = e
                {
                    Some((*stage, *progress))
                } else {
                    None
                }
            })
            .collect();
        assert!(phases.contains(&(2, 1.0)) && phases.contains(&(3, 1.0)));
        assert!(phases.contains(&(4, 0.25)), "{phases:?}");
        assert!(phases.contains(&(4, 1.0)) && phases.contains(&(5, 1.0)));
    }
}
