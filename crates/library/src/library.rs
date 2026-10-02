//! The library as the app sees it: the catalogue of translations, and their books,
//! parsed on first use from the embedded archive and kept in a small cache.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::alignment::{Alignment, Ref};
use crate::archive::Archive;
use crate::crossrefs::{self, CrossrefInfo, Line, Target, Xref};
use crate::notes::{self, CommentaryInfo, Note};
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

/// Books' verse lists kept for reuse (cross-references read verses from many books).
const VERSE_LISTS: usize = 96;

pub struct Library {
    archive: Archive,
    bibles: Vec<BibleInfo>,
    parsed: Mutex<Parsed>,
    alignments: Mutex<HashMap<String, Arc<Alignment>>>,
    commentaries: Vec<CommentaryInfo>,
    /// (commentary, book) -> its notes
    notes: Mutex<HashMap<(String, String), Notes>>,
    verse_sets: Mutex<HashMap<(String, String), Arc<VerseSet>>>,
    crossrefs: Vec<CrossrefInfo>,
    /// (collection, book) -> its references
    xrefs: Mutex<HashMap<(String, String), Xrefs>>,
    verse_lists: Mutex<VerseLists>,
}

/// A list's references from one book, shared.
type Xrefs = Arc<Vec<Xref>>;

/// A book's verses, shared.
type VerseList = Arc<Vec<usfm::VerseText>>;

/// (translation, book) -> its verses, and when they were last used.
struct VerseLists {
    lists: HashMap<(String, String), (VerseList, u64)>,
    clock: u64,
}

/// A commentary book's notes, shared.
type Notes = Arc<Vec<Note>>;

/// The (chapter, number) of every verse in a book.
type VerseSet = std::collections::HashSet<(u32, String)>;

struct Parsed {
    books: HashMap<(String, String), (Arc<usfm::Book>, u64)>,
    clock: u64,
}

impl Library {
    pub fn open(bytes: impl Into<Cow<'static, [u8]>>) -> Result<Self, String> {
        let archive = Archive::open(bytes)?;
        let bibles: Vec<BibleInfo> = serde_json::from_str(&archive.get_str("bibles.json")?)
            .map_err(|e| format!("bibles.json: {}", e))?;
        let commentaries: Vec<CommentaryInfo> = if archive.contains("commentaries.json") {
            serde_json::from_str(&archive.get_str("commentaries.json")?).map_err(|e| format!("commentaries.json: {}", e))?
        } else {
            Vec::new()
        };
        let crossrefs: Vec<CrossrefInfo> = if archive.contains("crossrefs.json") {
            serde_json::from_str(&archive.get_str("crossrefs.json")?).map_err(|e| format!("crossrefs.json: {}", e))?
        } else {
            Vec::new()
        };
        Ok(Self {
            archive,
            bibles,
            commentaries,
            crossrefs,
            xrefs: Mutex::new(HashMap::new()),
            verse_lists: Mutex::new(VerseLists { lists: HashMap::new(), clock: 0 }),
            notes: Mutex::new(HashMap::new()),
            parsed: Mutex::new(Parsed { books: HashMap::new(), clock: 0 }),
            alignments: Mutex::new(HashMap::new()),
            verse_sets: Mutex::new(HashMap::new()),
        })
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

    /// How translation `id`'s verses correspond to the KJV's (the KJV's own is identity).
    pub fn alignment(&self, id: &str) -> Result<Arc<Alignment>, String> {
        if let Some(a) = self.alignments.lock().unwrap().get(id) {
            return Ok(a.clone());
        }
        let key = format!("align/{}.tsv", id);
        let a = Arc::new(if id == "kjv" || !self.archive.contains(&key) {
            Alignment::identity()
        } else {
            Alignment::parse(&self.archive.get_str(&key)?)?
        });
        self.alignments.lock().unwrap().insert(id.to_string(), a.clone());
        Ok(a)
    }

    /// Whether translation `id` has verse `r` (a Psalm title is number "0").
    pub fn has_verse(&self, id: &str, r: &Ref) -> bool {
        let key = (id.to_string(), r.0.clone());
        let set = {
            let cached = self.verse_sets.lock().unwrap().get(&key).cloned();
            match cached {
                Some(s) => s,
                None => {
                    let Ok(book) = self.book(id, &r.0) else { return false };
                    let s: Arc<VerseSet> =
                        Arc::new(usfm::verses(&book).into_iter().map(|v| (v.chapter, v.number)).collect());
                    self.verse_sets.lock().unwrap().insert(key, s.clone());
                    s
                }
            }
        };
        set.contains(&(r.1, r.2.clone()))
    }

    /// The verses of translation `to` that correspond to verse `r` of translation
    /// `from`, through the KJV. Empty when `to` has no counterpart.
    pub fn map(&self, from: &str, to: &str, r: &Ref) -> Result<Vec<Ref>, String> {
        let kjv: Vec<Ref> = self.alignment(from)?.to_kjv(r, &|k| self.has_verse("kjv", k));
        if to == "kjv" {
            return Ok(kjv);
        }
        let target = self.alignment(to)?;
        let mut out: Vec<Ref> = kjv.iter().flat_map(|k| target.from_kjv(k, &|x| self.has_verse(to, x))).collect();
        // The same book first (Brenton prints Nehemiah both as Nehemiah and as Ezra
        // 11-23), then in the app's order, verses by number ("9" before "10")
        let number = |v: &str| v.split('-').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0);
        out.sort_by_key(|x| (x.0 != r.0, crate::books::order(&x.0), x.1, number(&x.2), x.2.clone()));
        out.dedup();
        Ok(out)
    }

