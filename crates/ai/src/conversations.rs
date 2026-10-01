//! Saved study-assistant conversations: one JSON file per conversation in a folder,
//! plus an index of their titles and dates so the list opens without reading them
//! all. Files are written whole (temp file, then rename) so a crash never leaves a
//! half-written conversation. The page owns the conversation's shape; this module
//! only needs `id`, and reads `title`, `created`, `updated`, `starred`, `model`,
//! `provider`, and `messages` for the list.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Value, json};

const INDEX: &str = "index.json";

pub struct Conversations {
    dir: PathBuf,
    // Saves and deletes rewrite the index; one at a time
    lock: Mutex<()>,
}

impl Conversations {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, lock: Mutex::new(()) }
    }

    /// Summaries, starred first, then most recently updated.
    pub fn list(&self) -> Result<Vec<Value>, String> {
        let _guard = self.lock.lock().unwrap();
        self.read_index()
    }

    pub fn load(&self, id: &str) -> Result<Value, String> {
        let path = self.file(id)?;
        let text = fs::read_to_string(&path).map_err(|e| format!("That conversation couldn't be opened: {}", e))?;
        serde_json::from_str(&text).map_err(|e| format!("That conversation file is damaged: {}", e))
    }

    pub fn save(&self, conversation: &Value) -> Result<(), String> {
        let id = conversation.get("id").and_then(Value::as_str).ok_or("conversation has no id")?;
        let path = self.file(id)?;
        let _guard = self.lock.lock().unwrap();
        fs::create_dir_all(&self.dir).map_err(|e| format!("create {}: {}", self.dir.display(), e))?;
        write_atomic(&path, &serde_json::to_string(conversation).map_err(|e| e.to_string())?)?;
        let mut index = self.read_index()?;
        index.retain(|s| s.get("id").and_then(Value::as_str) != Some(id));
        index.push(summary(conversation));
        self.write_index(index)
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.file(id)?;
        let _guard = self.lock.lock().unwrap();
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("delete {}: {}", path.display(), e)),
        }
        let mut index = self.read_index()?;
        index.retain(|s| s.get("id").and_then(Value::as_str) != Some(id));
        self.write_index(index)
    }

    /// `<dir>/<id>.json`, for ids the page makes (letters, digits, `-`, `_`).
    fn file(&self, id: &str) -> Result<PathBuf, String> {
        let ok = !id.is_empty()
            && id.len() <= 64
            && id != "index"
            && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !ok {
            return Err(format!("not a conversation id: {:?}", id));
        }
        Ok(self.dir.join(format!("{}.json", id)))
    }

    /// The index, rebuilt from the files if it's missing or unreadable.
    fn read_index(&self) -> Result<Vec<Value>, String> {
        if let Ok(text) = fs::read_to_string(self.dir.join(INDEX))
            && let Ok(Value::Array(list)) = serde_json::from_str(&text)
        {
            return Ok(list);
        }
        let mut list = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(list);
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json")
                && path.file_name().is_some_and(|n| n != INDEX)
                && let Some(conversation) = fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok())
            {
                list.push(summary(&conversation));
            }
        }
        Ok(sorted(list))
    }

    fn write_index(&self, list: Vec<Value>) -> Result<(), String> {
        let text = serde_json::to_string(&sorted(list)).map_err(|e| e.to_string())?;
        write_atomic(&self.dir.join(INDEX), &text)
    }
}

/// What the list shows for a conversation.
fn summary(c: &Value) -> Value {
    let messages = c.get("messages").and_then(Value::as_array);
    json!({
        "id": c.get("id"),
        "title": c.get("title").and_then(Value::as_str).unwrap_or("Conversation"),
        "created": c.get("created"),
        "updated": c.get("updated"),
        "starred": c.get("starred").and_then(Value::as_bool).unwrap_or(false),
        "model": c.get("model"),
        "provider": c.get("provider"),
        "questions": messages.map_or(0, |m| m.iter().filter(|x| x["role"] == "user").count()),
    })
}

fn sorted(mut list: Vec<Value>) -> Vec<Value> {
    let key = |v: &Value| {
        (
            !v["starred"].as_bool().unwrap_or(false),
            std::cmp::Reverse(v["updated"].as_f64().map(|t| t as i64).unwrap_or(0)),
        )
    };
    list.sort_by_key(key);
    list
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| format!("write {}: {}", tmp.display(), e))?;
    fs::rename(&tmp, path).map_err(|e| format!("replace {}: {}", path.display(), e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (Conversations, PathBuf) {
        let dir = std::env::temp_dir().join(format!("kjv-conversations-{}-{}", std::process::id(), rand_suffix()));
        (Conversations::new(dir.clone()), dir)
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    fn conv(id: &str, title: &str, updated: i64, starred: bool) -> Value {
        json!({"id": id, "title": title, "created": updated, "updated": updated, "starred": starred,
               "model": "m", "provider": "p",
               "messages": [{"role": "user", "content": "q"}, {"role": "assistant", "content": "a"}]})
    }

    #[test]
    fn saves_lists_loads_and_deletes() {
        let (s, dir) = store();
        assert!(s.list().unwrap().is_empty());
        s.save(&conv("a1", "First", 100, false)).unwrap();
        s.save(&conv("b2", "Second", 200, false)).unwrap();
        s.save(&conv("c3", "Kept", 50, true)).unwrap();
        let ids: Vec<String> = s.list().unwrap().iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["c3", "b2", "a1"], "starred first, then newest");
        assert_eq!(s.list().unwrap()[1]["questions"], 1);

        // Saving again replaces, and moves it up
        s.save(&conv("a1", "First, renamed", 300, false)).unwrap();
        let list = s.list().unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!((list[1]["id"].as_str(), list[1]["title"].as_str()), (Some("a1"), Some("First, renamed")));
        assert_eq!(s.load("a1").unwrap()["title"], "First, renamed");

        s.delete("b2").unwrap();
        assert_eq!(s.list().unwrap().len(), 2);
        assert!(s.load("b2").is_err());
        s.delete("b2").unwrap(); // already gone is fine

        // A lost index is rebuilt from the files
        fs::remove_file(dir.join(INDEX)).unwrap();
        let ids: Vec<String> = s.list().unwrap().iter().map(|v| v["id"].as_str().unwrap().to_string()).collect();
        assert_eq!(ids, ["c3", "a1"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ids_cannot_reach_outside_the_folder() {
        let (s, _dir) = store();
        for bad in ["../x", "a/b", "a\\b", "", "index", "index.json", &"x".repeat(65)] {
            assert!(s.save(&json!({"id": bad})).is_err(), "{:?}", bad);
            assert!(s.load(bad).is_err());
            assert!(s.delete(bad).is_err());
        }
    }
}
