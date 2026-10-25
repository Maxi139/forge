//! Secrets für ESP32-Projekte: im Schlüsselbund gespeichert, per NVS-Partition auf das Board gebracht,
//! nie im Quelltext und nie bei einer KI.

pub mod keychain;
pub mod library;
pub mod nvs;
pub mod redact;
pub mod scan;

pub use redact::Redactor;