    /// Every commentary, in the order the app lists them.
    pub fn commentaries(&self) -> &[CommentaryInfo] {
        &self.commentaries
    }

    /// Every note of commentary `id` on book `code` (KJV numbering), in order.
    pub fn commentary_book(&self, id: &str, code: &str) -> Result<Arc<Vec<Note>>, String> {
        let key = (id.to_string(), code.to_string());
        if let Some(n) = self.notes.lock().unwrap().get(&key) {
            return Ok(n.clone());
        }
        let info = self.commentaries.iter().find(|c| c.id == id).ok_or_else(|| format!("no commentary {:?}", id))?;
        let parsed = if info.books.iter().any(|b| b == code) {
            notes::parse(&self.archive.get_str(&format!("comm/{}/{}.jsonl", id, code))?).map_err(|e| format!("{} {}: {}", id, code, e))?
        } else {
            Vec::new()
        };
        let parsed = Arc::new(parsed);
        let mut cache = self.notes.lock().unwrap();
        // Commentary books are large (Gill's Psalms is megabytes): keep a few
        if cache.len() >= 16 {
            cache.clear();
        }
        cache.insert(key, parsed.clone());
        Ok(parsed)
    }

    /// The notes of commentary `id` covering KJV verse `chapter`:`verse` of `code`
    /// (verse 0: the chapter's introduction).
    pub fn notes_on(&self, id: &str, code: &str, chapter: u32, verse: u32) -> Result<Vec<Note>, String> {
        Ok(self.commentary_book(id, code)?.iter().filter(|n| n.covers(chapter, verse)).cloned().collect())
    }

    /// Every verse of book `code` in translation `id`, in order, as plain text.
    pub fn verses(&self, id: &str, code: &str) -> Result<VerseList, String> {
        let key = (id.to_string(), code.to_string());
        {
            let mut cache = self.verse_lists.lock().unwrap();
            cache.clock += 1;
            let now = cache.clock;
            if let Some((list, used)) = cache.lists.get_mut(&key) {
                *used = now;
                return Ok(list.clone());
            }
        }
        let book = self.book(id, code)?;
        let list = Arc::new(usfm::verses(&book));
        let mut cache = self.verse_lists.lock().unwrap();
        let now = cache.clock;
        cache.lists.insert(key, (list.clone(), now));
        if cache.lists.len() > VERSE_LISTS
            && let Some(oldest) = cache.lists.iter().min_by_key(|(_, (_, used))| *used).map(|(k, _)| k.clone())
        {
            cache.lists.remove(&oldest);
        }
        Ok(list)
    }

    /// Every cross-reference collection, in the order the app lists them.
    pub fn crossrefs(&self) -> &[CrossrefInfo] {
        &self.crossrefs
    }

    /// Every reference of list `id` from book `code` (KJV numbering), in order.
    pub fn crossref_book(&self, id: &str, code: &str) -> Result<Xrefs, String> {
        let key = (id.to_string(), code.to_string());
        if let Some(x) = self.xrefs.lock().unwrap().get(&key) {
            return Ok(x.clone());
        }
        let info = self.crossrefs.iter().find(|c| c.id == id).ok_or_else(|| format!("no cross-references {:?}", id))?;
        let parsed = if info.books.iter().any(|b| b == code) {
            crossrefs::parse(&self.archive.get_str(&format!("xref/{}/{}.tsv", id, code))?).map_err(|e| format!("{} {}: {}", id, code, e))?
        } else {
            Vec::new()
        };
        let parsed = Arc::new(parsed);
        let mut cache = self.xrefs.lock().unwrap();
        if cache.len() >= 16 {
            cache.clear();
        }
        cache.insert(key, parsed.clone());
        Ok(parsed)
    }

