//! API keys for AI providers, kept in the operating system's credential store:
//! Windows Credential Manager, the macOS and iOS Keychain, the Android Keystore, or
//! the Secret Service on Linux. Where none is available (a Linux desktop without a
//! keyring), keys go in a file only this user can read, and the page is told so.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use keyring_core::{CredentialStore, Entry};
use serde::Serialize;

const SERVICE: &str = "io.github.divhanthelion.kjvinterlinear";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Storage {
    Keychain,
    File,
}

#[derive(Debug, Serialize)]
pub struct KeyStatus {
    pub stored: bool,
    pub storage: Storage,
}

pub struct Secrets {
    fallback: PathBuf,
}

impl Secrets {
    /// `dir` is the app's private config directory (for the fallback file).
    pub fn new(dir: PathBuf) -> Self {
        Self { fallback: dir.join("secrets.json") }
    }

    pub fn storage(&self) -> Storage {
        if store().is_some() { Storage::Keychain } else { Storage::File }
    }

    pub fn get(&self, id: &str) -> Result<Option<String>, String> {
        if store().is_some() {
            match entry(id)?.get_password() {
                Ok(key) => return Ok(Some(key)),
                Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(format!("Couldn't read the key from the system keychain: {}", e)),
            }
        }
        // Keys saved before a keychain became available still work
        Ok(self.read_file()?.remove(id))
    }

    pub fn set(&self, id: &str, key: &str) -> Result<Storage, String> {
        let key = key.trim();
        if key.is_empty() {
            return self.delete(id).map(|()| self.storage());
        }
        if store().is_some() {
            entry(id)?
                .set_password(key)
                .map_err(|e| format!("Couldn't save the key in the system keychain: {}", e))?;
            self.remove_from_file(id)?;
            return Ok(Storage::Keychain);
        }
        let mut keys = self.read_file()?;
        keys.insert(id.to_string(), key.to_string());
        self.write_file(&keys)?;
        Ok(Storage::File)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if store().is_some() {
            match entry(id)?.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(format!("Couldn't remove the key from the system keychain: {}", e)),
            }
        }
        self.remove_from_file(id)
    }

    pub fn status(&self, id: &str) -> Result<KeyStatus, String> {
        Ok(KeyStatus { stored: self.get(id)?.is_some(), storage: self.storage() })
    }

    fn read_file(&self) -> Result<BTreeMap<String, String>, String> {
        match fs::read_to_string(&self.fallback) {
            Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(format!("read {}: {}", self.fallback.display(), e)),
        }
    }

    fn remove_from_file(&self, id: &str) -> Result<(), String> {
        let mut keys = self.read_file()?;
        if keys.remove(id).is_some() {
            if keys.is_empty() {
                fs::remove_file(&self.fallback).map_err(|e| e.to_string())?;
            } else {
                self.write_file(&keys)?;
            }
        }
        Ok(())
    }

    fn write_file(&self, keys: &BTreeMap<String, String>) -> Result<(), String> {
        if let Some(dir) = self.fallback.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let tmp = self.fallback.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string(keys).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
        }
        fs::rename(&tmp, &self.fallback).map_err(|e| e.to_string())
    }
}

fn entry(id: &str) -> Result<Entry, String> {
    let store = store().ok_or("no system keychain")?;
    store.build(SERVICE, id, None).map_err(|e| format!("system keychain: {}", e))
}

/// The platform's credential store, set up once; None where it can't be used.
fn store() -> Option<&'static Arc<CredentialStore>> {
    static STORE: OnceLock<Option<Arc<CredentialStore>>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            // Store setup talks to the OS (D-Bus, JNI); never let it take the app down
            std::panic::catch_unwind(platform_store).ok().flatten()
        })
        .as_ref()
}

#[cfg(target_os = "windows")]
fn platform_store() -> Option<Arc<CredentialStore>> {
    Some(windows_native_keyring_store::Store::new().ok()?)
}

#[cfg(target_os = "macos")]
fn platform_store() -> Option<Arc<CredentialStore>> {
    Some(apple_native_keyring_store::keychain::Store::new().ok()?)
}

#[cfg(target_os = "ios")]
fn platform_store() -> Option<Arc<CredentialStore>> {
    Some(apple_native_keyring_store::protected::Store::new().ok()?)
}

#[cfg(target_os = "android")]
fn platform_store() -> Option<Arc<CredentialStore>> {
    Some(android_native_keyring_store::Store::new().ok()?)
}

#[cfg(all(unix, not(any(target_os = "macos", target_os = "ios", target_os = "android"))))]
fn platform_store() -> Option<Arc<CredentialStore>> {
    let store = zbus_secret_service_keyring_store::Store::new().ok()?;
    // A Secret Service that exists but can't store anything (locked, or blocked by a
    // sandbox) is worse than none: check it works before trusting it
    let probe = store.build(SERVICE, "__probe__", None).ok()?;
    probe.set_password("ok").ok()?;
    let _ = probe.delete_credential();
    Some(store)
}
