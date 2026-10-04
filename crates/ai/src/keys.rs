//! API keys at rest, shared by the app and the browser preview server.
//!
//! A saved key is bound to the server it was saved for, stored as
//! `{"origin":"https://api.openai.com","key":"sk-…"}`, and is only ever sent there:
//! editing a provider's address can't send its key somewhere else. A key saved before
//! keys were bound (a plain string) is bound to the first server it's used with.
//!
//! Also here: scrubbing a key out of error messages, and the file keys fall back to on
//! systems with no keychain.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

/// What a key is replaced with wherever it would be shown.
pub const MASK: &str = "•••";

#[derive(Serialize, Deserialize)]
struct Bound {
    origin: String,
    key: String,
}

/// The origin requests to `base_url` go to: scheme, host, and port (left out when
/// it's the scheme's default), lowercase. `https://API.OpenAI.com:443/v1` →
/// `https://api.openai.com`. The address must pass [`crate::check_base_url`].
pub fn origin(base_url: &str) -> Result<String, String> {
    Ok(crate::check_base_url(base_url)?.origin().ascii_serialization())
}

/// A stored origin in the same form `origin` gives (in case it was written by hand).
fn normalize(origin: &str) -> String {
    reqwest::Url::parse(origin.trim()).map(|u| u.origin().ascii_serialization()).unwrap_or_else(|_| origin.to_string())
}

/// What to store for `key`, bound to `base_url`'s server.
pub fn bind(key: &str, base_url: &str) -> Result<String, String> {
    let bound = Bound { origin: origin(base_url)?, key: key.trim().to_string() };
    serde_json::to_string(&bound).map_err(|e| e.to_string())
}

fn parse(stored: &str) -> Option<Bound> {
    serde_json::from_str(stored).ok()
}

/// The server a stored key is bound to; None for a key saved before keys were bound.
pub fn bound_origin(stored: &str) -> Option<String> {
    parse(stored).map(|b| normalize(&b.origin))
}

/// A stored key, ready for a request.
pub struct Unlocked {
    pub key: String,
    /// Store this in place of the old value: a key saved before keys were bound, now
    /// bound to the server it's being used with (a one-time upgrade)
    pub rebind: Option<String>,
}

/// The key in `stored`, for a request to `base_url`. A key bound to a different server
/// is an error, and the request must not be sent.
pub fn unlock(stored: &str, base_url: &str) -> Result<Unlocked, String> {
    let here = origin(base_url)?;
    match parse(stored) {
        Some(bound) => {
            let saved = normalize(&bound.origin);
            if saved == here {
                Ok(Unlocked { key: bound.key, rebind: None })
            } else {
                Err(format!("This key is saved for {}; re-enter it to use a different server.", saved))
            }
        }
        None => Ok(Unlocked { key: stored.trim().to_string(), rebind: Some(bind(stored, base_url)?) }),
    }
}

/// Provider ids keys can't be saved under (`__test__`, which an older Test button
/// used for keys typed into the form; `__probe__`, the keychain check).
pub fn is_reserved_id(id: &str) -> bool {
    id.starts_with("__")
}

/// `text` with every occurrence of `key` replaced by [`MASK`]. Keys under four
/// characters are left alone: they'd mangle ordinary words and protect nothing.
pub fn redact(text: &str, key: Option<&str>) -> String {
    match key.map(str::trim) {
        Some(key) if key.chars().count() >= 4 => text.replace(key, MASK),
        _ => text.to_string(),
    }
}

/// Keys in a JSON file (`{"provider id": stored value}`) only this user can read, for
/// systems with no keychain. A damaged file is reported, never quietly replaced: the
/// next save first moves it aside to `<name>.bad`.
pub struct KeyFile {
    path: PathBuf,
    lock: Mutex<()>,
}

enum Contents {
    Keys(BTreeMap<String, String>),
    Damaged(String),
}

