use crate::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
}

const HIDDEN: &[&str] = &[".git", ".forge", "build", ".DS_Store", "node_modules"];
const MAX_DEPTH: usize = 6;
const MAX_ENTRIES: usize = 5_000;

/// Dateibaum eines Projekts: Ordner zuerst, dann alphabetisch ohne Beachtung der Groß-/Kleinschreibung.
pub fn list_tree(root: &Path) -> Result<Vec<FileNode>> {
    let mut budget = MAX_ENTRIES;
    walk(root, 0, &mut budget)
}

fn walk(dir: &Path, depth: usize, budget: &mut usize) -> Result<Vec<FileNode>> {
    let mut nodes = Vec::new();
    // Sortiert vor dem Abstieg, damit das Budget bei großen Ordnern deterministisch aufgebraucht wird.
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        if HIDDEN.contains(&name.as_str()) {
            continue;
        }
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let children = if is_dir && depth < MAX_DEPTH {
            // Unlesbarer oder verschwundener Unterordner: als leer behandeln statt alles abzubrechen.
            Some(walk(&path, depth + 1, budget).unwrap_or_default())
        } else if is_dir {
            Some(Vec::new())
        } else {
            None
        };
        nodes.push(FileNode {
            name,
            path,
            is_dir,
            children,
        });
    }
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(nodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_first_and_hidden_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("data")).unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join("b.cpp"), "").unwrap();
        std::fs::write(tmp.path().join("A.h"), "").unwrap();
        std::fs::write(tmp.path().join("data/index.html"), "").unwrap();
        let names: Vec<_> = list_tree(tmp.path())
            .unwrap()
            .into_iter()
            .map(|n| n.name)
            .collect();
        assert_eq!(names, vec!["data", "A.h", "b.cpp"]);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_dir_is_empty() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let locked = tmp.path().join("zu");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(tmp.path().join("a.h"), "").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let tree = list_tree(tmp.path());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(tree.unwrap().len(), 2);
    }
}
