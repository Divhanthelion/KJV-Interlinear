//! API keys for AI providers, kept in the operating system's credential store:
//! Windows Credential Manager, the macOS and iOS Keychain, the Android Keystore, or
//! the Secret Service on Linux. Where none is available (a Linux desktop without a
//! keyring), keys go in a file only this user can read, and the page is told so.
//!
//! Each key is saved with the server it's for (see `kjv_ai::keys`) and is only sent
//! there; a key saved before that is bound to the first server it's used with.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use keyring_core::{CredentialStore, Entry};
use kjv_ai::keys::{self, KeyFile};
use serde::Serialize;

const SERVICE: &str = "io.github.divhanthelion.kjvinterlinear";
/// Where an older Test button put keys typed into the provider form
const OLD_TEST_ID: &str = "__test__";

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
    /// The server the key is saved for; None if no key, or one saved before keys
    /// were bound to a server
    pub origin: Option<String>,
}

pub struct Secrets {
    file: KeyFile,
}

impl Secrets {
    /// `dir` is the app's private config directory (for the fallback file).
    pub fn new(dir: PathBuf) -> Self {
        Self { file: KeyFile::new(dir.join("secrets.json")) }
    }

    pub fn storage(&self) -> Storage {
        if store().is_some() { Storage::Keychain } else { Storage::File }
    }

    /// The key to send with a request to `base_url`, if one is saved. A key saved for
    /// a different server is an error, and nothing should be sent.
    pub fn key_for(&self, id: &str, base_url: &str) -> Result<Option<String>, String> {
        let Some(stored) = self.read(id)? else {
            return Ok(None);
        };
        let found = keys::unlock(&stored, base_url)?;
        if let Some(bound) = found.rebind {
            // One-time upgrade of an unbound key; it still works this time if saving fails
            let _ = self.write(id, &bound);
        }
        Ok(Some(found.key))
    }

    /// Save a key; an empty one removes it. With `base_url` the key is bound to that
    /// server. Without it (a page that predates binding) it's saved unbound, and bound
    /// on first use.
    pub fn set(&self, id: &str, key: &str, base_url: Option<&str>) -> Result<Storage, String> {
        let key = key.trim();
        if key.is_empty() {
            return self.delete(id).map(|()| self.storage());
        }
        if keys::is_reserved_id(id) {
            return Err(format!("Keys can't be saved under {:?}.", id));
        }
        let stored = match base_url {
            Some(base) => keys::bind(key, base)?,
            None => key.to_string(),
        };
        let storage = self.write(id, &stored)?;
        // Clear away a typed key the old Test button may have left behind
        let _ = self.delete(OLD_TEST_ID);
        Ok(storage)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if store().is_some() {
            match entry(id)?.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(format!("Couldn't remove the key from the system keychain: {}", e)),
            }
        }
        self.file.remove(id)
    }

    pub fn status(&self, id: &str) -> Result<KeyStatus, String> {
        let stored = self.read(id)?;
        Ok(KeyStatus {
            stored: stored.is_some(),
            storage: self.storage(),
            origin: stored.as_deref().and_then(keys::bound_origin),
        })
    }

    /// The stored value (a bound key's JSON, or an older plain key).
    fn read(&self, id: &str) -> Result<Option<String>, String> {
        if store().is_some() {
            match entry(id)?.get_password() {
                Ok(stored) => return Ok(Some(stored)),
                Err(keyring_core::Error::NoEntry) => {}
                Err(e) => return Err(format!("Couldn't read the key from the system keychain: {}", e)),
            }
        }
        // Keys saved before a keychain became available still work
        self.file.get(id)
    }

    fn write(&self, id: &str, stored: &str) -> Result<Storage, String> {
        if store().is_some() {
            entry(id)?
                .set_password(stored)
                .map_err(|e| format!("Couldn't save the key in the system keychain: {}", e))?;
            self.file.remove(id)?;
            return Ok(Storage::Keychain);
        }
        self.file.set(id, stored)?;
        Ok(Storage::File)
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
    let store: Arc<CredentialStore> = zbus_secret_service_keyring_store::Store::new().ok()?;
    // A Secret Service that exists but can't store anything (locked, or blocked by a
    // sandbox) is worse than none: check it works before trusting it
    let probe = store.build(SERVICE, "__probe__", None).ok()?;
    probe.set_password("ok").ok()?;
    let _ = probe.delete_credential();
    Some(store)
}
