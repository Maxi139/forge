//! Minimaler JSON-RPC-Client für Sprachserver über stdin/stdout.

use crate::{Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

type Pending = Arc<Mutex<HashMap<i64, oneshot::Sender<std::result::Result<Value, String>>>>>;

pub struct Client {
    tx: mpsc::UnboundedSender<Value>,
    pending: Pending,
    next: AtomicI64,
    _child: Child,
}

impl Client {
    pub fn spawn(program: &Path, args: &[String]) -> Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| Error::Spawn(e.to_string()))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Spawn("kein stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Spawn("kein stdout".into()))?;
        let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
        let pending: Pending = Arc::default();

        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let body = msg.to_string();
                let frame = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
                if stdin.write_all(frame.as_bytes()).await.is_err() || stdin.flush().await.is_err()
                {
                    break;
                }
            }
        });

        let reply = tx.clone();
        let waiting = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut len = None;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => {
                            fail_all(&waiting, "Der Sprachserver wurde beendet.");
                            return;
                        }
                        Ok(_) => {}
                    }
                    let line = line.trim();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(v) = line.strip_prefix("Content-Length:") {
                        len = v.trim().parse::<usize>().ok();
                    }
                }
                let Some(len) = len else { continue };
                let mut buf = vec![0u8; len];
                if reader.read_exact(&mut buf).await.is_err() {
                    fail_all(&waiting, "Der Sprachserver wurde beendet.");
                    return;
                }
                let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
                    continue;
                };
                let id = msg.get("id").and_then(Value::as_i64);
                if msg.get("method").is_some() {
                    // Anfragen des Servers (Fortschritt, Registrierungen) beantworten wir ohne Inhalt.
                    if let Some(id) = msg.get("id") {
                        let _ = reply.send(json!({ "jsonrpc": "2.0", "id": id, "result": null }));
                    }
                    continue;
                }
                if let Some(id) = id {
                    if let Some(tx) = waiting.lock().ok().and_then(|mut g| g.remove(&id)) {
                        let outcome = match msg.get("error") {
                            Some(e) => Err(e
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or("Fehler")
                                .to_string()),
                            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(outcome);
                    }
                }
            }
        });

        Ok(Self {
            tx,
            pending,
            next: AtomicI64::new(1),
            _child: child,
        })
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .map_err(|_| Error::Closed)?
            .insert(id, tx);
        self.tx
            .send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .map_err(|_| Error::Closed)?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(Error::Server(e)),
            Ok(Err(_)) => Err(Error::Closed),
            Err(_) => {
                if let Ok(mut g) = self.pending.lock() {
                    g.remove(&id);
                }
                Err(Error::Timeout)
            }
        }
    }

    pub fn notify(&self, method: &str, params: Value) {
        let _ = self
            .tx
            .send(json!({ "jsonrpc": "2.0", "method": method, "params": params }));
    }
}

fn fail_all(pending: &Pending, why: &str) {
    if let Ok(mut g) = pending.lock() {
        for (_, tx) in g.drain() {
            let _ = tx.send(Err(why.to_string()));
        }
    }
}
