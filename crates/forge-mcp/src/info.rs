use serde::{Deserialize, Serialize};
use std::io;
use std::path::Path;

/// Wo der Server lauscht und womit sich Clients ausweisen. Liegt im Datenordner der App, nie im Projekt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerInfo {
    pub port: u16,
    pub token: String,
}

pub fn write_info(path: &Path, info: &ServerInfo) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(info).unwrap_or_default())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub fn read_info(path: &Path) -> io::Result<ServerInfo> {
    let bytes = std::fs::read(path)?;
    serde_json::from_slice(&bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_with_private_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/mcp.json");
        let info = ServerInfo {
            port: 4711,
            token: "abc".into(),
        };
        write_info(&path, &info).unwrap();
        assert_eq!(read_info(&path).unwrap(), info);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
}
