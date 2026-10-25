//! Updates über WLAN („OTA“). Erkennung per mDNS (`_arduino._tcp`, das meldet ArduinoOTA) und der
//! espota-Ablauf in Rust: UDP-Einladung mit Größe und MD5, bei Passwort eine Challenge, danach schickt
//! das Board eine TCP-Verbindung zurück und wir übertragen die Firmware. Python ist dafür nicht nötig.

use md5::{Digest, Md5};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SERVICE: &str = "_arduino._tcp.local.";
/// Befehl „Firmware schreiben“ (`U_FLASH`). Dateisysteme (`100`) schreiben wir nicht.
const CMD_FLASH: u32 = 0;
const CMD_AUTH: u32 = 200;
const CHUNK: usize = 1460;

/// Ein Board im WLAN, das ArduinoOTA anbietet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDevice {
    /// Name des Dienstes, meist der Hostname des Boards.
    pub name: String,
    pub hostname: String,
    pub ip: Ipv4Addr,
    pub port: u16,
    pub board: Option<String>,
    /// Das Board verlangt ein OTA-Passwort.
    pub auth: bool,
}

/// Hält die Suche am Leben. Nach dem Löschen endet sie.
pub struct OtaBrowser {
    stop: Arc<AtomicBool>,
    daemon: ServiceDaemon,
}

impl Drop for OtaBrowser {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.daemon.shutdown();
    }
}

