//! Serieller Monitor. Er läuft in einem eigenen Thread, bündelt Zeilen für die Oberfläche
//! und verbindet sich selbst neu, wenn das Board per Reset den Port wechselt.

use crate::ports::list_devices;
use serde::Serialize;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Rx,
    Tx,
    Sys,
    Err,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    pub id: u64,
    /// Millisekunden seit dem Start des Monitors.
    pub t: u64,
    pub kind: LineKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum MonitorState {
    Connecting,
    Connected,
    Reconnecting { reason: String },
    Closed,
}

#[derive(Debug, Clone)]
pub enum MonitorEvent {
    Lines(Vec<Line>),
    State(MonitorState),
}

#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub port: String,
    /// Wenn gesetzt, wird der Port nach einem Reset über diese Geräte-ID wiedergefunden.
    pub device_id: Option<String>,
    pub baud: u32,
    /// DTR/RTS beim Öffnen so lassen, wie das System sie setzt. Das startet viele Boards neu.
    pub reset_on_open: bool,
}

/// Verbindung zum Board. Die echte Umsetzung ist ein serieller Port, Tests nutzen eine Attrappe.
pub trait Link: Read + Write + Send {
    /// DTR und RTS setzen (steuern EN und GPIO0 vieler Boards).
    fn set_lines(&mut self, _dtr: bool, _rts: bool) {}
    /// Stellt die Baudrate bei offenem Port um.
    fn set_baud(&mut self, _baud: u32) -> Result<(), String> {
        Ok(())
    }
}

/// Aufträge an den Monitor-Thread.
enum Command {
    Write(Vec<u8>),
    /// Board per DTR/RTS neu starten.
    Reset,
    SetBaud(u32),
}

/// Öffnet eine Verbindung. Der Fehler ist ein fertiger Satz für die Anzeige.
pub type Opener = Box<dyn Fn(&str, u32) -> Result<Box<dyn Link>, String> + Send>;

struct SerialLink(Box<dyn serialport::SerialPort>);

impl Read for SerialLink {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.0.read(buf)
    }
}

impl Write for SerialLink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Link for SerialLink {
    fn set_lines(&mut self, dtr: bool, rts: bool) {
        let _ = self.0.write_data_terminal_ready(dtr);
        let _ = self.0.write_request_to_send(rts);
    }
    fn set_baud(&mut self, baud: u32) -> Result<(), String> {
        self.0.set_baud_rate(baud).map_err(|e| describe(&e))
    }
}

pub fn serial_opener() -> Opener {
    Box::new(|path, baud| {
        serialport::new(path, baud)
            .timeout(READ_TIMEOUT)
            .open()
            .map(|p| Box::new(SerialLink(p)) as Box<dyn Link>)
            .map_err(|e| describe(&e))
    })
}

pub struct Monitor {
    stop: Arc<AtomicBool>,
    write_tx: mpsc::Sender<Command>,
    join: Option<JoinHandle<()>>,
}

/// Entfernt ANSI-Escape-Sequenzen. ESP-IDF färbt Logzeilen ein.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
        } else if c != '\0' {
            out.push(c);
        }
    }
    out
}

/// Längste Zeile, bevor wir sie erzwungen abschließen (falsche Baudrate, Binärdaten ohne Umbruch).
const MAX_LINE: usize = 4096;

/// Setzt Bytes zu Zeilen zusammen.
#[derive(Default)]
pub struct LineSplitter {
    buf: Vec<u8>,
}

impl LineSplitter {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for &b in bytes {
            if b == b'\n' {
                lines.push(self.take());
            } else {
                self.buf.push(b);
                if self.buf.len() >= MAX_LINE {
                    // Nur bis zur letzten vollständigen UTF-8-Grenze abschneiden, der Rest bleibt stehen.
                    let cut = match std::str::from_utf8(&self.buf) {
                        Ok(_) => self.buf.len(),
                        Err(e) if e.valid_up_to() > 0 => e.valid_up_to(),
                        Err(_) => self.buf.len(),
                    };
                    let rest = self.buf.split_off(cut);
                    lines.push(self.take());
                    self.buf = rest;
                }
            }
        }
        lines
    }

    fn take(&mut self) -> String {
        let mut s = String::from_utf8_lossy(&self.buf).into_owned();
        self.buf.clear();
        while s.ends_with('\r') {
            s.pop();
        }
        // Ein einzelnes \r mitten in der Zeile überschreibt bei Terminals den Anfang. Wir behalten das Ende.
        if let Some(i) = s.rfind('\r') {
            s = s[i + 1..].to_string();
        }
        strip_ansi(&s)
    }

    /// Nicht abgeschlossene Zeile, etwa ein Prompt ohne Zeilenumbruch.
    /// Ein angefangenes Mehrbyte-Zeichen am Ende bleibt im Puffer, bis der Rest da ist.
    pub fn take_partial(&mut self) -> Option<String> {
        if self.buf.is_empty() {
            return None;
        }
        let tail = match std::str::from_utf8(&self.buf) {
            Err(e) if e.error_len().is_none() => self.buf.split_off(e.valid_up_to()),
            _ => Vec::new(),
        };
        if self.buf.is_empty() {
            self.buf = tail;
            return None;
        }
        let s = self.take();
        self.buf = tail;
        Some(s)
    }
}

