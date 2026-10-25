//! Zugriff auf den macOS-Schlüsselbund. Werte liegen unter dem Dienst „Forge“.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("Der Schlüsselbund hat den Zugriff verweigert: {0}")]
    Denied(String),
}

/// Ein Speicher für Secrets. Der echte ist der Schlüsselbund, Tests nutzen den Speicher im RAM.
pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError>;
    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError>;
    fn delete(&self, account: &str) -> Result<(), KeychainError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Project,
    Global,
}

fn project_account(project: &str, name: &str) -> String {
    format!("project:{project}:{name}")
}

fn global_account(name: &str) -> String {
    format!("global:{name}")
}

/// Wert eines Secrets: erst das des Projekts, dann das für alle Projekte.
pub fn resolve(
    store: &dyn SecretStore,
    project: &str,
    name: &str,
) -> Result<Option<(String, Scope)>, KeychainError> {
    if let Some(v) = store.get(&project_account(project, name))? {
        return Ok(Some((v, Scope::Project)));
    }
    Ok(store
        .get(&global_account(name))?
        .map(|v| (v, Scope::Global)))
}

pub fn set(
    store: &dyn SecretStore,
    project: &str,
    name: &str,
    value: &str,
    scope: Scope,
) -> Result<(), KeychainError> {
    let account = if scope == Scope::Global {
        global_account(name)
    } else {
        project_account(project, name)
    };
    store.set(&account, value)
}

pub fn remove(store: &dyn SecretStore, project: &str, name: &str) -> Result<(), KeychainError> {
    store.delete(&project_account(project, name))?;
    store.delete(&global_account(name))
}

/// Zieht die Secrets eines Projekts auf eine neue Kennung um, etwa nach dem Umbenennen eines Sketches, dessen
/// Kennung vom Pfad abhängt. Gibt die Zahl der umgezogenen Werte zurück. Werte für alle Projekte bleiben, wo sie sind.
pub fn migrate_project(
    store: &dyn SecretStore,
    old_id: &str,
    new_id: &str,
    names: &[String],
) -> Result<usize, KeychainError> {
    if old_id == new_id {
        return Ok(0);
    }
    let mut moved = 0;
    for name in names {
        if let Some(v) = store.get(&project_account(old_id, name))? {
            store.set(&project_account(new_id, name), &v)?;
            store.delete(&project_account(old_id, name))?;
            moved += 1;
        }
    }
    Ok(moved)
}

/// Speicher im Arbeitsspeicher für Tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        Ok(self.0.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        self.0
            .lock()
            .unwrap()
            .insert(account.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        self.0.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(target_os = "macos")]
pub struct Keychain;

#[cfg(target_os = "macos")]
impl SecretStore for Keychain {
    fn get(&self, account: &str) -> Result<Option<String>, KeychainError> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(SERVICE, account) {
            Ok(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(None),
            Err(e) => Err(KeychainError::Denied(e.to_string())),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), KeychainError> {
        security_framework::passwords::set_generic_password(SERVICE, account, value.as_bytes())
            .map_err(|e| KeychainError::Denied(e.to_string()))
    }

    fn delete(&self, account: &str) -> Result<(), KeychainError> {
        use security_framework::passwords::delete_generic_password;
        match delete_generic_password(SERVICE, account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(KeychainError::Denied(e.to_string())),
        }
    }
}

#[cfg(target_os = "macos")]
const SERVICE: &str = "Forge";
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_move_to_the_new_id_and_globals_stay() {
        let s = MemoryStore::default();
        set(&s, "alt", "WIFI_PASS", "pw", Scope::Project).unwrap();
        set(&s, "alt", "API", "g", Scope::Global).unwrap();
        let names = vec![
            "WIFI_PASS".to_string(),
            "API".to_string(),
            "LEER".to_string(),
        ];
        assert_eq!(migrate_project(&s, "alt", "neu", &names).unwrap(), 1);
        assert_eq!(
            resolve(&s, "neu", "WIFI_PASS").unwrap(),
            Some(("pw".into(), Scope::Project))
        );
        assert_eq!(s.get(&project_account("alt", "WIFI_PASS")).unwrap(), None);
        assert_eq!(
            resolve(&s, "neu", "API").unwrap(),
            Some(("g".into(), Scope::Global))
        );
        assert_eq!(migrate_project(&s, "neu", "neu", &names).unwrap(), 0);
    }

    #[test]
    fn project_value_wins_over_global_and_global_is_the_fallback() {
        let s = MemoryStore::default();
        assert_eq!(resolve(&s, "p1", "WIFI_PASS").unwrap(), None);
        set(&s, "p1", "WIFI_PASS", "global-pw", Scope::Global).unwrap();
        assert_eq!(
            resolve(&s, "p1", "WIFI_PASS").unwrap(),
            Some(("global-pw".into(), Scope::Global))
        );
        set(&s, "p1", "WIFI_PASS", "mein-pw", Scope::Project).unwrap();
        assert_eq!(
            resolve(&s, "p1", "WIFI_PASS").unwrap(),
            Some(("mein-pw".into(), Scope::Project))
        );
        // Ein anderes Projekt sieht nur den globalen Wert.
        assert_eq!(
            resolve(&s, "p2", "WIFI_PASS").unwrap(),
            Some(("global-pw".into(), Scope::Global))
        );
        remove(&s, "p1", "WIFI_PASS").unwrap();
        assert_eq!(resolve(&s, "p1", "WIFI_PASS").unwrap(), None);
    }

    /// Echter Schlüsselbund: legt einen Eintrag an und löscht ihn wieder.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "braucht Zugriff auf den Schlüsselbund und kann eine Rückfrage auslösen"]
    fn real_keychain_roundtrip() {
        let k = Keychain;
        let account = format!("test:{}", std::process::id());
        assert_eq!(k.get(&account).unwrap(), None);
        k.set(&account, "geheim ä😀").unwrap();
        assert_eq!(k.get(&account).unwrap().as_deref(), Some("geheim ä😀"));
        k.set(&account, "neu").unwrap();
        assert_eq!(k.get(&account).unwrap().as_deref(), Some("neu"));
        k.delete(&account).unwrap();
        assert_eq!(k.get(&account).unwrap(), None);
    }
}
