//! Geräte: Ports finden und erkennen, Hotplug, serieller Monitor, Chip-Erkennung.

pub mod classify;
pub mod monitor;
pub mod ports;
pub mod probe;
pub mod watch;

pub use classify::{classify, Bridge};
pub use monitor::{Line, LineKind, Monitor, MonitorConfig, MonitorEvent, MonitorState};
pub use ports::{list_devices, DeviceInfo};
pub use probe::{probe_chip, ChipProbe};
pub use watch::{watch, WatchHandle};