    /// The references of collection `id` from KJV verse `chapter`:`verse` of `code`, as
    /// lines (see [`crossrefs::lines`]); a list's are one line, most helpful first.
    pub fn crossrefs_from(&self, id: &str, code: &str, chapter: u32, verse: u32) -> Result<Vec<Line>, String> {
        let info = self.crossrefs.iter().find(|c| c.id == id).ok_or_else(|| format!("no cross-references {:?}", id))?;
        if let Some(commentary) = &info.commentary {
            return Ok(self.notes_on(commentary, code, chapter, verse)?.iter().flat_map(|n| crossrefs::lines(&n.body)).collect());
        }
        let refs: Vec<Target> = self
            .crossref_book(id, code)?
            .iter()
            .filter(|x| x.from == (chapter, verse))
            .map(|x| Target { to: x.to.clone(), votes: Some(x.votes) })
            .collect();
        Ok(if refs.is_empty() { Vec::new() } else { vec![Line { text: String::new(), refs }] })
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
    struct CommentaryCatalogue {
        commentary: Vec<CommentaryEntry>,
    }

    #[derive(Deserialize)]
    struct CommentaryEntry {
        id: String,
        name: String,
        #[serde(default)]
        short: Option<String>,
        author: String,
        year: String,
        tradition: String,
        coverage: String,
        licence: String,
        credit: String,
        about: String,
    }

    #[derive(Deserialize)]
    struct CrossrefCatalogue {
        crossrefs: Vec<CrossrefEntry>,
    }

    #[derive(Deserialize)]
    struct CrossrefEntry {
        id: String,
        name: String,
        short: String,
        licence: String,
        credit: String,
        about: String,
        #[serde(default)]
        commentary: Option<String>,
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
        // Commentaries: the catalogue, and one entry per commentary and book
        let path = lib.join("commentaries.toml");
        if path.exists() {
            let cat: CommentaryCatalogue = toml::from_str(&read_text(&path, &mut read)?).map_err(|e| format!("commentaries.toml: {}", e))?;
            let mut infos = Vec::new();
            for c in cat.commentary {
                let dir = lib.join("commentaries").join(&c.id);
                let mut codes: Vec<String> = fs::read_dir(&dir)
                    .map_err(|e| format!("{}: {}", dir.display(), e))?
                    .flatten()
                    .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".jsonl").map(str::to_string))
                    .collect();
                codes.sort_by_key(|code| books::order(code));
                for code in &codes {
                    let text = read_text(&dir.join(format!("{}.jsonl", code)), &mut read)?;
                    entries.push((format!("comm/{}/{}.jsonl", c.id, code), text));
                }
                infos.push(crate::notes::CommentaryInfo {
                    id: c.id,
                    name: c.name,
                    short: c.short,
                    author: c.author,
                    year: c.year,
                    tradition: c.tradition,
                    coverage: c.coverage,
                    licence: c.licence,
                    credit: c.credit,
                    about: c.about,
                    books: codes,
                });
            }
            entries.push(("commentaries.json".into(), serde_json::to_string(&infos).map_err(|e| e.to_string())?));
        }
        // Cross-references: the catalogue, and one entry per list and book (the Treasury's
        // are its commentary notes, packed above)
        let path = lib.join("crossrefs.toml");
        if path.exists() {
            let cat: CrossrefCatalogue = toml::from_str(&read_text(&path, &mut read)?).map_err(|e| format!("crossrefs.toml: {}", e))?;
            let mut infos = Vec::new();
            for c in cat.crossrefs {
                let codes: Vec<String> = match &c.commentary {
                    Some(comm) => {
                        let key = format!("comm/{}/", comm);
                        entries.iter().filter_map(|(k, _)| k.strip_prefix(&key)?.strip_suffix(".jsonl").map(str::to_string)).collect()
                    }
                    None => {
                        let dir = lib.join("crossrefs").join(&c.id);
                        let mut codes: Vec<String> = fs::read_dir(&dir)
                            .map_err(|e| format!("{}: {}", dir.display(), e))?
                            .flatten()
                            .filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".tsv").map(str::to_string))
                            .collect();
                        codes.sort_by_key(|code| books::order(code));
                        for code in &codes {
                            let text = read_text(&dir.join(format!("{}.tsv", code)), &mut read)?;
                            entries.push((format!("xref/{}/{}.tsv", c.id, code), text));
                        }
                        codes
                    }
                };
                infos.push(crate::crossrefs::CrossrefInfo {
                    id: c.id,
                    name: c.name,
                    short: c.short,
                    licence: c.licence,
                    credit: c.credit,
                    about: c.about,
                    commentary: c.commentary,
                    books: codes,
                });
            }
            entries.push(("crossrefs.json".into(), serde_json::to_string(&infos).map_err(|e| e.to_string())?));
        }
        // Verse alignment tables, where made (data/library/alignment/<id>.tsv)
        for entry in &catalogue.bible {
            let path = lib.join("alignment").join(format!("{}.tsv", entry.id));
            if path.exists() {
                entries.push((format!("align/{}.tsv", entry.id), read_text(&path, &mut read)?));
            }
        }

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
