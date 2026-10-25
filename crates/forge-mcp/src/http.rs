use crate::protocol::handle_message;
use crate::Host;
use serde_json::Value;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(300);

pub struct Server {
    pub port: u16,
    task: JoinHandle<()>,
}

impl Server {
    pub fn stop(&self) {
        self.task.abort();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Startet den Server auf 127.0.0.1. Ist der gewünschte Port belegt, nimmt er einen freien.
pub async fn serve(host: Arc<dyn Host>, preferred_port: u16, token: String) -> io::Result<Server> {
    let listener = match TcpListener::bind(("127.0.0.1", preferred_port)).await {
        Ok(l) => l,
        Err(_) => TcpListener::bind(("127.0.0.1", 0)).await?,
    };
    let port = listener.local_addr()?.port();
    let token: Arc<str> = token.into();
    let task = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let host = host.clone();
            let token = token.clone();
            tokio::spawn(async move {
                let _ = connection(stream, host, token, port).await;
            });
        }
    });
    Ok(Server { port, task })
}

struct Request {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream, buf: &mut Vec<u8>) -> io::Result<Option<Request>> {
    loop {
        if let Some(end) = find(buf, b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&buf[..end]).to_string();
            let mut lines = head.split("\r\n");
            let first = lines.next().unwrap_or("");
            let mut parts = first.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let headers: HashMap<String, String> = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
                .collect();
            let len: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if len > MAX_BODY || headers.contains_key("transfer-encoding") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Anfrage zu groß oder gestückelt",
                ));
            }
            let total = end + 4 + len;
            while buf.len() < total {
                let mut chunk = [0u8; 8192];
                let n = tokio::time::timeout(IDLE, stream.read(&mut chunk))
                    .await
                    .map_err(|_| io::ErrorKind::TimedOut)??;
                if n == 0 {
                    return Ok(None);
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body = buf[end + 4..total].to_vec();
            buf.drain(..total);
            return Ok(Some(Request {
                method,
                path,
                headers,
                body,
            }));
        }
        if buf.len() > MAX_HEAD {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Kopfzeilen zu lang",
            ));
        }
        let mut chunk = [0u8; 4096];
        let n = tokio::time::timeout(IDLE, stream.read(&mut chunk))
            .await
            .map_err(|_| io::ErrorKind::TimedOut)??;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Nur Anfragen, die wirklich an diesen Rechner gerichtet sind. Das hält Webseiten fern, die per DNS-Trick kommen.
