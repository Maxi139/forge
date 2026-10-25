use crate::Host;
use serde::Serialize;
use serde_json::{json, Value};

/// Unterstützte Protokollversionen, die neueste zuerst.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    /// Nur lesen oder auch etwas am Gerät oder Projekt ändern.
    #[serde(skip)]
    pub read_only: bool,
}

impl ToolDef {
    pub fn new(name: &str, description: &str, read_only: bool, properties: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            read_only,
            input_schema: json!({ "type": "object", "properties": properties, "additionalProperties": false }),
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: Value, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

/// Beantwortet eine JSON-RPC-Nachricht. Benachrichtigungen (ohne `id`) bekommen keine Antwort.
pub async fn handle_message(host: &dyn Host, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else { return None };
    if method.is_empty() {
        // Antworten des Clients auf Anfragen des Servers gibt es nicht.
        return None;
    }
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let wanted = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("");
            let version = PROTOCOL_VERSIONS
                .iter()
                .find(|v| **v == wanted)
                .unwrap_or(&PROTOCOL_VERSIONS[0]);
            result(
                id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "forge", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
        }
        "ping" => result(id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = host
                .tools()
                .into_iter()
                .map(|t| {
                    let mut v = serde_json::to_value(&t).unwrap_or(Value::Null);
                    v["annotations"] = json!({ "readOnlyHint": t.read_only });
                    v
                })
                .collect();
            result(id, json!({ "tools": tools }))
        }
        "tools/call" => {
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "Werkzeugname fehlt"));
            };
            if !host.tools().iter().any(|t| t.name == name) {
                return Some(error(id, -32602, &format!("Unbekanntes Werkzeug: {name}")));
            }
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let (text, is_error) = match host.call(name, args).await {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            result(
                id,
                json!({ "content": [{ "type": "text", "text": host.redact(&text) }], "isError": is_error }),
            )
        }
        other => error(id, -32601, &format!("Methode nicht unterstützt: {other}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct Fake;
    #[async_trait]
    impl Host for Fake {
        fn tools(&self) -> Vec<ToolDef> {
            vec![ToolDef::new(
                "echo",
                "Gibt zurück.",
                true,
                json!({ "text": { "type": "string" } }),
            )]
        }
        async fn call(&self, tool: &str, args: Value) -> Result<String, String> {
            match tool {
                "echo" => args
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| "text fehlt".into()),
                _ => Err("?".into()),
            }
        }
        fn redact(&self, text: &str) -> String {
            text.replace("geheim", "[Secret]")
        }
    }

    #[tokio::test]
    async fn initialize_negotiates_version() {
        let r = handle_message(&Fake, json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-03-26" } })).await.unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        assert_eq!(r["result"]["serverInfo"]["name"], "forge");
        let r = handle_message(&Fake, json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize", "params": { "protocolVersion": "1999-01-01" } })).await.unwrap();
        assert_eq!(r["result"]["protocolVersion"], PROTOCOL_VERSIONS[0]);
    }

    #[tokio::test]
    async fn notifications_get_no_reply() {
        assert!(handle_message(
            &Fake,
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })
        )
        .await
        .is_none());
    }

    #[tokio::test]
    async fn lists_tools_with_read_only_hint() {
        let r = handle_message(
            &Fake,
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
        )
        .await
        .unwrap();
        let tool = &r["result"]["tools"][0];
        assert_eq!(tool["name"], "echo");
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["inputSchema"]["type"], "object");
    }

    #[tokio::test]
    async fn call_redacts_output_and_reports_errors() {
        let r = handle_message(&Fake, json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "echo", "arguments": { "text": "das ist geheim" } } })).await.unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "das ist [Secret]");
        assert_eq!(r["result"]["isError"], false);
        let r = handle_message(&Fake, json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": { "name": "echo", "arguments": {} } })).await.unwrap();
        assert_eq!(r["result"]["isError"], true);
    }

    #[tokio::test]
    async fn unknown_tool_and_method_are_protocol_errors() {
        let r = handle_message(&Fake, json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": { "name": "nope" } })).await.unwrap();
        assert_eq!(r["error"]["code"], -32602);
        let r = handle_message(
            &Fake,
            json!({ "jsonrpc": "2.0", "id": 7, "method": "resources/list" }),
        )
        .await
        .unwrap();
        assert_eq!(r["error"]["code"], -32601);
    }
}
