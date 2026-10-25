//! KI-Prüfung auf Logikfehler über OpenRouter. Läuft blockweise (je Funktion), mit Cache und Tagesbudget.
//! Alles, was das Gerät verlässt, geht vorher durch den Schwärzer für Secrets.

pub mod budget;
pub mod cache;
pub mod client;
pub mod models;
pub mod prompt;
pub mod service;
pub mod task;
pub mod threads;

pub use client::{AiError, OpenRouter};
pub use service::{review_blocks, BlockResult, BlockState, RunSummary};
