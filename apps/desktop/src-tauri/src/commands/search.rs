//! Textsuche im Projekt, mit dem Stand des Editors für ungespeicherte Dateien.

use crate::state::AppState;
use forge_core::utf16_col;
use regex::RegexBuilder;
use serde::Serialize;
use std::collections::HashMap;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::State;

/// Jede Suche zählt hoch. Läuft eine ältere noch, merkt sie es zwischen zwei Dateien und gibt auf.
static GENERATION: AtomicU64 = AtomicU64::new(0);

const MAX_HITS: usize = 1000;
const MAX_PER_FILE: usize = 200;
const MAX_FILE_BYTES: u64 = 1_000_000;
const SKIP_DIRS: &[&str] = &["node_modules", "build", "target"];
const BINARY_EXT: &[&str] = &[
    "bin", "elf", "o", "a", "png", "jpg", "jpeg", "gif", "webp", "ico", "zip", "gz", "pdf", "woff",
    "woff2", "ttf", "otf", "map",
];

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub file: String,
    /// Ab 1.
    pub line: u32,
    /// Ab 0, UTF-16.
    pub col: u32,
    pub end_col: u32,
    /// Die Zeile, bei sehr langen Zeilen gekürzt. `col` bezieht sich auf die ungekürzte Zeile.
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<Hit>,
    pub truncated: bool,
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !name.starts_with('.') && !SKIP_DIRS.contains(&name.as_str()) {
                collect(&path, out);
            }
        } else if ft.is_file() && !name.starts_with('.') || name == ".gitignore" {
            let ext = path
                .extension()
                .map(|x| x.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if !BINARY_EXT.contains(&ext.as_str())
                && e.metadata()
                    .map(|m| m.len() <= MAX_FILE_BYTES)
                    .unwrap_or(false)
            {
                out.push(path);
            }
        }
    }
}

/// Bytes mit NUL in den ersten 8 KB gelten als Binärdatei, so wie bei Git.
fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8192)].contains(&0)
}

/// Sucht `query` in allen Textdateien unter `root`, ohne Abbruch durch neuere Suchen (nur für Tests).
#[cfg(test)]
fn search(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    overrides: &HashMap<PathBuf, String>,
) -> Result<SearchResult, String> {
    let own = AtomicU64::new(0);
    search_gen(
        root,
        query,
        case_sensitive,
        whole_word,
        use_regex,
        overrides,
        (&own, 0),
    )
}

fn search_gen(
    root: &Path,
    query: &str,
    case_sensitive: bool,
    whole_word: bool,
    use_regex: bool,
    overrides: &HashMap<PathBuf, String>,
    (current, generation): (&AtomicU64, u64),
) -> Result<SearchResult, String> {
    if query.is_empty() {
        return Ok(SearchResult {
            hits: vec![],
            truncated: false,
        });
    }
    let mut pattern = if use_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };
    if whole_word {
        pattern = format!(r"\b(?:{pattern})\b");
    }
    let re = RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .size_limit(1 << 22)
        .build()
        .map_err(|_| "Der Ausdruck ist ungültig.".to_string())?;
    let mut files = Vec::new();
    collect(root, &mut files);
    let mut hits = Vec::new();
    let mut truncated = false;
    'files: for path in files {
        // Eine neuere Suche hat begonnen: das Ergebnis dieser liest niemand mehr.
        if current.load(Ordering::SeqCst) != generation {
            break;
        }
        let text: Cow<str> = match overrides.get(&path) {
            Some(t) => Cow::Borrowed(t.as_str()),
            None => match std::fs::read(&path) {
                Ok(b) if !is_binary(&b) => match String::from_utf8(b) {
                    Ok(t) => Cow::Owned(t),
                    Err(_) => continue,
                },
                _ => continue,
            },
        };
        let mut in_file = 0;
        for (i, line) in text.split('\n').enumerate() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            for m in re.find_iter(line) {
                if m.is_empty() {
                    continue;
                }
                if hits.len() >= MAX_HITS {
                    truncated = true;
                    break 'files;
                }
                if in_file >= MAX_PER_FILE {
                    truncated = true;
                    continue 'files;
                }
                in_file += 1;
                let shown: String = line.chars().take(300).collect();
                hits.push(Hit {
                    file: path.to_string_lossy().into_owned(),
                    line: i as u32 + 1,
                    col: utf16_col(line, m.start()),
                    end_col: utf16_col(line, m.end()),
                    text: shown,
                });
            }
        }
    }
    Ok(SearchResult { hits, truncated })
}