fn kind_of(text: &str) -> LineKind {
    if text.starts_with("E (")
        || text.contains("Guru Meditation")
        || text.starts_with("assert failed")
        || text.contains("abort() was called")
    {
        LineKind::Err
    } else {
        LineKind::Rx
    }
}

fn describe(e: &serialport::Error) -> String {
    use serialport::ErrorKind::*;
    match e.kind() {
        NoDevice => "Port nicht gefunden".into(),
        Io(std::io::ErrorKind::PermissionDenied) => "Port belegt".into(),
        _ => {
            let d = e.description.to_lowercase();
            if d.contains("busy") {
                "Port belegt".into()
            } else if d.contains("no such file") {
                "Port nicht gefunden".into()
            } else {
                e.description.clone()
            }
        }
    }
}

const READ_TIMEOUT: Duration = Duration::from_millis(30);
const BATCH_WINDOW: Duration = Duration::from_millis(40);
const PARTIAL_AFTER: Duration = Duration::from_millis(150);
const RESET_PULSE: Duration = Duration::from_millis(100);
const RETRY: Duration = Duration::from_millis(400);
const RETRY_MAX: Duration = Duration::from_millis(2000);

/// Verwirft Befehle, die sich während der Trennung angesammelt haben (sonst würden sie veraltet
/// nach dem Wiederverbinden gesendet). Eine Baudrate-Änderung bleibt gültig.
fn drop_stale(rx: &mpsc::Receiver<Command>, baud: &mut u32) {
    while let Ok(cmd) = rx.try_recv() {
        if let Command::SetBaud(b) = cmd {
            *baud = b;
        }
    }
}

impl Monitor {
    pub fn start(cfg: MonitorConfig, on_event: impl Fn(MonitorEvent) + Send + 'static) -> Monitor {
        Self::start_with(cfg, serial_opener(), on_event)
    }

    pub fn start_with(
        cfg: MonitorConfig,
        open: Opener,
        on_event: impl Fn(MonitorEvent) + Send + 'static,
    ) -> Monitor {
        let stop = Arc::new(AtomicBool::new(false));
        let (write_tx, write_rx) = mpsc::channel::<Command>();
        let flag = stop.clone();
        let join = std::thread::spawn(move || run(cfg, open, flag, write_rx, on_event));
        Monitor {
            stop,
            write_tx,
            join: Some(join),
        }
    }

    pub fn write(&self, bytes: Vec<u8>) {
        let _ = self.write_tx.send(Command::Write(bytes));
    }

    /// Startet das Board neu: RTS zieht EN kurz auf Masse, DTR bleibt aus, damit es normal bootet.
    pub fn reset(&self) {
        let _ = self.write_tx.send(Command::Reset);
    }

    /// Ändert die Baudrate, ohne den Port zu schließen.
    pub fn set_baud(&self, baud: u32) {
        let _ = self.write_tx.send(Command::SetBaud(baud));
    }

