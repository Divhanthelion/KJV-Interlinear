//! Binary cache for parsed ExtendedBible data.

use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use bincode::Options;
use serde::{Deserialize, Serialize};

use crate::models::ExtendedBible;
use crate::paths;

/// Bump when ExtendedBible layout or parser semantics change.
pub const CACHE_VERSION: u32 = 3;

const CACHE_FILE: &str = "extended_bible_v1.bin";
const META_FILE: &str = "extended_bible_v1.meta.json";

/// Cap deserialization size so a tampered length prefix cannot OOM the process.
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;

/// Fingerprint of source TSV/lexicon files used to invalidate the cache.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CacheMeta {
    version: u32,
    fingerprint: String,
}

fn cache_dir() -> Option<PathBuf> {
    let dirs = paths::project_dirs()?;
    let dir = dirs.cache_dir().join("original_languages");
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn fingerprint_data_dir(data_dir: &Path) -> Result<String, String> {
    let mut entries: Vec<(String, u64, u64)> = Vec::new();

    let read_dir = fs::read_dir(data_dir)
        .map_err(|e| format!("Failed to read data dir {}: {}", data_dir.display(), e))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Failed to read data entry: {}", e))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // Only fingerprint STEP text sources (not red-letter JSON)
        if !(name.ends_with(".txt")
            && (name.starts_with("TAHOT")
                || name.starts_with("TAGNT")
                || name.starts_with("TBESH")
                || name.starts_with("TBESG")))
        {
            continue;
        }

        let meta = fs::metadata(&path)
            .map_err(|e| format!("Failed to stat {}: {}", path.display(), e))?;
        let len = meta.len();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        entries.push((name, len, mtime));
    }

    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut out = String::new();
    for (name, len, mtime) in entries {
        out.push_str(&format!("{}:{}:{};", name, len, mtime));
    }
    Ok(out)
}

fn meta_matches(data_dir: &Path, meta_path: &Path) -> bool {
    let Ok(expected_fp) = fingerprint_data_dir(data_dir) else {
        return false;
    };
    let Ok(file) = File::open(meta_path) else {
        return false;
    };
    let Ok(meta): Result<CacheMeta, _> = serde_json::from_reader(BufReader::new(file)) else {
        return false;
    };
    meta.version == CACHE_VERSION && meta.fingerprint == expected_fp
}

/// Try to load ExtendedBible from cache. Returns None if missing or stale.
pub fn try_load_cached(data_dir: &Path) -> Option<ExtendedBible> {
    let dir = cache_dir()?;
    let cache_path = dir.join(CACHE_FILE);
    let meta_path = dir.join(META_FILE);

    if !meta_matches(data_dir, &meta_path) {
        return None;
    }

    let file = File::open(&cache_path).ok()?;
    match bincode::options()
        .with_limit(MAX_CACHE_BYTES)
        .deserialize_from(BufReader::new(file))
    {
        Ok(bible) => {
            eprintln!("Loaded original language data from cache");
            Some(bible)
        }
        Err(e) => {
            eprintln!("Warning: corrupt original-language cache ({}), rebuilding", e);
            let _ = fs::remove_file(&cache_path);
            let _ = fs::remove_file(&meta_path);
            None
        }
    }
}

/// Persist ExtendedBible to cache after a successful TSV parse.
pub fn save_cache(data_dir: &Path, bible: &ExtendedBible) {
    let Some(dir) = cache_dir() else {
        return;
    };
    let cache_path = dir.join(CACHE_FILE);
    let meta_path = dir.join(META_FILE);

    let Ok(fingerprint) = fingerprint_data_dir(data_dir) else {
        return;
    };

    let meta = CacheMeta {
        version: CACHE_VERSION,
        fingerprint,
    };

    if let Ok(file) = File::create(&cache_path) {
        let mut writer = BufWriter::new(file);
        if let Err(e) = bincode::serialize_into(&mut writer, bible) {
            eprintln!("Warning: failed to write language cache: {}", e);
            let _ = fs::remove_file(&cache_path);
            return;
        }
        let _ = writer.flush();
    } else {
        return;
    }

    if let Ok(file) = File::create(&meta_path)
        && let Err(e) = serde_json::to_writer_pretty(file, &meta) {
            eprintln!("Warning: failed to write language cache meta: {}", e);
        }
}
