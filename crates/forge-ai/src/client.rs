use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const DEFAULT_BASE: &str = "https://openrouter.ai/api/v1";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AiError {
    #[error("Der OpenRouter-Schlüssel ist ungültig. Prüfe ihn in den Einstellungen.")]
    BadKey,
    #[error("Das Guthaben bei OpenRouter ist aufgebraucht.")]
    NoCredit,
    #[error("OpenRouter bremst gerade. Die Prüfung läuft später weiter.")]
    RateLimited,
    #[error("Keine Verbindung zu OpenRouter.")]
    Offline,
    #[error("OpenRouter antwortet nicht verwertbar: {0}")]
    BadReply(String),
    #[error("OpenRouter meldet einen Fehler ({0}).")]
    Status(u16),
    #[error("Das Tageslimit für die KI-Prüfung ist erreicht.")]
    Budget,
}

pub struct ChatRequest<'a> {
    pub model: &'a str,
    pub system: &'a str,
    pub user: &'a str,
    pub max_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct ChatReply {
    pub content: String,
    pub tokens: u64,
}

#[derive(Clone)]
pub struct OpenRouter {
    http: reqwest::Client,
    base: String,
    key: String,
    /// Wartezeiten zwischen den Wiederholungen.
    retries: Vec<Duration>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct Body<'a> {
    model: &'a str,
    messages: [Message<'a>; 2],
    temperature: f32,
    max_tokens: u32,
    response_format: serde_json::Value,
    /// Denkmodelle verbrauchen sonst die ganze Antwort mit Überlegungen. Das kostet Geld und lässt die Antwort leer.
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Choice {
    message: ReplyMessage,
}

#[derive(Deserialize)]
struct ReplyMessage {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct Usage {
    #[serde(default)]
    total_tokens: u64,
}

fn map_status(code: u16) -> AiError {
    match code {
        401 | 403 => AiError::BadKey,
        402 => AiError::NoCredit,
        429 => AiError::RateLimited,
        other => AiError::Status(other),
    }
}

impl OpenRouter {
    pub fn new(key: String, base: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(45))
            .user_agent("Forge/0.1")
            .build()
            .expect("HTTP-Client");
        Self {
            http,
            base: base
                .unwrap_or_else(|| DEFAULT_BASE.to_string())
                .trim_end_matches('/')
                .to_string(),
            key,
            retries: vec![Duration::from_millis(1500), Duration::from_secs(4)],
        }
    }

    /// Für Tests: ohne Wartezeit wiederholen.
    pub fn with_retries(mut self, retries: Vec<Duration>) -> Self {
        self.retries = retries;
        self
    }

    /// Fragt das Modell. Kurze Aussetzer (Bremse, Serverfehler) wiederholt es zweimal, bevor es aufgibt.
    pub async fn chat(&self, req: ChatRequest<'_>) -> Result<ChatReply, AiError> {
        let mut reasoning_off = true;
        let mut tries = 0;
        loop {
            match self.once(&req, reasoning_off).await {
                // Manche Modelle kennen die Einstellung nicht und lehnen sie ab. Dann ohne.
                Err(AiError::Status(400)) if reasoning_off => reasoning_off = false,
                Err(AiError::RateLimited | AiError::Status(500..=599))
                    if tries < self.retries.len() =>
                {
                    tokio::time::sleep(self.retries[tries]).await;
                    tries += 1;
                }
                other => return other,
            }
        }
    }

    async fn once(&self, req: &ChatRequest<'_>, reasoning_off: bool) -> Result<ChatReply, AiError> {
        let body = Body {
            model: req.model,
            messages: [
                Message {
                    role: "system",
                    content: req.system,
                },
                Message {
                    role: "user",
                    content: req.user,
                },
            ],
            temperature: 0.0,
            max_tokens: req.max_tokens,
            response_format: serde_json::json!({ "type": "json_object" }),
            reasoning: reasoning_off.then(|| serde_json::json!({ "enabled": false })),
        };
        let resp = self
            .http
            .post(format!("{}/chat/completions", self.base))
            .bearer_auth(&self.key)
            .header("HTTP-Referer", "https://forge.local")
            .header("X-Title", "Forge")
            .json(&body)
            .send()
            .await
            .map_err(|_| AiError::Offline)?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(map_status(status));
        }
        let reply: Reply = resp
            .json()
            .await
            .map_err(|e| AiError::BadReply(e.to_string()))?;
        let content = reply
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| AiError::BadReply("leere Antwort".into()))?;
        let tokens = reply
            .usage
            .map(|u| u.total_tokens)
            .filter(|t| *t > 0)
            .unwrap_or_else(|| ((req.system.len() + req.user.len() + content.len()) / 4) as u64);
        Ok(ChatReply { content, tokens })
    }

