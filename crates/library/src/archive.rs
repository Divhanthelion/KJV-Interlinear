//! The library archive the app embeds: a table of contents, then one zstd frame per
//! entry ("bible/web/JHN.usfm", "bible/web/index.json", …). Nothing is decompressed
//! until it is read, and recently read entries stay in a small cache, so memory use
//! stays low however many works ship.
//!
//! Layout: `KJVLIB01`, a u32 (little-endian) table-of-contents length, the table of
//! contents (UTF-8 lines `key\toffset\tlength\tsize`, offsets from the end of the
//! table), then the frames.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

const MAGIC: &[u8; 8] = b"KJVLIB01";

/// Builds an archive from already-compressed entries (the build script compresses).
#[derive(Default)]
pub struct Writer {
    entries: Vec<(String, Vec<u8>, usize)>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    /// `compressed` is one zstd frame of `size` bytes once decompressed.
    pub fn add(&mut self, key: &str, compressed: Vec<u8>, size: usize) {
        assert!(!key.contains(['\t', '\n']), "archive key {:?}", key);
        self.entries.push((key.to_string(), compressed, size));
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        let mut toc = String::new();
        let mut offset = 0usize;
        for (key, data, size) in &self.entries {
            toc.push_str(&format!("{}\t{}\t{}\t{}\n", key, offset, data.len(), size));
            offset += data.len();
        }
        let mut out = Vec::with_capacity(16 + toc.len() + offset);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(toc.len() as u32).to_le_bytes());
        out.extend_from_slice(toc.as_bytes());
        for (_, data, _) in &self.entries {
            out.extend_from_slice(data);
        }
        out
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    offset: usize,
    length: usize,
    size: usize,
}

pub struct Archive {
    bytes: Cow<'static, [u8]>,
    data_start: usize,
    entries: HashMap<String, Entry>,
    keys: Vec<String>,
    cache: Mutex<Cache>,
}

struct Cache {
    items: HashMap<String, (Arc<Vec<u8>>, u64)>,
    bytes: usize,
    clock: u64,
    limit: usize,
}

/// Decompressed entries kept for reuse, least recently used dropped first.
pub const DEFAULT_CACHE_BYTES: usize = 48 * 1024 * 1024;

impl Archive {
    pub fn open(bytes: impl Into<Cow<'static, [u8]>>) -> Result<Self, String> {
        let bytes = bytes.into();
        if bytes.len() < 12 || &bytes[..8] != MAGIC {
            return Err("not a library archive".into());
        }
        let toc_len = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let toc = std::str::from_utf8(bytes.get(12..12 + toc_len).ok_or("archive table of contents is cut short")?)
            .map_err(|_| "archive table of contents is not UTF-8")?;
        let data_start = 12 + toc_len;
        let mut entries = HashMap::new();
        let mut keys = Vec::new();
        for line in toc.lines() {
            let mut f = line.split('\t');
            let (Some(key), Some(o), Some(l), Some(s), None) = (f.next(), f.next(), f.next(), f.next(), f.next()) else {
                return Err(format!("bad archive entry {:?}", line));
            };
            let parse = |v: &str| v.parse::<usize>().map_err(|_| format!("bad archive entry {:?}", line));
            let e = Entry { offset: parse(o)?, length: parse(l)?, size: parse(s)? };
            if data_start + e.offset + e.length > bytes.len() {
                return Err(format!("archive entry {} runs past the end", key));
            }
            keys.push(key.to_string());
            entries.insert(key.to_string(), e);
        }
        Ok(Self {
            bytes,
            data_start,
            entries,
            keys,
            cache: Mutex::new(Cache { items: HashMap::new(), bytes: 0, clock: 0, limit: DEFAULT_CACHE_BYTES }),
        })
    }

    /// Every key, sorted.
    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    pub fn contains(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// An entry's bytes, decompressed (from the cache when recently read).
    pub fn get(&self, key: &str) -> Result<Arc<Vec<u8>>, String> {
        let e = *self.entries.get(key).ok_or_else(|| format!("{} is not in the library", key))?;
        {
            let mut c = self.cache.lock().unwrap();
            c.clock += 1;
            let now = c.clock;
            if let Some((data, used)) = c.items.get_mut(key) {
                *used = now;
                return Ok(data.clone());
            }
        }
        let frame = &self.bytes[self.data_start + e.offset..self.data_start + e.offset + e.length];
        let mut out = Vec::with_capacity(e.size);
        ruzstd::decoding::StreamingDecoder::new(frame)
            .map_err(|err| format!("{}: {}", key, err))?
            .read_to_end(&mut out)
            .map_err(|err| format!("{}: {}", key, err))?;
        if out.len() != e.size {
            return Err(format!("{} decompressed to {} bytes, expected {}", key, out.len(), e.size));
        }
        let data = Arc::new(out);
        let mut c = self.cache.lock().unwrap();
        let now = c.clock;
        c.bytes += data.len();
        c.items.insert(key.to_string(), (data.clone(), now));
        while c.bytes > c.limit && c.items.len() > 1 {
            let oldest = c.items.iter().filter(|(k, _)| k.as_str() != key).min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    let (d, _) = c.items.remove(&k).unwrap();
                    c.bytes -= d.len();
                }
                None => break,
            }
        }
        Ok(data)
    }

    /// An entry as text.
    pub fn get_str(&self, key: &str) -> Result<String, String> {
        let bytes = self.get(key)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| format!("{} is not UTF-8", key))
    }

    /// Bytes held in the cache now (for tests and diagnostics).
    pub fn cached_bytes(&self) -> usize {
        self.cache.lock().unwrap().bytes
    }

    pub fn set_cache_limit(&self, bytes: usize) {
        self.cache.lock().unwrap().limit = bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(text: &str) -> Vec<u8> {
        zstd::encode_all(text.as_bytes(), 3).unwrap()
    }

    #[test]
    fn round_trip_and_cache() {
        let mut w = Writer::new();
        w.add("bible/web/JHN.usfm", frame("\\id JHN\n\\c 1\n"), 13);
        w.add("bible/web/GEN.usfm", frame(&"x".repeat(1000)), 1000);
        let a = Archive::open(w.finish()).unwrap();
        assert_eq!(a.keys(), ["bible/web/GEN.usfm", "bible/web/JHN.usfm"]);
        assert_eq!(a.get_str("bible/web/JHN.usfm").unwrap(), "\\id JHN\n\\c 1\n");
        assert!(a.get("bible/web/MAT.usfm").unwrap_err().contains("not in the library"));
        a.set_cache_limit(500);
        a.get("bible/web/GEN.usfm").unwrap();
        a.get("bible/web/JHN.usfm").unwrap();
        // GEN (1000 bytes) went over the limit and was dropped for JHN
        assert_eq!(a.cached_bytes(), 13);
        assert!(Archive::open(b"nonsense".to_vec()).is_err());
    }
}
