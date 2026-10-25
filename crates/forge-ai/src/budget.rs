use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Default)]
struct Usage {
    day: u64,
    tokens: u64,
}

/// Zählt die Token des Tages, damit eine Prüfung nie mehr kostet als erlaubt.
pub struct Budget {
    path: PathBuf,
    usage: Usage,
}

fn today() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

impl Budget {
    pub fn load(path: PathBuf) -> Self {
        let mut usage: Usage = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if usage.day != today() {
            usage = Usage {
                day: today(),
                tokens: 0,
            };
        }
        Budget { path, usage }
    }

    pub fn used(&self) -> u64 {
        self.usage.tokens
    }

    pub fn allows(&self, limit: u64) -> bool {
        self.usage.tokens < limit
    }

    pub fn add(&mut self, tokens: u64) {
        self.usage.tokens += tokens;
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            &self.path,
            serde_json::to_vec(&self.usage).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_day_and_resets() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("u.json");
        let mut b = Budget::load(p.clone());
        assert!(b.allows(100));
        b.add(60);
        b.add(50);
        assert!(!b.allows(100));
        assert_eq!(Budget::load(p.clone()).used(), 110);
        std::fs::write(&p, "{\"day\":1,\"tokens\":999}").unwrap();
        assert_eq!(Budget::load(p).used(), 0);
    }
}