impl KeyFile {
    pub fn new(path: PathBuf) -> Self {
        Self { path, lock: Mutex::new(()) }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn get(&self, id: &str) -> Result<Option<String>, String> {
        let _guard = self.lock();
        match self.load()? {
            Contents::Keys(mut keys) => Ok(keys.remove(id)),
            Contents::Damaged(why) => Err(format!(
                "The file of saved API keys ({}) is damaged ({}). Enter the key again to start a new file; the \
                 damaged one will be kept as {}.",
                self.path.display(),
                why,
                self.sibling("bad").display()
            )),
        }
    }

    pub fn set(&self, id: &str, stored: &str) -> Result<(), String> {
        let _guard = self.lock();
        let mut keys = self.load_for_change()?;
        keys.insert(id.to_string(), stored.to_string());
        self.save(&keys)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let _guard = self.lock();
        let mut keys = self.load_for_change()?;
        if keys.remove(id).is_none() {
            return Ok(());
        }
        if keys.is_empty() {
            return match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(format!("remove {}: {}", self.path.display(), e)),
            };
        }
        self.save(&keys)
    }

    fn lock(&self) -> MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// `secrets.json` → `secrets.json.<extension>`
    fn sibling(&self, extension: &str) -> PathBuf {
        let mut name = self.path.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(extension);
        self.path.with_file_name(name)
    }