    /// Beendet den Monitor und gibt den Port frei, bevor die Funktion zurückkehrt.
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(
    cfg: MonitorConfig,
    open: Opener,
    stop: Arc<AtomicBool>,
    write_rx: mpsc::Receiver<Command>,
    on_event: impl Fn(MonitorEvent),
) {
    let started = Instant::now();
    let mut next_id = 0u64;
    let mut path = cfg.port.clone();
    let mut baud = cfg.baud;
    let mut first = true;
    let mut last_reason = String::new();
    let mut last_state: Option<MonitorState> = None;
    let mut emit_state = |s: MonitorState, f: &dyn Fn(MonitorEvent)| {
        if last_state.as_ref() != Some(&s) {
            last_state = Some(s.clone());
            f(MonitorEvent::State(s));
        }
    };

    let mut retry = RETRY;
    while !stop.load(Ordering::Relaxed) {
        if !first {
            drop_stale(&write_rx, &mut baud);
        }
        emit_state(
            if first {
                MonitorState::Connecting
            } else {
                MonitorState::Reconnecting {
                    reason: last_reason.clone(),
                }
            },
            &on_event,
        );

        if let Some(id) = &cfg.device_id {
            if let Some(d) = list_devices().into_iter().find(|d| &d.id == id) {
                path = d.port;
            }
        }

        let mut port = match open(&path, baud) {
            Ok(p) => p,
            Err(reason) => {
                last_reason = reason;
                emit_state(
                    MonitorState::Reconnecting {
                        reason: last_reason.clone(),
                    },
                    &on_event,
                );
                sleep_unless(&stop, retry);
                // Langsamer abfragen, damit list_devices() nicht alle 400 ms läuft.
                retry = (retry * 2).min(RETRY_MAX);
                continue;
            }
        };
        retry = RETRY;
        if !first {
            drop_stale(&write_rx, &mut baud);
        }
        if !cfg.reset_on_open {
            port.set_lines(false, false);
        }
        emit_state(MonitorState::Connected, &on_event);
        let was_first = first;
        first = false;

        let mut splitter = LineSplitter::default();
        let mut pending: Vec<Line> = Vec::new();
        let mut pending_since = Instant::now();
        let mut last_rx = Instant::now();
        let mut buf = [0u8; 4096];
        let mut mk = |kind: LineKind, text: String, next_id: &mut u64| {
            *next_id += 1;
            Line {
                id: *next_id,
                t: started.elapsed().as_millis() as u64,
                kind,
                text,
            }
        };
        if !was_first {
            pending.push(mk(
                LineKind::Sys,
                "Verbindung wiederhergestellt.".into(),
                &mut next_id,
            ));
        }

        let lost = loop {
            if stop.load(Ordering::Relaxed) {
                break None;
            }
            while let Ok(cmd) = write_rx.try_recv() {
                if pending.is_empty() {
                    pending_since = Instant::now();
                }
                match cmd {
                    Command::Write(bytes) => {
                        if let Err(e) = port.write_all(&bytes) {
                            break_with(
                                &mut pending,
                                &mut mk,
                                &mut next_id,
                                format!("Senden fehlgeschlagen: {e}"),
                            );
                        } else {
                            let text = String::from_utf8_lossy(&bytes)
                                .trim_end_matches(['\r', '\n'])
                                .to_string();
                            pending.push(mk(LineKind::Tx, text, &mut next_id));
                        }
                    }
                    Command::Reset => {
                        port.set_lines(false, true);
                        std::thread::sleep(RESET_PULSE);
                        port.set_lines(false, false);
                        pending.push(mk(
                            LineKind::Sys,
                            "Board wird neu gestartet.".into(),
                            &mut next_id,
                        ));
                    }
                    Command::SetBaud(b) => match port.set_baud(b) {
                        Ok(()) => {
                            baud = b;
                            pending.push(mk(LineKind::Sys, format!("Baudrate: {b}"), &mut next_id));
                        }
                        Err(e) => break_with(
                            &mut pending,
                            &mut mk,
                            &mut next_id,
                            format!("Baudrate nicht änderbar: {e}"),
                        ),
                    },
                }
            }
            match port.read(&mut buf) {
                Ok(0) => {}
                Ok(n) => {
                    last_rx = Instant::now();
                    for text in splitter.push(&buf[..n]) {
                        if pending.is_empty() {
                            pending_since = Instant::now();
                        }
                        let kind = kind_of(&text);
                        pending.push(mk(kind, text, &mut next_id));
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::TimedOut
                        || e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => break Some(e.to_string()),
            }
            if last_rx.elapsed() > PARTIAL_AFTER {
                if let Some(text) = splitter.take_partial() {
                    if pending.is_empty() {
                        pending_since = Instant::now();
                    }
                    pending.push(mk(LineKind::Rx, text, &mut next_id));
                }
            }
            if !pending.is_empty()
                && (pending.len() >= 80 || pending_since.elapsed() >= BATCH_WINDOW)
            {
                on_event(MonitorEvent::Lines(std::mem::take(&mut pending)));
            }
        };
        if !pending.is_empty() {
            on_event(MonitorEvent::Lines(std::mem::take(&mut pending)));
        }
        drop(port);
        match lost {
            None => break,
            Some(reason) => {
                last_reason = if reason.to_lowercase().contains("no such")
                    || reason.to_lowercase().contains("device not configured")
                {
                    "Verbindung verloren".into()
                } else {
                    reason
                };
                sleep_unless(&stop, RETRY);
            }
        }
    }
    emit_state(MonitorState::Closed, &on_event);
}

fn break_with(
    pending: &mut Vec<Line>,
    mk: &mut impl FnMut(LineKind, String, &mut u64) -> Line,
    id: &mut u64,
    text: String,
) {
    pending.push(mk(LineKind::Err, text, id));
}

fn sleep_unless(stop: &AtomicBool, d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn splitter_caps_long_lines_on_char_boundary() {
        let mut sp = LineSplitter::default();
        // 3-Byte-Zeichen, damit die Grenze 4096 mitten in einem Zeichen liegt.
        let data = "€".repeat(2000);
        let lines = sp.push(data.as_bytes());
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|l| !l.contains('\u{FFFD}')));
        assert!(lines.iter().all(|l| l.len() <= MAX_LINE));
    }

    #[test]
    fn partial_keeps_incomplete_utf8_tail() {
        let mut sp = LineSplitter::default();
        let euro = "€".as_bytes();
        sp.push(&[b'a', euro[0], euro[1]]);
        assert_eq!(sp.take_partial().as_deref(), Some("a"));
        sp.push(&euro[2..]);
        assert_eq!(sp.take_partial().as_deref(), Some("€"));
    }

    #[test]
    fn splits_lines_and_handles_crlf_and_colors() {
        let mut s = LineSplitter::default();
        assert_eq!(s.push(b"hallo\r\nwelt"), ["hallo"]);
        assert_eq!(
            s.push(b"!\n\x1b[0;32mI (12) wifi: ok\x1b[0m\r\n"),
            ["welt!", "I (12) wifi: ok"]
        );
        assert_eq!(s.take_partial(), None);
        s.push(b"> ");
        assert_eq!(s.take_partial().as_deref(), Some("> "));
    }

    #[test]
    fn carriage_return_keeps_the_last_segment() {
        let mut s = LineSplitter::default();
        assert_eq!(s.push(b"10 %\r50 %\r100 %\n"), ["100 %"]);
    }

    #[test]
    fn errors_in_logs_are_marked() {
        assert_eq!(kind_of("E (321) i2c: timeout"), LineKind::Err);
        assert_eq!(
            kind_of("Guru Meditation Error: Core 1 panic'ed"),
            LineKind::Err
        );
        assert_eq!(kind_of("I (321) boot: ok"), LineKind::Rx);
    }

    /// Eine Attrappe ersetzt das Board: Daten hinein, Zeilen heraus, Senden zurück, Verbindungsabbruch und Neuaufbau.
    #[derive(Default)]
    struct Wire {
        incoming: Mutex<std::collections::VecDeque<Vec<u8>>>,
        written: Mutex<Vec<u8>>,
        break_next_read: AtomicBool,
        lines: Mutex<Vec<(bool, bool)>>,
        bauds: Mutex<Vec<u32>>,
    }

    struct FakeLink(Arc<Wire>);

    impl Read for FakeLink {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.0.break_next_read.swap(false, Ordering::Relaxed) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "Device not configured",
                ));
            }
            if let Some(chunk) = self.0.incoming.lock().unwrap().pop_front() {
                buf[..chunk.len()].copy_from_slice(&chunk);
                return Ok(chunk.len());
            }
            std::thread::sleep(Duration::from_millis(5));
            Err(std::io::ErrorKind::TimedOut.into())
        }
    }

    impl Write for FakeLink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.written.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Link for FakeLink {
        fn set_lines(&mut self, dtr: bool, rts: bool) {
            self.0.lines.lock().unwrap().push((dtr, rts));
        }
        fn set_baud(&mut self, baud: u32) -> Result<(), String> {
            self.0.bauds.lock().unwrap().push(baud);
            Ok(())
        }
    }

    fn wait_for(
        events: &Arc<Mutex<Vec<MonitorEvent>>>,
        pred: impl Fn(&[MonitorEvent]) -> bool,
    ) -> bool {
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end {
            if pred(&events.lock().unwrap()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        false
    }

    fn line_count(e: &[MonitorEvent]) -> usize {
        e.iter()
            .filter_map(|x| {
                if let MonitorEvent::Lines(l) = x {
                    Some(l.len())
                } else {
                    None
                }
            })
            .sum()
    }

    #[test]
    fn reads_writes_and_reconnects() {
        let wire = Arc::new(Wire::default());
        let opens = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (w, o) = (wire.clone(), opens.clone());
        // Der erste Öffnungsversuch scheitert, wie bei einem Board, das gerade neu startet.
        let opener: Opener = Box::new(move |_, _| {
            if o.fetch_add(1, Ordering::Relaxed) == 0 {
                Err("Port nicht gefunden".into())
            } else {
                Ok(Box::new(FakeLink(w.clone())))
            }
        });
        let events: Arc<Mutex<Vec<MonitorEvent>>> = Arc::default();
        let sink = events.clone();
        let monitor = Monitor::start_with(
            MonitorConfig {
                port: "/dev/cu.fake".into(),
                device_id: None,
                baud: 115200,
                reset_on_open: true,
            },
            opener,
            move |e| sink.lock().unwrap().push(e),
        );

        assert!(wait_for(&events, |e| e.iter().any(|x| matches!(
            x,
            MonitorEvent::State(MonitorState::Connected)
        ))));
        assert!(events.lock().unwrap().iter().any(|x| matches!(x, MonitorEvent::State(MonitorState::Reconnecting { reason }) if reason == "Port nicht gefunden")));

        wire.incoming
            .lock()
            .unwrap()
            .push_back(b"raw:2048 volts:1.65\r\nE (5) x: kaputt\r\n".to_vec());
        assert!(wait_for(&events, |e| line_count(e) >= 2));

        monitor.write(b"hallo\n".to_vec());
        let end = Instant::now() + Duration::from_secs(5);
        while wire.written.lock().unwrap().len() < 6 && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(wire.written.lock().unwrap().as_slice(), b"hallo\n");

        // Verbindungsabbruch: Der Monitor öffnet erneut und meldet es.
        let before = opens.load(Ordering::Relaxed);
        wire.break_next_read.store(true, Ordering::Relaxed);
        assert!(wait_for(&events, |_| opens.load(Ordering::Relaxed) > before));
        wire.incoming
            .lock()
            .unwrap()
            .push_back(b"wieder da\n".to_vec());
        assert!(wait_for(&events, |e| e
            .iter()
            .filter_map(|x| if let MonitorEvent::Lines(l) = x {
                Some(l)
            } else {
                None
            })
            .flatten()
            .any(|l| l.text == "wieder da")));
        monitor.stop();

        let ev = events.lock().unwrap();
        let lines: Vec<&Line> = ev
            .iter()
            .filter_map(|x| {
                if let MonitorEvent::Lines(l) = x {
                    Some(l)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        assert_eq!(lines[0].text, "raw:2048 volts:1.65");
        assert_eq!(lines[1].kind, LineKind::Err);
        assert!(lines
            .iter()
            .any(|l| l.kind == LineKind::Tx && l.text == "hallo"));
        assert!(lines
            .iter()
            .any(|l| l.kind == LineKind::Sys && l.text.contains("wiederhergestellt")));
        assert!(matches!(
            ev.last(),
            Some(MonitorEvent::State(MonitorState::Closed))
        ));
    }

    #[test]
    fn reset_pulses_rts_and_baud_changes_without_reopening() {
        let wire = Arc::new(Wire::default());
        let opens = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (w, o) = (wire.clone(), opens.clone());
        let opener: Opener = Box::new(move |_, _| {
            o.fetch_add(1, Ordering::Relaxed);
            Ok(Box::new(FakeLink(w.clone())))
        });
        let events: Arc<Mutex<Vec<MonitorEvent>>> = Arc::default();
        let sink = events.clone();
        let monitor = Monitor::start_with(
            MonitorConfig {
                port: "/dev/cu.fake".into(),
                device_id: None,
                baud: 115200,
                reset_on_open: true,
            },
            opener,
            move |e| sink.lock().unwrap().push(e),
        );
        assert!(wait_for(&events, |e| e.iter().any(|x| matches!(
            x,
            MonitorEvent::State(MonitorState::Connected)
        ))));
        monitor.reset();
        monitor.set_baud(9600);
        assert!(wait_for(&events, |e| line_count(e) >= 2));
        monitor.stop();
        // Ein Neustart: EN kurz tief (RTS), danach beide Leitungen frei.
        assert_eq!(
            wire.lines.lock().unwrap().as_slice(),
            [(false, true), (false, false)]
        );
        assert_eq!(wire.bauds.lock().unwrap().as_slice(), [9600]);
        assert_eq!(opens.load(Ordering::Relaxed), 1);
    }
}
