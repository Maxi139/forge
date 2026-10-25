//! Beobachtet den Projektordner. Änderungen von außen (anderer Editor, Claude Code, Git) landen als
//! Ereignis `project:changed` im Fenster, gebündelt in einem kurzen Zeitfenster.

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const DEBOUNCE: Duration = Duration::from_millis(150);
const IGNORED: &[&str] = &[".git", ".forge", "build", "node_modules", ".DS_Store"];

#[derive(Serialize, Clone)]
struct Changed {
    paths: Vec<PathBuf>,
}

#[derive(Default)]
pub struct ProjectWatcher {
    current: Mutex<Option<notify::RecommendedWatcher>>,
}

fn relevant(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.ends_with(".forge-tmp") {
        return false;
    }
    !path
        .components()
        .any(|c| IGNORED.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

impl ProjectWatcher {
    /// Ersetzt die laufende Beobachtung durch eine neue für `root`.
    pub fn watch(&self, app: AppHandle, root: PathBuf) {
        let (tx, rx) = channel::<PathBuf>();
        let watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            for p in event.paths {
                if relevant(&p) {
                    let _ = tx.send(p);
                }
            }
        });
        let Ok(mut watcher) = watcher else { return };
        if watcher.watch(&root, RecursiveMode::Recursive).is_err() {
            return;
        }

        // Der Thread endet, sobald der Watcher (und damit der Sender) ersetzt wird.
        std::thread::spawn(move || loop {
            let Ok(first) = rx.recv() else { break };
            let mut batch = BTreeSet::from([first]);
            loop {
                match rx.recv_timeout(DEBOUNCE) {
                    Ok(p) => {
                        batch.insert(p);
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            let _ = app.emit(
                "project:changed",
                Changed {
                    paths: batch.into_iter().collect(),
                },
            );
            crate::commands::package::touch(&app);
        });

        if let Ok(mut g) = self.current.lock() {
            *g = Some(watcher);
        }
    }

    pub fn stop(&self) {
        if let Ok(mut g) = self.current.lock() {
            *g = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_noise() {
        assert!(!relevant(Path::new("/p/.git/index")));
        assert!(!relevant(Path::new("/p/build/x.o")));
        assert!(!relevant(Path::new("/p/a.ino.forge-tmp")));
        assert!(relevant(Path::new("/p/a.ino")));
        assert!(relevant(Path::new("/p/data/index.html")));
    }
}
