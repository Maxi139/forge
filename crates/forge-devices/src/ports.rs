use crate::classify::{classify, mac_from_serial, Bridge};
use serde::Serialize;
use serialport::SerialPortType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// Stabil über Neustarts, auch wenn der Portname wechselt (Seriennummer, sonst Portname).
    pub id: String,
    /// `/dev/cu.usbmodem1101`
    pub port: String,
    pub vid: u16,
    pub pid: u16,
    pub serial: Option<String>,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub bridge: Bridge,
    /// Bezeichnung des Wandlers oder Chips.
    pub label: String,
    /// Passt zu einem ESP32.
    pub likely_esp: bool,
    pub mac: Option<String>,
}

/// Nur `cu.*`-Geräte sind zum Öffnen gedacht. `tty.*` blockiert beim Öffnen.
fn is_candidate_name(name: &str) -> bool {
    name.starts_with("/dev/cu.")
        && !["Bluetooth-Incoming-Port", "debug-console", "wlan-debug"]
            .iter()
            .any(|n| name.ends_with(n))
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let Ok(ports) = serialport::available_ports() else {
        return Vec::new();
    };
    let mut out: Vec<DeviceInfo> = ports
        .into_iter()
        .filter(|p| is_candidate_name(&p.port_name))
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(u) => {
                let (bridge, likely, label) = classify(u.vid, u.pid);
                let mac = if bridge == Bridge::EspNative {
                    u.serial_number.as_deref().and_then(mac_from_serial)
                } else {
                    None
                };
                let id = match &u.serial_number {
                    Some(s) if !s.is_empty() => {
                        format!("usb:{:04x}:{:04x}:{}", u.vid, u.pid, s.to_lowercase())
                    }
                    _ => format!("port:{}", p.port_name),
                };
                Some(DeviceInfo {
                    id,
                    port: p.port_name,
                    vid: u.vid,
                    pid: u.pid,
                    serial: u.serial_number,
                    product: u.product,
                    manufacturer: u.manufacturer,
                    bridge,
                    label: label.into(),
                    likely_esp: likely,
                    mac,
                })
            }
            _ => None,
        })
        .collect();
    // ESP-Kandidaten zuerst, dann nach Portname.
    out.sort_by(|a, b| {
        b.likely_esp
            .cmp(&a.likely_esp)
            .then_with(|| a.port.cmp(&b.port))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_cu_devices_that_are_not_system_ports_count() {
        assert!(is_candidate_name("/dev/cu.usbmodem1101"));
        assert!(is_candidate_name("/dev/cu.usbserial-0001"));
        assert!(is_candidate_name("/dev/cu.SLAB_USBtoUART"));
        assert!(!is_candidate_name("/dev/tty.usbmodem1101"));
        assert!(!is_candidate_name("/dev/cu.Bluetooth-Incoming-Port"));
        assert!(!is_candidate_name("/dev/cu.debug-console"));
    }

    #[test]
    fn listing_never_panics_without_hardware() {
        let _ = list_devices();
    }
}
