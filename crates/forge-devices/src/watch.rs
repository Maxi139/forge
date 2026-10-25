use futures_lite::StreamExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// Hält die Beobachtung am Leben. Nach dem Löschen wird `on_change` nicht mehr aufgerufen.
pub struct WatchHandle {
    stop: Arc<AtomicBool>,
}

impl Drop for WatchHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Die Portdatei erscheint kurz nach dem USB-Ereignis. Wir warten, bis Ruhe herrscht.
const SETTLE: Duration = Duration::from_millis(350);
const FALLBACK_POLL: Duration = Duration::from_millis(1200);

/// Ruft `on_change` auf, sobald ein USB-Gerät kommt oder geht. Ereignisse werden gebündelt.
/// Ohne Hotplug-Unterstützung fragt die Funktion regelmäßig ab. Der Aufrufer vergleicht dann die Liste.
pub fn watch(on_change: impl Fn() + Send + 'static) -> WatchHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel::<()>();

    match nusb::watch_devices() {
        Ok(mut events) => {
            std::thread::spawn(move || {
                futures_lite::future::block_on(async move {
                    while events.next().await.is_some() {
                        if tx.send(()).is_err() {
                            break;
                        }
                    }
                });
            });
        }
        Err(_) => {
            std::thread::spawn(move || loop {
                std::thread::sleep(FALLBACK_POLL);
                if tx.send(()).is_err() {
                    break;
                }
            });
        }
    }

    let flag = stop.clone();
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            while rx.recv_timeout(SETTLE).is_ok() {}
            if flag.load(Ordering::Relaxed) {
                break;
            }
            on_change();
        }
    });

    WatchHandle { stop }
}
