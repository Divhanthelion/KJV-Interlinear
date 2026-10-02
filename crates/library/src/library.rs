//! The library as the app sees it: the catalogue of translations, and their books,
//! parsed on first use from the embedded archive and kept in a small cache.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::archive::Archive;
use crate::usfm::{self, Options};
use crate::view::{self, ChapterView};

/// One translation, as listed in the archive's `bibles.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BibleInfo {
    pub id: String,
    pub abbr: String,
    pub name: String,
    pub year: String,
    pub group: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    #[serde(default)]
    pub heading_markers: Vec<String>,
    /// The books it has, in the app's order
    pub books: Vec<BookEntry>,
    pub chapters: usize,
    pub verses: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookEntry {
    /// USFM code ("1SA")
    pub code: String,
    /// The translation's own name for it ("Kings I" in Brenton)
    pub name: String,
    pub chapters: usize,
    /// The chapter numbers, in order: usually 1..=chapters, but not always (Greek
    /// Esther starting at 10, a translation of selected chapters)
    pub numbers: Vec<u32>,
    pub verses: usize,
}

/// Parsed books kept for reuse.
const PARSED_BOOKS: usize = 64;

pub struct Library {
    archive: Archive,
    bibles: Vec<BibleInfo>,
    parsed: Mutex<Parsed>,
}

struct Parsed {
    books: HashMap<(String, String), (Arc<usfm::Book>, u64)>,
    clock: u64,
}

impl Library {
    pub fn open(bytes: impl Into<Cow<'static, [u8]>>) -> Result<Self, String> {
        let archive = Archive::open(bytes)?;
        let bibles: Vec<BibleInfo> = serde_json::from_str(&archive.get_str("bibles.json")?)
            .map_err(|e| format!("bibles.json: {}", e))?;
        Ok(Self { archive, bibles, parsed: Mutex::new(Parsed { books: HashMap::new(), clock: 0 }) })
    }

    /// Every translation, in the order the app lists them.
    pub fn bibles(&self) -> &[BibleInfo] {
        &self.bibles
    }

    pub fn bible(&self, id: &str) -> Option<&BibleInfo> {
        self.bibles.iter().find(|b| b.id == id)
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    /// Book `code` of translation `id`, parsed.
    pub fn book(&self, id: &str, code: &str) -> Result<Arc<usfm::Book>, String> {
        let info = self.bible(id).ok_or_else(|| format!("no translation {:?}", id))?;
        if !info.books.iter().any(|b| b.code == code) {
            return Err(format!("{} has no book {}", info.abbr, code));
        }
        let key = (id.to_string(), code.to_string());
        {
            let mut p = self.parsed.lock().unwrap();
            p.clock += 1;
            let now = p.clock;
            if let Some((book, used)) = p.books.get_mut(&key) {
                *used = now;
                return Ok(book.clone());
            }
        }
        let src = self.archive.get_str(&format!("bible/{}/{}.usfm", id, code))?;
        let options = Options { heading_markers: info.heading_markers.clone() };
        let book = Arc::new(usfm::parse(&src, &options).map_err(|e| format!("{} {}: {}", id, code, e))?);
        let mut p = self.parsed.lock().unwrap();
        let now = p.clock;
        p.books.insert(key, (book.clone(), now));
        if p.books.len() > PARSED_BOOKS
            && let Some(oldest) = p.books.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone())
        {
            p.books.remove(&oldest);
        }
        Ok(book)
    }

    /// Chapter `chapter` of book `code` in translation `id`, ready to draw.
    pub fn chapter(&self, id: &str, code: &str, chapter: u32) -> Result<ChapterView, String> {
        let book = self.book(id, code)?;
        view::chapter(&book, chapter).ok_or_else(|| format!("{} {} has no chapter {}", id, code, chapter))
    }
}

/// Packing `data/library/` into an archive, for build scripts.
#[cfg(feature = "build")]
pub mod build {
    use std::fs;
    use std::path::Path;

    use serde::Deserialize;

    use super::{BibleInfo, BookEntry};
    use crate::archive::Writer;
    use crate::books;