/// Sucht dauerhaft nach OTA-Boards. `on_change` bekommt bei jeder Änderung die ganze Liste.
pub fn browse(on_change: impl Fn(Vec<NetworkDevice>) + Send + 'static) -> Option<OtaBrowser> {
    let daemon = ServiceDaemon::new().ok()?;
    let rx = daemon.browse(SERVICE).ok()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let mut found: BTreeMap<String, NetworkDevice> = BTreeMap::new();
        while !flag.load(Ordering::Relaxed) {
            let Ok(event) = rx.recv_timeout(Duration::from_millis(500)) else {
                continue;
            };
            let changed = match event {
                ServiceEvent::ServiceResolved(info) => {
                    let Some(ip) = info.get_addresses_v4().into_iter().next() else {
                        continue;
                    };
                    let name = info
                        .get_fullname()
                        .trim_end_matches(SERVICE)
                        .trim_end_matches('.')
                        .to_string();
                    let prop = |k: &str| info.get_property_val_str(k).map(str::to_string);
                    let dev = NetworkDevice {
                        hostname: info.get_hostname().trim_end_matches('.').to_string(),
                        ip,
                        port: info.get_port(),
                        board: prop("board").filter(|b| !b.is_empty() && b != "unknown"),
                        auth: prop("auth_upload").is_some_and(|v| v == "yes"),
                        name: name.clone(),
                    };
                    found.insert(info.get_fullname().to_string(), dev).as_ref()
                        != found.get(info.get_fullname())
                }
                ServiceEvent::ServiceRemoved(_, full) => found.remove(&full).is_some(),
                _ => false,
            };
            if changed {
                on_change(found.values().cloned().collect());
            }
        }
    });
    Some(OtaBrowser { stop, daemon })
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OtaError {
    #[error("Die Firmware-Datei lässt sich nicht lesen: {0}")]
    File(String),
    #[error("Das Gerät antwortet nicht. Prüfe, ob es im selben WLAN ist und ArduinoOTA läuft.")]
    NoAnswer,
    #[error("Das Gerät verlangt ein OTA-Passwort. Trage es im Geräte-Bereich ein.")]
    PasswordRequired,
    #[error("Das OTA-Passwort stimmt nicht.")]
    WrongPassword,
    #[error("Das Gerät hat den Update abgelehnt: {0}")]
    Rejected(String),
    #[error("Das Gerät hat keine Verbindung für die Übertragung aufgebaut. Eine Firewall kann sie blockieren.")]
    NoConnection,
    #[error("Die Übertragung brach ab: {0}")]
    Transfer(String),
    #[error("Abgebrochen.")]
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct OtaRequest {
    pub ip: IpAddr,
    pub port: u16,
    pub firmware: PathBuf,
    pub password: Option<String>,
    /// Wartezeit auf die Antworten des Boards. Echte Boards brauchen mehrere Sekunden.
    pub timeout: Duration,
}

fn md5_hex(data: &[u8]) -> String {
    Md5::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Antwort auf die Passwort-Herausforderung des Boards, wie sie ArduinoOTA erwartet.
pub fn auth_response(password: &str, nonce: &str, cnonce: &str) -> String {
    md5_hex(format!("{}:{nonce}:{cnonce}", md5_hex(password.as_bytes())).as_bytes())
}

fn io_err(e: std::io::Error) -> OtaError {
    OtaError::Transfer(e.to_string())
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

/// Überträgt die Firmware. Läuft blockierend, daher in `spawn_blocking` aufrufen. Gibt die Bytezahl zurück.
pub fn upload(
    req: &OtaRequest,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32),
) -> Result<u64, OtaError> {
    let data = std::fs::read(&req.firmware).map_err(|e| OtaError::File(e.to_string()))?;
    let (size, md5) = (data.len(), md5_hex(&data));
    let check = || {
        if cancel.load(Ordering::Relaxed) {
            Err(OtaError::Cancelled)
        } else {
            Ok(())
        }
    };

    // Das Board verbindet sich nach der Einladung mit uns. Der Port steht deshalb vorher fest.
    let listener = TcpListener::bind(("0.0.0.0", 0)).map_err(io_err)?;
    listener.set_nonblocking(true).map_err(io_err)?;
    let host_port = listener.local_addr().map_err(io_err)?.port();

    let udp = UdpSocket::bind(("0.0.0.0", 0)).map_err(io_err)?;
    udp.set_read_timeout(Some(req.timeout)).map_err(io_err)?;
    let target = SocketAddr::new(req.ip, req.port);
    udp.connect(target).map_err(io_err)?;
    let local_ip = udp.local_addr().map_err(io_err)?.ip();

    let invite = format!("{CMD_FLASH} {host_port} {size} {md5}\n");
    let mut reply = String::new();
    let mut buf = [0u8; 64];
    for _ in 0..2 {
        check()?;
        udp.send(invite.as_bytes()).map_err(io_err)?;
        match udp.recv(&mut buf) {
            Ok(n) => {
                reply = String::from_utf8_lossy(&buf[..n]).trim().to_string();
                break;
            }
            Err(e) if is_timeout(&e) => {}
            Err(e) => return Err(io_err(e)),
        }
    }
    if reply.is_empty() {
        return Err(OtaError::NoAnswer);
    }

    if let Some(nonce) = reply.strip_prefix("AUTH") {
        let nonce = nonce.trim();
        let password = req
            .password
            .as_deref()
            .filter(|p| !p.is_empty())
            .ok_or(OtaError::PasswordRequired)?;
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let cnonce = md5_hex(format!("{size}{md5}{local_ip}{stamp}").as_bytes());
        let result = auth_response(password, nonce, &cnonce);
        udp.send(format!("{CMD_AUTH} {cnonce} {result}\n").as_bytes())
            .map_err(io_err)?;
        let n = match udp.recv(&mut buf) {
            Ok(n) => n,
            Err(e) if is_timeout(&e) => return Err(OtaError::NoAnswer),
            Err(e) => return Err(io_err(e)),
        };
        let answer = String::from_utf8_lossy(&buf[..n]).trim().to_string();
        if !answer.starts_with("OK") {
            return Err(OtaError::WrongPassword);
        }
    } else if !reply.starts_with("OK") {
        return Err(OtaError::Rejected(reply));
    }

    let started = Instant::now();
    let mut stream = loop {
        check()?;
        match listener.accept() {
            Ok((s, _)) => break s,
            Err(e) if is_timeout(&e) => {
                if started.elapsed() > req.timeout {
                    return Err(OtaError::NoConnection);
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(io_err(e)),
        }
    };
    stream.set_nonblocking(false).map_err(io_err)?;
    stream.set_read_timeout(Some(req.timeout)).map_err(io_err)?;
    stream
        .set_write_timeout(Some(req.timeout))
        .map_err(io_err)?;

    let mut sent = 0usize;
    let mut ok = false;
    for chunk in data.chunks(CHUNK) {
        check()?;
        stream.write_all(chunk).map_err(io_err)?;
        sent += chunk.len();
        progress(sent as f32 / size as f32);
        // Das Board bestätigt jedes Stück mit der Zahl der geschriebenen Bytes.
        let n = stream.read(&mut buf[..10]).map_err(io_err)?;
        ok = String::from_utf8_lossy(&buf[..n]).contains("OK");
    }
    if !ok {
        // Nach dem letzten Stück schreibt das Board noch und meldet dann „OK“.
        let deadline = Instant::now() + Duration::from_secs(60).max(req.timeout);
        loop {
            check()?;
            match stream.read(&mut buf[..32]) {
                Ok(0) => {
                    return Err(OtaError::Transfer(
                        "Das Gerät hat die Verbindung beendet.".into(),
                    ))
                }
                Ok(n) => {
                    let text = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if text.contains("OK") {
                        break;
                    }
                    if text.contains("ERR") || text.contains("FAIL") {
                        return Err(OtaError::Rejected(text.trim().into()));
                    }
                }
                Err(e) if is_timeout(&e) && Instant::now() < deadline => {}
                Err(e) => return Err(io_err(e)),
            }
        }
    }
    progress(1.0);
    Ok(size as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::JoinHandle;

    /// Ein Gerät, das den Ablauf von ArduinoOTA nachstellt. Gibt Port und die empfangenen Bytes zurück.
    fn fake_device(password: Option<&'static str>) -> (u16, JoinHandle<Option<Vec<u8>>>) {
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        udp.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let port = udp.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut buf = [0u8; 256];
            let (n, from) = udp.recv_from(&mut buf).ok()?;
            let invite = String::from_utf8_lossy(&buf[..n]).into_owned();
            let parts: Vec<&str> = invite.trim().split(' ').collect();
            assert_eq!(parts[0], "0");
            let (host_port, size): (u16, usize) =
                (parts[1].parse().unwrap(), parts[2].parse().unwrap());
            if let Some(pw) = password {
                udp.send_to(b"AUTH abc123nonce", from).unwrap();
                let (n, _) = udp.recv_from(&mut buf).ok()?;
                let msg = String::from_utf8_lossy(&buf[..n]).into_owned();
                let p: Vec<&str> = msg.trim().split(' ').collect();
                assert_eq!(p[0], "200");
                if p[2] != auth_response(pw, "abc123nonce", p[1]) {
                    udp.send_to(b"Authentication Failed", from).unwrap();
                    return None;
                }
            }
            udp.send_to(b"OK", from).unwrap();
            let mut tcp = std::net::TcpStream::connect(("127.0.0.1", host_port)).unwrap();
            let mut got = Vec::new();
            let mut chunk = [0u8; CHUNK];
            while got.len() < size {
                let n = tcp.read(&mut chunk).unwrap();
                got.extend_from_slice(&chunk[..n]);
                tcp.write_all(got.len().to_string().as_bytes()).unwrap();
            }
            tcp.write_all(b"OK").unwrap();
            Some(got)
        });
        (port, handle)
    }

    fn firmware(len: usize) -> (tempfile::TempDir, PathBuf, Vec<u8>) {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<u8> = (0..len).map(|i| (i * 7 % 251) as u8).collect();
        let path = dir.path().join("app.bin");
        std::fs::write(&path, &data).unwrap();
        (dir, path, data)
    }

    fn request(port: u16, firmware: PathBuf, password: Option<&str>) -> OtaRequest {
        OtaRequest {
            ip: "127.0.0.1".parse().unwrap(),
            port,
            firmware,
            password: password.map(str::to_string),
            timeout: Duration::from_secs(3),
        }
    }

    #[test]
    fn uploads_firmware_in_chunks_and_reports_progress() {
        let (_dir, path, data) = firmware(10_000);
        let (port, device) = fake_device(None);
        let mut last = 0.0f32;
        let sent = upload(&request(port, path, None), &AtomicBool::new(false), |p| {
            assert!(p >= last);
            last = p;
        })
        .unwrap();
        assert_eq!(sent, 10_000);
        assert_eq!(last, 1.0);
        assert_eq!(device.join().unwrap().unwrap(), data);
    }

    #[test]
    fn answers_the_password_challenge() {
        let (_dir, path, data) = firmware(3_000);
        let (port, device) = fake_device(Some("geheim"));
        upload(
            &request(port, path, Some("geheim")),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(device.join().unwrap().unwrap(), data);
    }

    #[test]
    fn wrong_or_missing_password_is_reported() {
        let (_dir, path, _) = firmware(100);
        let (port, device) = fake_device(Some("geheim"));
        let err = upload(
            &request(port, path.clone(), Some("falsch")),
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap_err();
        assert_eq!(err, OtaError::WrongPassword);
        assert!(device.join().unwrap().is_none());

        let (port, _device) = fake_device(Some("geheim"));
        let err = upload(&request(port, path, None), &AtomicBool::new(false), |_| {}).unwrap_err();
        assert_eq!(err, OtaError::PasswordRequired);
    }

    #[test]
    fn silent_device_times_out() {
        let (_dir, path, _) = firmware(100);
        let silent = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut req = request(silent.local_addr().unwrap().port(), path, None);
        req.timeout = Duration::from_millis(200);
        assert_eq!(
            upload(&req, &AtomicBool::new(false), |_| {}).unwrap_err(),
            OtaError::NoAnswer
        );
    }

    #[test]
    fn md5_matches_known_value() {
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }
}