    fn load(&self) -> Result<Contents, String> {
        match fs::read_to_string(&self.path) {
            Ok(text) => Ok(match serde_json::from_str(text.trim_start_matches('\u{FEFF}')) {
                Ok(keys) => Contents::Keys(keys),
                Err(e) => Contents::Damaged(e.to_string()),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Contents::Keys(BTreeMap::new())),
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => Ok(Contents::Damaged("it isn't text".into())),
            Err(e) => Err(format!("read {}: {}", self.path.display(), e)),
        }
    }

    /// The keys, ready to change; a damaged file is moved aside (kept, not overwritten).
    fn load_for_change(&self) -> Result<BTreeMap<String, String>, String> {
        match self.load()? {
            Contents::Keys(keys) => Ok(keys),
            Contents::Damaged(_) => {
                let bad = self.sibling("bad");
                fs::rename(&self.path, &bad)
                    .map_err(|e| format!("move the damaged {} to {}: {}", self.path.display(), bad.display(), e))?;
                Ok(BTreeMap::new())
            }
        }
    }

    /// Write the whole file: a new temp file (only this user can read it, from the
    /// moment it exists), then a rename, so a crash never leaves half a file.
    fn save(&self, keys: &BTreeMap<String, String>) -> Result<(), String> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(|e| format!("create {}: {}", dir.display(), e))?;
        }
        let text = serde_json::to_string(keys).map_err(|e| e.to_string())?;
        let tmp = self.sibling("tmp");
        // Left behind by a crash mid-save; `create_new` needs it gone
        match fs::remove_file(&tmp) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("remove {}: {}", tmp.display(), e)),
        }
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let written = options.open(&tmp).and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        });
        if let Err(e) = written {
            let _ = fs::remove_file(&tmp);
            return Err(format!("write {}: {}", tmp.display(), e));
        }
        fs::rename(&tmp, &self.path).map_err(|e| format!("replace {}: {}", self.path.display(), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE: &str = "sk-test-1111-not-a-real-key";

    #[test]
    fn origins_ignore_case_default_ports_and_paths_but_not_other_ports() {
        let o = |u: &str| origin(u).unwrap();
        assert_eq!(o("https://api.openai.com/v1"), "https://api.openai.com");
        assert_eq!(o("HTTPS://API.OpenAI.COM/v1/"), "https://api.openai.com");
        assert_eq!(o("https://api.openai.com:443/v1"), "https://api.openai.com");
        assert_eq!(o("http://127.0.0.1:80/v1"), o("http://127.0.0.1/v1"));
        assert_eq!(o("http://192.168.1.20:8000/v1"), "http://192.168.1.20:8000");
        assert_ne!(o("http://localhost:8000/v1"), o("http://localhost:8001/v1"));
        assert_ne!(o("http://localhost:8443/v1"), o("https://localhost:8443/v1"));
        assert_ne!(o("https://api.openai.com/v1"), o("https://api.openai.com.evil.example/v1"));
        assert_eq!(o("http://[::1]:8000/v1"), "http://[::1]:8000");
        assert!(origin("http://api.example.com/v1").is_err(), "plain http to the internet");
    }

    #[test]
    fn bound_keys_go_only_to_their_server() {
        let stored = bind(&format!("  {}\n", FAKE), "https://api.openai.com/v1").unwrap();
        assert_eq!(bound_origin(&stored).as_deref(), Some("https://api.openai.com"));
        assert!(stored.contains(r#""key":"sk-test-1111-not-a-real-key""#), "trimmed: {}", stored);

        // Same server, any spelling
        for same in ["https://api.openai.com/v1", "https://API.OPENAI.COM:443/v1/", "https://api.openai.com"] {
            let found = unlock(&stored, same).unwrap();
            assert_eq!((found.key.as_str(), found.rebind.is_none()), (FAKE, true), "{}", same);
        }
        // Another server: refused, with the server it's saved for
        for other in
            ["https://api.openai.com.evil.example/v1", "https://api.deepseek.com/v1", "http://localhost:443/v1"]
        {
            let err = unlock(&stored, other).err().unwrap();
            assert_eq!(err, "This key is saved for https://api.openai.com; re-enter it to use a different server.");
        }
        // A hand-written origin in another case still matches
        let odd = r#"{"origin":"HTTP://LocalHost:8000","key":"abcd"}"#;
        assert_eq!(unlock(odd, "http://localhost:8000/v1").unwrap().key, "abcd");
        assert!(unlock(odd, "http://localhost:8001/v1").is_err());
    }

    #[test]
    fn legacy_keys_are_bound_on_first_use() {
        let found = unlock(FAKE, "http://192.168.1.20:8000/v1").unwrap();
        assert_eq!(found.key, FAKE);
        let rebound = found.rebind.unwrap();
        assert_eq!(bound_origin(&rebound).as_deref(), Some("http://192.168.1.20:8000"));
        assert!(unlock(&rebound, "https://api.openai.com/v1").is_err());
        assert_eq!(bound_origin(FAKE), None);
    }

    #[test]
    fn redaction_removes_every_copy_of_the_key() {
        let text = format!("bad key {k}; got {k}.", k = FAKE);
        assert_eq!(redact(&text, Some(FAKE)), "bad key •••; got •••.");
        assert_eq!(redact(&text, Some(&format!(" {} ", FAKE))), "bad key •••; got •••.");
        assert_eq!(redact("max_tokens too big", Some("x")), "max_tokens too big");
        assert_eq!(redact("no key here", None), "no key here");
    }

    #[test]
    fn reserved_ids() {
        assert!(is_reserved_id("__test__") && is_reserved_id("__probe__"));
        assert!(!is_reserved_id("p1abc") && !is_reserved_id("mock"));
    }

    fn temp_file(name: &str) -> (KeyFile, PathBuf) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("kjv-keys-{}-{}-{}", name, std::process::id(), nanos));
        (KeyFile::new(dir.join("secrets.json")), dir)
    }

    #[test]
    fn key_file_saves_and_removes() {
        let (file, dir) = temp_file("roundtrip");
        assert_eq!(file.get("a").unwrap(), None);
        file.set("a", "one").unwrap();
        file.set("b", "two").unwrap();
        assert_eq!(file.get("a").unwrap().as_deref(), Some("one"));
        file.remove("a").unwrap();
        file.remove("a").unwrap();
        assert_eq!(file.get("a").unwrap(), None);
        file.remove("b").unwrap();
        assert!(!file.path().exists(), "the last key out removes the file");
        // A temp file left by a crash doesn't block saving
        fs::write(dir.join("secrets.json.tmp"), "partial").unwrap();
        file.set("c", "three").unwrap();
        assert_eq!(file.get("c").unwrap().as_deref(), Some("three"));
        assert!(!dir.join("secrets.json.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn damaged_key_file_is_reported_and_kept() {
        let (file, dir) = temp_file("damaged");
        fs::create_dir_all(&dir).unwrap();
        fs::write(file.path(), "{\"a\": \"one\", oops").unwrap();
        let err = file.get("a").unwrap_err();
        assert!(err.contains("damaged") && err.contains("secrets.json.bad"), "{}", err);
        assert!(file.get("a").is_err(), "still an error until a key is saved");

        file.set("b", "two").unwrap();
        assert_eq!(fs::read_to_string(dir.join("secrets.json.bad")).unwrap(), "{\"a\": \"one\", oops");
        assert_eq!(file.get("b").unwrap().as_deref(), Some("two"));
        assert_eq!(file.get("a").unwrap(), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn key_file_is_private_to_this_user() {
        use std::os::unix::fs::PermissionsExt;
        let (file, dir) = temp_file("mode");
        file.set("a", "one").unwrap();
        assert_eq!(fs::metadata(file.path()).unwrap().permissions().mode() & 0o777, 0o600);
        fs::remove_dir_all(dir).unwrap();
    }
}
