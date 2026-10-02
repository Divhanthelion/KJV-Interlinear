//! Pinned upstream sources: `data/library/sources.toml`.

use std::fs;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{bibles, cache, library};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    /// Path under `.cache/sources/`, e.g. "ebible/engwebp_usfm.zip"
    pub path: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// When it was downloaded (YYYY-MM-DD)
    pub retrieved: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "source")]
    sources: Vec<Source>,
}

const HEADER: &str = "\
# Every upstream file the library is built from, pinned by size and SHA-256.
# `kjv-import fetch` downloads them into .cache/sources/ and refuses any file whose
# hash differs; `kjv-import pin` re-records them after an upstream change has been
# reviewed. See docs/LIBRARY.md.

";

fn path() -> std::path::PathBuf {
    library().join("sources.toml")
}

pub fn load() -> Result<Vec<Source>, String> {
    let text = fs::read_to_string(path()).map_err(|e| format!("{}: {}", path().display(), e))?;
    let file: File = toml::from_str(&text).map_err(|e| format!("{}: {}", path().display(), e))?;
    Ok(file.sources)
}

fn save(sources: &[Source]) -> Result<(), String> {
    let body = toml::to_string(&File { sources: sources.to_vec() }).map_err(|e| e.to_string())?;
    fs::write(path(), format!("{}{}", HEADER, body)).map_err(|e| e.to_string())
}

/// Every source the catalogues need.
fn wanted() -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for b in bibles::catalogue()? {
        for kind in ["usfm", "vpl"] {
            let name = format!("{}_{}.zip", b.ebible, kind);
            out.push((format!("ebible/{}", name), format!("https://ebible.org/Scriptures/{}", name)));
        }
    }
    Ok(out)
}

pub fn sha256(path: &Path) -> Result<(u64, String), String> {
    let mut f = fs::File::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        hasher.update(&buf[..n]);
    }
    Ok((size, hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect()))
}

fn today() -> String {
    // Days since 1970 to a civil date (no time zone needed for a retrieval date)
    let days = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 / 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Record the hashes of every cached source the catalogues need.
pub fn pin() -> Result<(), String> {
    let old = load().unwrap_or_default();
    let mut out = Vec::new();
    for (rel, url) in wanted()? {
        let file = cache().join(&rel);
        if !file.exists() {
            return Err(format!("{} is not downloaded; run `kjv-import fetch` first", rel));
        }
        let (size, sha256) = sha256(&file)?;
        let retrieved = match old.iter().find(|s| s.path == rel && s.sha256 == sha256) {
            Some(s) => s.retrieved.clone(),
            None => today(),
        };
        out.push(Source { path: rel, url, size, sha256, retrieved });
    }
    save(&out)?;
    println!("pinned {} sources", out.len());
    Ok(())
}

/// Download whatever is missing, then verify everything.
pub fn fetch() -> Result<(), String> {
    let pinned = load().unwrap_or_default();
    let client = reqwest::blocking::Client::builder()
        .user_agent("kjv-import (https://github.com/Divhanthelion/KJV-Interlinear)")
        .build()
        .map_err(|e| e.to_string())?;
    for (rel, url) in wanted()? {
        let file = cache().join(&rel);
        if file.exists() {
            continue;
        }
        fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        println!("downloading {}", url);
        let bytes = client.get(&url).send().and_then(|r| r.error_for_status()).and_then(|r| r.bytes()).map_err(|e| format!("{}: {}", url, e))?;
        let tmp = file.with_extension("part");
        fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
        fs::rename(&tmp, &file).map_err(|e| e.to_string())?;
        if !pinned.iter().any(|s| s.path == rel) {
            println!("  {} is new: review it, then run `kjv-import pin`", rel);
        }
    }
    verify()
}

/// Every pinned source is present and unchanged.
pub fn verify() -> Result<(), String> {
    let pinned = load()?;
    let mut problems = Vec::new();
    for s in &pinned {
        let file = cache().join(&s.path);
        match sha256(&file) {
            Ok((size, sha)) if size == s.size && sha == s.sha256 => {}
            Ok((size, sha)) => problems.push(format!(
                "{} changed upstream or locally: {} bytes {} (pinned {} bytes {})",
                s.path, size, &sha[..16], s.size, &s.sha256[..16]
            )),
            Err(_) => problems.push(format!("{} is missing; run `kjv-import fetch`", s.path)),
        }
    }
    for (rel, _) in wanted()? {
        if !pinned.iter().any(|s| s.path == rel) {
            problems.push(format!("{} is not pinned; run `kjv-import pin` after reviewing it", rel));
        }
    }
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}
