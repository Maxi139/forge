//! Anbindung an `arduino-cli`, den Compiler und esptool. Alles läuft als eigener Prozess,
//! mit Ausgabe in Echtzeit und der Möglichkeit abzubrechen.

pub mod build;
pub mod check;
pub mod cli;
pub mod error;
pub mod fbuild;
pub mod flash;
pub mod gcc;
pub mod hash;
pub mod memory;
pub mod models;
pub mod process;
pub mod tools;
pub mod translate;

pub use error::{Error, Result};
pub use tokio_util::sync::CancellationToken;
