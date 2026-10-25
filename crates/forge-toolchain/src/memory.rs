//! Speicheraufschlüsselung eines Builds: Abschnitte aus `size -A` und die größten Symbole aus `nm`.
//! Die Werkzeuge liegen neben dem Compiler, dessen Pfad in der Compile-Datenbank des Builds steht.

use crate::process::{capture, os};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGroup {
    /// `IRAM`, `DRAM`, `Flash-Code` oder `Rodata`.
    pub label: String,
    pub size: u64,
    /// Die Abschnitte, aus denen sich die Gruppe zusammensetzt.
    pub sections: Vec<(String, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolSize {
    pub name: String,
    pub size: u64,
    /// `code`, `data`, `bss` oder `rodata`.
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryDetail {
    pub groups: Vec<MemoryGroup>,
    /// Die größten Symbole, absteigend.
    pub symbols: Vec<SymbolSize>,
}

const TOP_SYMBOLS: usize = 20;

fn group_of(section: &str) -> Option<&'static str> {
    if section.starts_with(".iram0") {
        Some("IRAM")
    } else if section.starts_with(".dram0.heap") || section == ".dram0.dummy" {
        None
    } else if section.starts_with(".dram0") || section == ".noinit" {
        Some("DRAM")
    } else if section.starts_with(".flash.text") {
        Some("Flash-Code")
    } else if section.starts_with(".flash.rodata")
        || section == ".flash.appdesc"
        || section == ".eh_frame"
        || section.starts_with(".flash.init_array")
    {
        Some("Rodata")
    } else {
        None
    }
}

/// Liest die Ausgabe von `size -A`.
pub fn parse_size_a(text: &str) -> Vec<MemoryGroup> {
    const ORDER: [&str; 4] = ["IRAM", "DRAM", "Flash-Code", "Rodata"];
    let mut groups: Vec<MemoryGroup> = ORDER
        .iter()
        .map(|l| MemoryGroup {
            label: (*l).into(),
            size: 0,
            sections: vec![],
        })
        .collect();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let (Some(name), Some(size)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(size) = size.parse::<u64>() else {
            continue;
        };
        let Some(label) = group_of(name) else {
            continue;
        };
        if size == 0 {
            continue;
        }
        if let Some(g) = groups.iter_mut().find(|g| g.label == label) {
            g.size += size;
            g.sections.push((name.to_string(), size));
        }
    }
    groups.retain(|g| g.size > 0);
    groups
}

/// Liest `nm --size-sort -S --radix=d` (Zeilen `adresse größe typ name`) und behält die größten Symbole.
pub fn parse_nm(text: &str, top: usize) -> Vec<SymbolSize> {
    let mut out: Vec<SymbolSize> = text
        .lines()
        .filter_map(|l| {
            let mut p = l.splitn(4, ' ');
            let (_addr, size, kind, name) = (p.next()?, p.next()?, p.next()?, p.next()?);
            let size: u64 = size.parse().ok()?;
            let kind = match kind.chars().next()?.to_ascii_lowercase() {
                't' | 'w' => "code",
                'd' => "data",
                'b' => "bss",
                'r' => "rodata",
                _ => return None,
            };
            (size > 0).then(|| SymbolSize {
                name: name.trim().to_string(),
                size,
                kind: kind.into(),
            })
        })
        .collect();
    out.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    out.truncate(top);
    out
}

#[derive(Deserialize)]
struct DbEntry {
    #[serde(default)]
    arguments: Vec<String>,
    #[serde(default)]
    command: String,
}

/// `…/bin/xtensa-esp32-elf-g++` ergibt `…/bin/xtensa-esp32-elf-<tool>`.
pub fn sibling_tool(compiler: &Path, tool: &str) -> Option<PathBuf> {
    let name = compiler.file_name()?.to_string_lossy().into_owned();
    let prefix = ["-g++", "-gcc", "-c++", "-cc"]
        .iter()
        .find_map(|s| name.strip_suffix(s))?;
    Some(compiler.with_file_name(format!("{prefix}-{tool}")))
}

fn compiler_from_db(db: &str) -> Option<PathBuf> {
    let entries: Vec<DbEntry> = serde_json::from_str(db).ok()?;
    entries.iter().find_map(|e| {
        let first = e
            .arguments
            .first()
            .cloned()
            .or_else(|| e.command.split_whitespace().next().map(str::to_string))?;
        let p = PathBuf::from(first);
        sibling_tool(&p, "size").map(|_| p)
    })
}

fn find_elf(build_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(build_dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.extension().is_some_and(|e| e == "elf") && !p.to_string_lossy().contains("bootloader")
        })
}

/// Untersucht den letzten Build in `build_dir`.
pub async fn memory_detail(build_dir: &Path) -> Result<MemoryDetail> {
    let elf = find_elf(build_dir)
        .ok_or_else(|| Error::Failed("Der Build hat keine ELF-Datei. Kompiliere neu.".into()))?;
    let db = std::fs::read_to_string(build_dir.join("compile_commands.json")).map_err(|_| {
        Error::Failed("Zu diesem Build gibt es keine Compile-Datenbank, daher fehlen die Werkzeuge für die Auswertung.".into())
    })?;
    let compiler = compiler_from_db(&db)
        .ok_or_else(|| Error::Failed("Der Compiler des Builds ist nicht zu finden.".into()))?;
    let size = sibling_tool(&compiler, "size").unwrap_or_default();
    let nm = sibling_tool(&compiler, "nm").unwrap_or_default();

    let sizes = capture(
        &size,
        &os(&["-A", &elf.to_string_lossy()]),
        None,
        "C",
        Some(Duration::from_secs(30)),
    )
    .await?;
    if !sizes.success() {
        return Err(Error::Failed(sizes.stderr.trim().to_string()));
    }
    let symbols = capture(
        &nm,
        &os(&[
            "--size-sort",
            "-S",
            "-C",
            "--radix=d",
            &elf.to_string_lossy(),
        ]),
        None,
        "C",
        Some(Duration::from_secs(60)),
    )
    .await?;
    if !symbols.success() {
        return Err(Error::Failed(symbols.stderr.trim().to_string()));
    }
    Ok(MemoryDetail {
        groups: parse_size_a(&sizes.stdout),
        symbols: parse_nm(&symbols.stdout, TOP_SYMBOLS),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_sections_are_grouped() {
        let out = "x.elf  :\nsection   size  addr\n.rtc.text 0 1\n.iram0.vectors 1028 1\n.iram0.text 82991 2\n.dram0.data 23933 3\n.dram0.bss 23448 4\n.dram0.heap_start 0 5\n.flash.appdesc 256 6\n.flash.rodata 114324 7\n.eh_frame 10528 8\n.flash.text 692652 9\n.debug_info 5089925 0\nTotal 99\n";
        let g = parse_size_a(out);
        let by = |l: &str| g.iter().find(|g| g.label == l).map(|g| g.size);
        assert_eq!(by("IRAM"), Some(1028 + 82991));
        assert_eq!(by("DRAM"), Some(23933 + 23448));
        assert_eq!(by("Flash-Code"), Some(692_652));
        assert_eq!(by("Rodata"), Some(256 + 114_324 + 10_528));
        assert_eq!(g[0].sections.len(), 2);
    }

    #[test]
    fn nm_keeps_the_largest_symbols() {
        let out = "0000000100 0000000040 t small_fn\n0000000200 0000009000 T big::fn(int, char const*)\n0000000300 0000000500 b buffer\n0000000400 0000000100 W weak\n0000000500 0000000010 A absolute\n";
        let s = parse_nm(out, 3);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].name, "big::fn(int, char const*)");
        assert_eq!((s[0].size, s[0].kind.as_str()), (9000, "code"));
        assert_eq!(s[1].kind, "bss");
    }

    #[test]
    fn tools_sit_next_to_the_compiler() {
        let c = Path::new("/t/bin/xtensa-esp32-elf-g++");
        assert_eq!(
            sibling_tool(c, "size").unwrap(),
            Path::new("/t/bin/xtensa-esp32-elf-size")
        );
        assert!(sibling_tool(Path::new("/usr/bin/clang"), "size").is_none());
        let db =
            r#"[{"directory":"/","arguments":["/t/bin/riscv32-esp-elf-g++","-c"],"file":"a.cpp"}]"#;
        assert_eq!(
            compiler_from_db(db).unwrap(),
            Path::new("/t/bin/riscv32-esp-elf-g++")
        );
    }

    /// Mit einem echten Build aus dem Arduino-Cache, falls einer da ist.
    #[tokio::test]
    async fn reads_a_real_build_if_present() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let root = PathBuf::from(home).join("Library/Caches/arduino/sketches");
        let Some(dir) = std::fs::read_dir(root).ok().and_then(|d| {
            d.flatten()
                .map(|e| e.path())
                .find(|p| p.join("compile_commands.json").is_file() && find_elf(p).is_some())
        }) else {
            return;
        };
        let d = memory_detail(&dir).await.unwrap();
        assert!(!d.groups.is_empty());
        assert!(!d.symbols.is_empty() && d.symbols.len() <= TOP_SYMBOLS);
    }
}
