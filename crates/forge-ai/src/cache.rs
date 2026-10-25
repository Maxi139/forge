use crate::prompt::Review;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Pro Projekt. Der Zwischenspeicher reist in der `.forgeproj`-Datei mit und soll sie nicht aufblähen.
const MAX_ENTRIES: usize = 600;

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    at: u64,
    review: Review,
}

/// Merkt sich Prüfungen je Block. Unveränderte Blöcke werden nie ein zweites Mal bezahlt.
/// Die Datei liegt im Projekt (`.forge/ai-cache.json`) und wird mit ihm weitergegeben.
pub struct Cache {
    path: PathBuf,
    map: BTreeMap<String, Entry>,
    dirty: bool,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Cache {
    pub fn load(path: PathBuf) -> Self {
        let map = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Cache {
            path,
            map,
            dirty: false,
        }
    }

    pub fn get(&self, key: &str) -> Option<Review> {
        self.map.get(key).map(|e| e.review.clone())
    }

    pub fn put(&mut self, key: String, review: Review) {
        self.map.insert(key, Entry { at: now(), review });
        self.dirty = true;
    }

    /// Schreibt die Datei, nur wenn sich etwas geändert hat. Älteste Einträge fallen zuerst heraus.
    pub fn save(&mut self) {
        if !self.dirty {
            return;
        }
        if self.map.len() > MAX_ENTRIES {
            let mut by_age: Vec<(String, u64)> =
                self.map.iter().map(|(k, e)| (k.clone(), e.at)).collect();
            by_age.sort_by_key(|(_, at)| *at);
            for (k, _) in by_age.into_iter().take(self.map.len() - MAX_ENTRIES) {
                self.map.remove(&k);
            }
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = self.path.with_extension("tmp");
        if std::fs::write(&tmp, serde_json::to_vec(&self.map).unwrap_or_default()).is_ok() {
            let _ = std::fs::rename(tmp, &self.path);
        }
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_and_prunes() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("c.json");
        let mut c = Cache::load(path.clone());
        assert!(c.get("a").is_none());
        c.put("a".into(), Review { issues: vec![] });
        c.save();
        assert_eq!(
            Cache::load(path.clone()).get("a"),
            Some(Review { issues: vec![] })
        );
        for i in 0..MAX_ENTRIES + 5 {
            c.put(format!("k{i}"), Review { issues: vec![] });
        }
        c.save();
        assert!(Cache::load(path).map.len() <= MAX_ENTRIES);
    }
}
