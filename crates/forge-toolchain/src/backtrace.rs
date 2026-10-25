//! Backtrace-Decoder: Adressen aus „Backtrace:“- und Guru-Meditation-Zeilen werden mit
//! `addr2line` des Toolchains und der ELF-Datei des letzten Builds zu Funktion, Datei und Zeile.

use crate::memory::{compiler_from_db, find_elf, sibling_tool};
use crate::process::{capture, os};
use crate::{Error, Result};
use serde::Serialize;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Frame {
    pub addr: String,
    pub function: String,
    pub file: String,
    pub line: u32,
}

fn hex_at(s: &str) -> Option<u64> {
    let rest = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    let len = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
    if len == 0 || len > 8 {
        return None;
    }
    u64::from_str_radix(&rest[..len], 16).ok()
}

/// Codeadressen aus einer Monitor-Zeile, in der Reihenfolge ihres Auftretens ohne Doppelte.
///
/// - `Backtrace: 0x400d1234:0x3ffb1f50 …` (Xtensa): nur die Adresse vor dem `:` ist Code.
/// - `Backtrace: 0x42001234 0x42001300` (RISC-V ohne Stapelwerte).
/// - `PC      : 0x400d2b6c` / `MEPC    : 0x42001234` / `RA      : …` (Register-Dump).
pub fn parse_addresses(line: &str) -> Vec<u64> {
    let mut out: Vec<u64> = Vec::new();
    let mut push = |a: u64| {
        if a != 0 && !out.contains(&a) {
            out.push(a);
        }
    };
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("Backtrace:") {
        for token in rest.split_whitespace() {
            if let Some(pc) = hex_at(token) {
                push(pc);
            }
        }
        return out;
    }
    for key in ["PC", "MEPC", "RA"] {
        let mut from = 0;
        while let Some(i) = trimmed[from..].find(key) {
            let at = from + i;
            from = at + key.len();
            if at > 0 && trimmed.as_bytes()[at - 1].is_ascii_alphanumeric() {
                continue;
            }
            let after = trimmed[at + key.len()..].trim_start();
            let Some(after) = after.strip_prefix(':') else {
                continue;
            };
            if let Some(pc) = hex_at(after.trim_start()) {
                push(pc);
            }
        }
    }
    out
}

/// Liest die Ausgabe von `addr2line -a -f -C -e elf …`: je Adresse drei Zeilen (Adresse, Funktion, Datei:Zeile).
pub fn parse_addr2line(text: &str) -> Vec<Frame> {
    let lines: Vec<&str> = text.lines().collect();
    let mut frames = Vec::new();
    let mut i = 0;
    while i + 2 < lines.len() {
        let addr = lines[i].trim();
        if !addr.starts_with("0x") {
            i += 1;
            continue;
        }
        let function = lines[i + 1].trim();
        let loc = lines[i + 2].trim();
        i += 3;
        // „file:12“ oder „file:12 (discriminator 3)“.
        let loc = loc.split(" (").next().unwrap_or(loc);
        let (file, line) = match loc.rsplit_once(':') {
            Some((f, l)) => (f, l.parse::<u32>().unwrap_or(0)),
            None => (loc, 0),
        };
        if function == "??" && (file == "??" || file.is_empty()) {
            continue;
        }
        frames.push(Frame {
            addr: addr.to_string(),
            function: function.to_string(),
            file: file.to_string(),
            line,
        });
    }
    frames
}

/// Löst Adressen mit einem gegebenen `addr2line` auf.
pub async fn decode_with(addr2line: &Path, elf: &Path, addrs: &[u64]) -> Result<Vec<Frame>> {
    if addrs.is_empty() {
        return Ok(Vec::new());
    }
    let mut args = os(&["-a", "-f", "-C", "-e", &elf.to_string_lossy()]);
    args.extend(addrs.iter().map(|a| format!("0x{a:08x}").into()));
    let out = capture(addr2line, &args, None, "C", Some(Duration::from_secs(20))).await?;
    if !out.success() {
        return Err(Error::Failed(out.stderr.trim().to_string()));
    }
    Ok(parse_addr2line(&out.stdout))
}

/// Decodiert Adressen gegen den letzten Build in `build_dir`.
pub async fn decode(build_dir: &Path, addrs: &[u64]) -> Result<Vec<Frame>> {
    let elf = find_elf(build_dir)
        .ok_or_else(|| Error::Failed("Der Build hat keine ELF-Datei. Kompiliere neu.".into()))?;
    let db = std::fs::read_to_string(build_dir.join("compile_commands.json")).map_err(|_| {
        Error::Failed(
            "Zu diesem Build gibt es keine Compile-Datenbank, daher fehlt addr2line.".into(),
        )
    })?;
    let compiler = compiler_from_db(&db)
        .ok_or_else(|| Error::Failed("Der Compiler des Builds ist nicht zu finden.".into()))?;
    let tool = sibling_tool(&compiler, "addr2line").unwrap_or_default();
    decode_with(&tool, &elf, addrs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xtensa_backtrace_takes_only_code_addresses() {
        let l = "Backtrace: 0x400d1234:0x3ffb1f50 0x400d5678:0x3ffb1f70 0x400d1234:0x3ffb1f90";
        assert_eq!(parse_addresses(l), [0x400d1234, 0x400d5678]);
    }

    #[test]
    fn register_dump_lines_are_read() {
        let l = "PC      : 0x400d2b6c  PS      : 0x00060830  A0      : 0x800d1111";
        assert_eq!(parse_addresses(l), [0x400d2b6c]);
        let l = "MEPC    : 0x42001234  RA      : 0x42001300  SP      : 0x3fc9a000";
        assert_eq!(parse_addresses(l), [0x42001234, 0x42001300]);
        assert!(parse_addresses("EXCVADDR: 0x00000000").is_empty());
        assert!(parse_addresses("I (12) wifi: PC is ok").is_empty());
    }

    #[test]
    fn riscv_backtrace_without_stack_values() {
        assert_eq!(
            parse_addresses("Backtrace: 0x42001234 0x42001300"),
            [0x42001234, 0x42001300]
        );
    }

    #[test]
    fn addr2line_output_becomes_frames() {
        let out = "0x400d1234\nloop()\n/home/u/Blink/Blink.ino:12\n0x400d5678\n??\n??:0\n0x400d9999\nvTaskDelay\n/idf/tasks.c:1500 (discriminator 3)\n";
        assert_eq!(
            parse_addr2line(out),
            [
                Frame {
                    addr: "0x400d1234".into(),
                    function: "loop()".into(),
                    file: "/home/u/Blink/Blink.ino".into(),
                    line: 12
                },
                Frame {
                    addr: "0x400d9999".into(),
                    function: "vTaskDelay".into(),
                    file: "/idf/tasks.c".into(),
                    line: 1500
                },
            ]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_the_given_tool_with_all_addresses() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("forge-a2l-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let tool = dir.join("addr2line");
        // Attrappe: gibt für jede Adresse nach `-e <elf>` Funktion und Ort aus.
        std::fs::write(
            &tool,
            "#!/bin/sh\nwhile [ \"$1\" != \"-e\" ]; do shift; done\nshift; shift\nfor a in \"$@\"; do echo \"$a\"; echo fn_$a; echo /p/main.ino:7; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        let frames = decode_with(&tool, Path::new("x.elf"), &[0x400d1234, 0x400d5678])
            .await
            .unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[1].function, "fn_0x400d5678");
        assert_eq!(frames[1].line, 7);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
