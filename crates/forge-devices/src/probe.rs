use regex::Regex;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChipProbe {
    /// `ESP32-S3`
    pub chip: String,
    pub mac: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("esptool konnte nicht gestartet werden: {0}")]
    Spawn(std::io::Error),
    #[error("Das Board antwortet nicht. Halte BOOT gedrückt und tippe RESET an.")]
    NoAnswer,
    #[error("Der Port ist belegt. Schließe den Monitor und andere Programme, die ihn nutzen.")]
    Busy,
}

pub fn parse_probe(output: &str) -> Option<ChipProbe> {
    let chip =
        Regex::new(r"Chip type:\s+(ESP32[\w-]*)|Connected to (ESP32[\w-]*)|Chip is (ESP32[\w-]*)")
            .ok()?;
    let c = chip.captures(output)?;
    let name = c.iter().skip(1).flatten().next()?.as_str().to_string();
    let mac = Regex::new(r"MAC:\s+([0-9a-fA-F:]{17})")
        .ok()?
        .captures(output)
        .map(|m| m[1].to_lowercase());
    Some(ChipProbe { chip: name, mac })
}

/// Fragt den Chip über esptool. Das startet das Board neu. Nie aufrufen, solange der Monitor den Port hält.
pub fn probe_chip(esptool: &Path, port: &str, timeout: Duration) -> Result<ChipProbe, ProbeError> {
    let mut child = Command::new(esptool)
        .args([
            "--port",
            port,
            "--before",
            "default-reset",
            "--after",
            "hard-reset",
            "read-mac",
        ])
        .env("LC_ALL", "en_US.UTF-8")
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(ProbeError::Spawn)?;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProbeError::NoAnswer);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(_) => break,
        }
    }
    let mut text = String::new();
    if let Some(o) = out.as_mut() {
        let _ = o.read_to_string(&mut text);
    }
    if let Some(e) = err.as_mut() {
        let _ = e.read_to_string(&mut text);
    }
    if text.contains("Resource busy") || text.contains("port is busy") {
        return Err(ProbeError::Busy);
    }
    parse_probe(&text).ok_or(ProbeError::NoAnswer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_esptool_v5_output() {
        let out = "esptool v5.3.1\nSerial port /dev/cu.usbmodem1101:\nConnecting...\nConnected to ESP32-S3 on /dev/cu.usbmodem1101:\nChip type:          ESP32-S3 (QFN56) (revision v0.2)\nFeatures:           Wi-Fi, BT 5 (LE)\nMAC:                7c:DF:a1:12:34:56\n\nHard resetting via RTS pin...\n";
        assert_eq!(
            parse_probe(out),
            Some(ChipProbe {
                chip: "ESP32-S3".into(),
                mac: Some("7c:df:a1:12:34:56".into())
            })
        );
    }

    #[test]
    fn plain_esp32_and_missing_info() {
        assert_eq!(
            parse_probe("Chip type: ESP32-D0WD-V3 (revision v3.1)")
                .unwrap()
                .chip,
            "ESP32-D0WD-V3"
        );
        assert_eq!(
            parse_probe("A fatal error occurred: Failed to connect"),
            None
        );
    }
}
