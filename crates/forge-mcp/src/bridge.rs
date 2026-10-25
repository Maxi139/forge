//! Brücke für Clients, die MCP über stdio sprechen. Liest Port und Token aus der Datei der App
//! und reicht jede Zeile an den laufenden Forge-Server weiter. So steht kein Token in `.mcp.json`.

use crate::read_info;
use serde_json::{json, Value};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

async fn post(port: u16, token: &str, body: &str) -> std::io::Result<(u16, String)> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).await?;
    let req = format!("POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
    s.write_all(req.as_bytes()).await?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).await?;
    let text = String::from_utf8_lossy(&raw).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_string())
        .unwrap_or_default();
    Ok((status, body))
}

fn failure(id: &Value, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32000, "message": message } })
        .to_string()
}

/// Läuft, bis stdin schließt.
pub async fn run(info_path: &Path) {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Ok(Some(line)) = lines.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let id = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|v| v.get("id").cloned());
        // Die App kann zwischen zwei Anfragen neu gestartet sein, also jedes Mal neu lesen.
        let reply = match read_info(info_path) {
            Err(_) => id.as_ref().map(|id| {
                failure(
                    id,
                    "Forge läuft nicht. Starte Forge und versuche es erneut.",
                )
            }),
            Ok(info) => match post(info.port, &info.token, &line).await {
                Err(_) => id.as_ref().map(|id| {
                    failure(
                        id,
                        "Forge ist nicht erreichbar. Starte Forge und versuche es erneut.",
                    )
                }),
                Ok((202, _)) => None,
                Ok((200, body)) => Some(body),
                Ok((401, _)) => id.as_ref().map(|id| {
                    failure(
                        id,
                        "Forge hat die Anmeldung abgelehnt. Verbinde Forge neu mit Claude Code.",
                    )
                }),
                Ok((status, _)) => id
                    .as_ref()
                    .map(|id| failure(id, &format!("Forge antwortete mit Status {status}."))),
            },
        };
        if let Some(r) = reply {
            let _ = out.write_all(r.trim().as_bytes()).await;
            let _ = out.write_all(b"\n").await;
            let _ = out.flush().await;
        }
    }
}