#[tauri::command]
pub async fn search_run(
    state: State<'_, AppState>,
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    regex: bool,
    overrides: HashMap<String, String>,
) -> Result<SearchResult, String> {
    let root = state.root()?;
    let overrides: HashMap<PathBuf, String> = overrides
        .into_iter()
        .map(|(k, v)| (PathBuf::from(k), v))
        .collect();
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    tokio::task::spawn_blocking(move || {
        search_gen(
            &root,
            &query,
            case_sensitive,
            whole_word,
            regex,
            &overrides,
            (&GENERATION, generation),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("a.ino"),
            "int led = 2;\nvoid loop() { digitalWrite(led, HIGH); }\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("src/b.h"),
            "#define LED 5\r\nconst char* s = \"ä led\";\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("build")).unwrap();
        std::fs::write(tmp.path().join("build/x.txt"), "led").unwrap();
        std::fs::write(tmp.path().join("fw.bin"), "led").unwrap();
        tmp
    }

    #[test]
    fn finds_text_case_insensitive_and_skips_build_and_binaries() {
        let tmp = project();
        let r = search(tmp.path(), "led", false, false, false, &HashMap::new()).unwrap();
        let files: Vec<String> = r
            .hits
            .iter()
            .map(|h| h.file.rsplit('/').next().unwrap().to_string())
            .collect();
        assert_eq!(files, ["a.ino", "a.ino", "b.h", "b.h"], "{:?}", r.hits);
        assert_eq!(
            (r.hits[0].line, r.hits[0].col, r.hits[0].end_col),
            (1, 4, 7)
        );
    }

    #[test]
    fn options_and_utf16_columns() {
        let tmp = project();
        let case = search(tmp.path(), "LED", true, false, false, &HashMap::new()).unwrap();
        assert_eq!(case.hits.len(), 1);
        let word = search(tmp.path(), "led", false, true, false, &HashMap::new()).unwrap();
        assert_eq!(word.hits.len(), 4);
        let re = search(
            tmp.path(),
            r"digital\w+",
            false,
            false,
            true,
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(re.hits.len(), 1);
        // „ä“ ist ein UTF-16-Zeichen, die Spalte zählt davon eines.
        let umlaut = search(tmp.path(), "led", false, false, false, &HashMap::new()).unwrap();
        let last = umlaut.hits.last().unwrap();
        assert_eq!((last.line, last.col, last.end_col), (2, 19, 22));
        assert!(search(tmp.path(), "(", false, false, true, &HashMap::new()).is_err());
    }

    #[test]
    fn data_dir_is_searched_and_nul_files_are_skipped() {
        let tmp = project();
        std::fs::create_dir_all(tmp.path().join("data")).unwrap();
        std::fs::write(tmp.path().join("data/index.html"), "<p>led</p>").unwrap();
        std::fs::write(tmp.path().join("blob.dat"), b"led ").unwrap();
        let r = search(tmp.path(), "led", false, false, false, &HashMap::new()).unwrap();
        assert!(r.hits.iter().any(|h| h.file.ends_with("data/index.html")));
        assert!(r.hits.iter().all(|h| !h.file.ends_with("blob.dat")));
        assert!(is_binary(b"a b") && !is_binary(b"abc"));
    }

    #[test]
    fn superseded_search_stops() {
        let tmp = project();
        let current = AtomicU64::new(2);
        let r = search_gen(
            tmp.path(),
            "led",
            false,
            false,
            false,
            &HashMap::new(),
            (&current, 1),
        )
        .unwrap();
        assert!(r.hits.is_empty());
    }

    #[test]
    fn unsaved_text_wins_over_disk() {
        let tmp = project();
        let mut over = HashMap::new();
        over.insert(
            tmp.path().join("a.ino"),
            "nur noch ein neuer Text".to_string(),
        );
        let r = search(tmp.path(), "led", false, false, false, &over).unwrap();
        assert!(r.hits.iter().all(|h| !h.file.ends_with("a.ino")));
    }
}