fn local_host(value: &str, port: u16) -> bool {
    let v = value
        .trim()
        .trim_start_matches("http://")
        .trim_end_matches('/');
    v == format!("127.0.0.1:{port}")
        || v == format!("localhost:{port}")
        || v == "127.0.0.1"
        || v == "localhost"
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    extra: &[(&str, &str)],
    body: &[u8],
    close: bool,
) -> io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nConnection: {}\r\n",
        body.len(),
        if close { "close" } else { "keep-alive" }
    );
    if !body.is_empty() {
        head.push_str("Content-Type: application/json\r\n");
    }
    for (k, v) in extra {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn connection(
    mut stream: TcpStream,
    host: Arc<dyn Host>,
    token: Arc<str>,
    port: u16,
) -> io::Result<()> {
    let mut buf = Vec::new();
    while let Some(req) = read_request(&mut stream, &mut buf).await? {
        let close = req
            .headers
            .get("connection")
            .is_some_and(|c| c.eq_ignore_ascii_case("close"));
        let bearer = req
            .headers
            .get("authorization")
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or("");
        if !same(bearer, &token) {
            respond(
                &mut stream,
                401,
                "Unauthorized",
                &[("WWW-Authenticate", "Bearer")],
                b"",
                close,
            )
            .await?;
            if close {
                break;
            }
            continue;
        }
        let host_ok = req
            .headers
            .get("host")
            .map(|h| local_host(h, port))
            .unwrap_or(false);
        let origin_ok = req
            .headers
            .get("origin")
            .map(|o| {
                local_host(o, port)
                    || o.starts_with("http://localhost")
                    || o.starts_with("http://127.0.0.1")
            })
            .unwrap_or(true);
        if !host_ok || !origin_ok {
            respond(&mut stream, 403, "Forbidden", &[], b"", close).await?;
            if close {
                break;
            }
            continue;
        }
        if req.path.split('?').next() != Some("/mcp") {
            respond(&mut stream, 404, "Not Found", &[], b"", close).await?;
            if close {
                break;
            }
            continue;
        }
        match req.method.as_str() {
            "POST" => {
                let Ok(msg) = serde_json::from_slice::<Value>(&req.body) else {
                    respond(&mut stream, 400, "Bad Request", &[], r#"{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Ungültiges JSON"}}"#.as_bytes(), close).await?;
                    if close {
                        break;
                    }
                    continue;
                };
                let reply = match msg {
                    Value::Array(items) => {
                        let mut out = Vec::new();
                        for m in items {
                            if let Some(r) = handle_message(host.as_ref(), m).await {
                                out.push(r);
                            }
                        }
                        (!out.is_empty()).then_some(Value::Array(out))
                    }
                    single => handle_message(host.as_ref(), single).await,
                };
                match reply {
                    Some(v) => {
                        respond(&mut stream, 200, "OK", &[], v.to_string().as_bytes(), close)
                            .await?
                    }
                    None => respond(&mut stream, 202, "Accepted", &[], b"", close).await?,
                }
            }
            "DELETE" => respond(&mut stream, 200, "OK", &[], b"", close).await?,
            _ => {
                respond(
                    &mut stream,
                    405,
                    "Method Not Allowed",
                    &[("Allow", "POST, DELETE")],
                    b"",
                    close,
                )
                .await?
            }
        }
        if close {
            break;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolDef;
    use async_trait::async_trait;
    use serde_json::json;

    struct Fake;
    #[async_trait]
    impl Host for Fake {
        fn tools(&self) -> Vec<ToolDef> {
            vec![ToolDef::new("ping", "Antwortet.", true, json!({}))]
        }
        async fn call(&self, _tool: &str, _args: Value) -> Result<String, String> {
            Ok("pong".into())
        }
    }

    async fn request(
        port: u16,
        method: &str,
        path: &str,
        headers: &[(&str, &str)],
        body: &str,
    ) -> (u16, String) {
        let mut s = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        req.push_str("\r\n");
        req.push_str(body);
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        // Der Server hält die Verbindung offen: nur so lange lesen, bis die Antwort vollständig ist.
        let mut chunk = [0u8; 4096];
        loop {
            let n = tokio::time::timeout(Duration::from_millis(300), s.read(&mut chunk)).await;
            match n {
                Ok(Ok(0)) | Err(_) | Ok(Err(_)) => break,
                Ok(Ok(n)) => out.extend_from_slice(&chunk[..n]),
            }
        }
        let text = String::from_utf8_lossy(&out).to_string();
        let status = text
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    const AUTH: (&str, &str) = ("Authorization", "Bearer sehr-geheim");

    #[tokio::test]
    async fn requires_the_token() {
        let server = serve(Arc::new(Fake), 0, "sehr-geheim".into())
            .await
            .unwrap();
        let rpc = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        assert_eq!(request(server.port, "POST", "/mcp", &[], rpc).await.0, 401);
        assert_eq!(
            request(
                server.port,
                "POST",
                "/mcp",
                &[("Authorization", "Bearer falsch")],
                rpc
            )
            .await
            .0,
            401
        );
        let (status, body) = request(server.port, "POST", "/mcp", &[AUTH], rpc).await;
        assert_eq!(status, 200);
        assert!(body.contains("\"ping\""), "{body}");
    }

    #[tokio::test]
    async fn calls_tools_and_accepts_notifications() {
        let server = serve(Arc::new(Fake), 0, "sehr-geheim".into())
            .await
            .unwrap();
        let call = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"ping"}}"#;
        let (status, body) = request(server.port, "POST", "/mcp", &[AUTH], call).await;
        assert_eq!(status, 200);
        assert!(body.contains("pong"), "{body}");
        let note = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        assert_eq!(
            request(server.port, "POST", "/mcp", &[AUTH], note).await.0,
            202
        );
    }

    #[tokio::test]
    async fn rejects_foreign_origin_and_unknown_paths() {
        let server = serve(Arc::new(Fake), 0, "sehr-geheim".into())
            .await
            .unwrap();
        let rpc = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        assert_eq!(
            request(
                server.port,
                "POST",
                "/mcp",
                &[AUTH, ("Origin", "https://boese.example")],
                rpc
            )
            .await
            .0,
            403
        );
        assert_eq!(
            request(server.port, "POST", "/anderes", &[AUTH], rpc)
                .await
                .0,
            404
        );
        assert_eq!(
            request(server.port, "GET", "/mcp", &[AUTH], "").await.0,
            405
        );
        assert_eq!(
            request(server.port, "POST", "/mcp", &[AUTH], "{kaputt")
                .await
                .0,
            400
        );
    }

    #[tokio::test]
    async fn falls_back_to_a_free_port() {
        let first = serve(Arc::new(Fake), 0, "a".into()).await.unwrap();
        let second = serve(Arc::new(Fake), first.port, "a".into()).await.unwrap();
        assert_ne!(first.port, second.port);
    }
}
