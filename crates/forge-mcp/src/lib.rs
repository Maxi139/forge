//! MCP-Server für Claude Code. Spricht JSON-RPC über HTTP auf 127.0.0.1, geschützt durch ein Token.
//!
//! Der Server kennt nur das Protokoll. Was die Werkzeuge tun, liefert die App über [`Host`]. Alles, was an
//! den Client geht, läuft durch [`Host::redact`], damit nie ein Secret das Programm verlässt.

pub mod bridge;
mod http;
mod info;
mod protocol;

pub use http::{serve, Server};
pub use info::{read_info, write_info, ServerInfo};
pub use protocol::{handle_message, ToolDef, PROTOCOL_VERSIONS};

use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait Host: Send + Sync + 'static {
    /// Alle Werkzeuge mit Beschreibung und Eingabeschema.
    fn tools(&self) -> Vec<ToolDef>;
    /// Führt ein Werkzeug aus. `Err` wird dem Client als Fehlertext gemeldet.
    async fn call(&self, tool: &str, args: Value) -> Result<String, String>;
    /// Entfernt Secrets aus Text, bevor er den Rechner verlässt.
    fn redact(&self, text: &str) -> String {
        text.to_string()
    }
}