    /// Die öffentliche Modellliste. Sie braucht keinen Schlüssel.
    pub async fn models(base: Option<&str>) -> Result<serde_json::Value, AiError> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|_| AiError::Offline)?;
        let url = format!(
            "{}/models",
            base.unwrap_or(DEFAULT_BASE).trim_end_matches('/')
        );
        let resp = http.get(url).send().await.map_err(|_| AiError::Offline)?;
        if !resp.status().is_success() {
            return Err(map_status(resp.status().as_u16()));
        }
        resp.json()
            .await
            .map_err(|e| AiError::BadReply(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// Beantwortet nacheinander jede Anfrage mit der nächsten Antwort und merkt sich die Bodies.
    fn sequence(replies: Vec<(u16, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let bodies: Arc<Mutex<Vec<String>>> = Arc::default();
        let log = bodies.clone();
        std::thread::spawn(move || {
            for (status, reply) in replies {
                let Ok((mut sock, _)) = listener.accept() else {
                    return;
                };
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                loop {
                    let n = sock.read(&mut tmp).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                        let len: usize = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                        while buf.len() < p + 4 + len {
                            let n = sock.read(&mut tmp).unwrap_or(0);
                            if n == 0 {
                                break;
                            }
                            buf.extend_from_slice(&tmp[..n]);
                        }
                        log.lock()
                            .unwrap()
                            .push(String::from_utf8_lossy(&buf[p + 4..]).into_owned());
                        break;
                    }
                }
                let resp = format!("HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len());
                let _ = sock.write_all(resp.as_bytes());
            }
        });
        (base, bodies)
    }

    fn ok(content: &str) -> (u16, String) {
        (200, serde_json::json!({ "choices": [{ "message": { "content": content } }], "usage": { "total_tokens": 10 } }).to_string())
    }

    fn req() -> ChatRequest<'static> {
        ChatRequest {
            model: "m",
            system: "s",
            user: "u",
            max_tokens: 50,
        }
    }

    #[tokio::test]
    async fn retries_a_rate_limit_and_then_succeeds() {
        let (base, bodies) = sequence(vec![(429, "{}".into()), (503, "{}".into()), ok("{}")]);
        let client =
            OpenRouter::new("k".into(), Some(base)).with_retries(vec![Duration::from_millis(1); 2]);
        assert_eq!(client.chat(req()).await.unwrap().content, "{}");
        assert_eq!(bodies.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn gives_up_after_the_retries_and_reports_the_limit() {
        let (base, _) = sequence(vec![(429, "{}".into()); 3]);
        let client =
            OpenRouter::new("k".into(), Some(base)).with_retries(vec![Duration::from_millis(1); 2]);
        assert_eq!(client.chat(req()).await.unwrap_err(), AiError::RateLimited);
    }

    #[tokio::test]
    async fn turns_thinking_off_and_drops_it_for_models_that_refuse() {
        let (base, bodies) = sequence(vec![(400, "{}".into()), ok("{}")]);
        let client = OpenRouter::new("k".into(), Some(base));
        client.chat(req()).await.unwrap();
        let sent = bodies.lock().unwrap().clone();
        assert!(
            sent[0].contains("\"reasoning\":{\"enabled\":false}"),
            "{}",
            sent[0]
        );
        assert!(!sent[1].contains("reasoning"), "{}", sent[1]);
    }
}