    #[derive(Deserialize)]
    struct Catalogue {
        bible: Vec<CatalogueEntry>,
    }

    #[derive(Deserialize)]
    struct CatalogueEntry {
        id: String,
    }

    #[derive(Deserialize)]
    struct Index {
        id: String,
        abbr: String,
        name: String,
        year: String,
        group: String,
        licence: String,
        credit: String,
        about: String,
        #[serde(default)]
        heading_markers: Vec<String>,
        chapters: usize,
        verses: usize,
        #[serde(rename = "book")]
        books: Vec<IndexBook>,
    }

    #[derive(Deserialize)]
    struct IndexBook {
        code: String,
        name: String,
        chapters: usize,
        verses: usize,
        #[serde(default)]
        chapter_numbers: Option<Vec<u32>>,
    }

    /// The archive for `root/data/library`, compressing each entry with `compress`.
    /// Every file it reads is returned too, for `cargo:rerun-if-changed`.
    pub fn archive(root: &Path, compress: &(dyn Fn(&[u8]) -> Vec<u8> + Sync)) -> Result<(Vec<u8>, Vec<std::path::PathBuf>), String> {
        let lib = root.join("data/library");
        let mut read = Vec::new();
        let read_text = |path: &Path, read: &mut Vec<std::path::PathBuf>| -> Result<String, String> {
            read.push(path.to_path_buf());
            fs::read_to_string(path).map(|t| t.replace("\r\n", "\n")).map_err(|e| format!("{}: {}", path.display(), e))
        };
        let catalogue: Catalogue = toml::from_str(&read_text(&lib.join("bibles.toml"), &mut read)?).map_err(|e| format!("bibles.toml: {}", e))?;

        // (key, text) for every entry, compressed in parallel at the end
        let mut entries: Vec<(String, String)> = Vec::new();
        let mut bibles = Vec::new();
        for entry in &catalogue.bible {
            let dir = lib.join("bibles").join(&entry.id);
            let index: Index = toml::from_str(&read_text(&dir.join("index.toml"), &mut read)?)
                .map_err(|e| format!("{}/index.toml: {}", entry.id, e))?;
            if index.id != entry.id {
                return Err(format!("{}/index.toml has id {:?}", entry.id, index.id));
            }
            let mut index_books = index.books;
            for b in &index_books {
                if books::by_code(&b.code).is_none() {
                    return Err(format!("{}: unknown book {}", entry.id, b.code));
                }
            }
            index_books.sort_by_key(|b| books::order(&b.code));
            for b in &index_books {
                let text = read_text(&dir.join(format!("{}.usfm", b.code)), &mut read)?;
                entries.push((format!("bible/{}/{}.usfm", entry.id, b.code), text));
            }
            bibles.push(BibleInfo {
                id: index.id,
                abbr: index.abbr,
                name: index.name,
                year: index.year,
                group: index.group,
                licence: index.licence,
                credit: index.credit,
                about: index.about,
                heading_markers: index.heading_markers,
                books: index_books
                    .into_iter()
                    .map(|b| BookEntry {
                        numbers: b.chapter_numbers.unwrap_or_else(|| (1..=b.chapters as u32).collect()),
                        code: b.code,
                        name: b.name,
                        chapters: b.chapters,
                        verses: b.verses,
                    })
                    .collect(),
                chapters: index.chapters,
                verses: index.verses,
            });
        }
        entries.push(("bibles.json".into(), serde_json::to_string(&bibles).map_err(|e| e.to_string())?));

        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done: std::sync::Mutex<Vec<(usize, Vec<u8>)>> = std::sync::Mutex::new(Vec::with_capacity(entries.len()));
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((_, text)) = entries.get(i) else { break };
                        let frame = compress(text.as_bytes());
                        done.lock().unwrap().push((i, frame));
                    }
                });
            }
        });
        let mut frames = done.into_inner().unwrap();
        frames.sort_by_key(|(i, _)| *i);
        let mut w = Writer::new();
        for ((key, text), (_, frame)) in entries.iter().zip(frames) {
            w.add(key, frame, text.len());
        }
        Ok((w.finish(), read))
    }
}
