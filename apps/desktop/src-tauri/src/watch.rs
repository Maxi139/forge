//! Beobachtet den Projektordner. Änderungen von außen (anderer Editor, Claude Code, Git) landen als
//! Ereignis `project:changed` im Fenster, gebündelt in einem kurzen Zeitfenster.

use notify::{recommended_watcher, EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const DEBOUNCE: Duration = Duration::from_millis(150);
/// Ohne Obergrenze würde dauerndes Schreiben (z. B. ein Log) nie ein Ereignis auslösen.
const MAX_WAIT: Duration = Duration::from_secs(1);
const IGNORED: &[&str] = &[".git", ".forge", "build", "node_modules", ".DS_Store"];

#[derive(Serialize, Clone)]
struct Changed {
    paths: Vec<PathBuf>,
}

#[derive(Default)]
pub struct ProjectWatcher {
    current: Mutex<Option<notify::RecommendedWatcher>>,
}

/// `root` bleibt außen vor: liegt das Projekt selbst unter `build/` oder `node_modules/`, zählt das nicht.
fn relevant(root: &Path, path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.ends_with(".forge-tmp") {
        return false;
    }
    let rel = path.strip_prefix(root).unwrap_or(path);
    !rel.components()
        .any(|c| IGNORED.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

impl ProjectWatcher {
    /// Ersetzt die laufende Beobachtung durch eine neue für `root`.
    pub fn watch(&self, app: AppHandle, root: PathBuf) {
        // Zuerst den alten Watcher loswerden: scheitert der neue, darf er nicht weiter für das alte Projekt melden.
        self.stop();
        let (tx, rx) = channel::<PathBuf>();
        // FSEvents meldet kanonische Pfade (z. B. /private/var statt /var).
        let filter_root = root.canonicalize().unwrap_or_else(|_| root.clone());
        let watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            for p in event.paths {
                if relevant(&filter_root, &p) {
                    let _ = tx.send(p);
                }
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                log::warn!("Ordnerbeobachtung lässt sich nicht starten: {e}");
                return;
            }
        };
        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            log::warn!("Ordner {} lässt sich nicht beobachten: {e}", root.display());
            return;
        }

        // Der Thread endet, sobald der Watcher (und damit der Sender) ersetzt wird.
        std::thread::spawn(move || loop {
            let Ok(first) = rx.recv() else { return };
            let mut batch = BTreeSet::from([first]);
            let deadline = Instant::now() + MAX_WAIT;
            loop {
                let wait = DEBOUNCE.min(deadline.saturating_duration_since(Instant::now()));
                if wait.is_zero() {
                    break;
                }
                match rx.recv_timeout(wait) {
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
        let r = Path::new("/p");
        assert!(!relevant(r, Path::new("/p/.git/index")));
        assert!(!relevant(r, Path::new("/p/build/x.o")));
        assert!(!relevant(r, Path::new("/p/a.ino.forge-tmp")));
        assert!(relevant(r, Path::new("/p/a.ino")));
        assert!(relevant(r, Path::new("/p/data/index.html")));
    }

    #[test]
    fn root_itself_may_live_under_ignored_folder() {
        let r = Path::new("/home/u/build/proj");
        assert!(relevant(r, Path::new("/home/u/build/proj/a.ino")));
        assert!(!relevant(r, Path::new("/home/u/build/proj/build/x.o")));
    }
}
