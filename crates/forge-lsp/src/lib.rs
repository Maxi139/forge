//! clangd als Sprachserver für Sketches: Vervollständigung, Hover, Definition, Signaturhilfe und
//! semantische Token. clangd sieht den zusammengesetzten Sketch; Positionen werden auf die
//! Originaldateien zurückgerechnet.

mod map;
mod rpc;
mod session;

pub use map::LineMap;
pub use session::{
    CompletionItem, Hover, Location, Lsp, SemanticTokens, SignatureHelp, StartOptions, Template,
    Token,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("clangd konnte nicht gestartet werden: {0}")]
    Spawn(String),
    #[error("Der Sprachserver antwortet nicht mehr.")]
    Closed,
    #[error("Der Sprachserver antwortet zu langsam.")]
    Timeout,
    #[error("Sprachserver: {0}")]
    Server(String),
    #[error("Dateizugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),
}
