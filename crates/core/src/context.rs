//! Context for the study assistant: the passages the reader chose, each in the chosen
//! translations, with the chosen commentaries' notes on it, its cross-references, and
//! the KJV's Hebrew and Greek, written as text a language model reads, with a size
//! estimate so the app can tell whether it fits the model's context window.
//!
//! A passage is a list of ranges in one translation's numbering, the translation it
//! was chosen in: "Psalm 22" chosen while reading the Douay-Rheims is the
//! Douay-Rheims' Psalm 22, the KJV's 23. Every other translation's text is found
//! verse by verse through the verse alignment, and the commentaries and
//! cross-references, keyed to the KJV, through the KJV's verses.
//!
//! The text is plain lines inside a few XML-style elements:
//!
//! ```text
//! <context>
//! <passage ref="Luke 2:14">
//! <bible translation="King James Version" abbr="KJV" year="1769" ref="Luke 2:14">
//! ## Luke 2
//! 14 Glory to God in the highest, and on earth peace, good will toward men.
//! </bible>
//! <commentary name="…" author="…" year="…">
//! <note on="Luke 2:8-20">
//! …
//! </note>
//! </commentary>
//! <crossrefs name="…" numbering="KJV">
//! Luke 2:14
//! - Isaiah 9:6: For unto us a child is born, …
//! </crossrefs>
//! </passage>
//! </context>
//! ```

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use kjv_library::Library;
use kjv_library::alignment::Ref;
use kjv_library::books::{self, Section};
use kjv_library::notes::text as note_text;
use kjv_library::reference::{self, Range};
use kjv_library::usfm::VerseText;
use serde::{Deserialize, Serialize};

use crate::api::strongs_display;
use crate::bundle::DataBundle;
use crate::models::OriginalLanguage;
use crate::text::format_gloss;
use crate::translations::{self, note_label};

fn kjv_id() -> String {
    "kjv".to_string()
}

/// What to attach to a question.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Spec {
    pub passages: Vec<PassageSpec>,
    /// Translation ids, in the order given ("kjv", "web"); the KJV when empty
    pub translations: Vec<String>,
    /// Commentary ids, in the order given
    pub commentaries: Vec<String>,
    /// Cross-reference collection ids
    pub crossrefs: Vec<String>,
    /// At most this many places from each verse in a list (OpenBible.info's), or from
    /// each of a verse's words in the Treasury (0: every one)
    pub crossref_limit: usize,
    /// Give each cross-reference's words (in the passage's first translation)
    pub crossref_text: bool,
    /// Each KJV verse's Hebrew, Aramaic, or Greek words with Strong's numbers
    pub original: bool,
    /// With `original`, the full lexicon entry for each Strong's number, once each
    pub definitions: bool,
}

/// One passage, and what to give with it where it differs from the rest.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassageSpec {
    /// The translation whose numbering `refs` follows
    #[serde(default = "kjv_id")]
    pub bible: String,
    /// Ranges as `Range::osis` writes them, separated by spaces: "LUK.2.14",
    /// "ROM.5.1-ROM.5.2", "MIC.6", "GEN EXO LEV"
    pub refs: String,
    #[serde(default)]
    pub translations: Option<Vec<String>>,
    #[serde(default)]
    pub commentaries: Option<Vec<String>>,
    #[serde(default)]
    pub crossrefs: Option<Vec<String>>,
    #[serde(default)]
    pub original: Option<bool>,
}

/// The context's text and what it holds.
#[derive(Debug, Default)]
pub struct Built {
    /// "Luke 2:14; Romans 5:1–2; Micah 6"
    pub label: String,
    pub text: String,
    /// Verses in the passages (as their own translations number them)
    pub verses: usize,
    pub tokens: usize,
    /// The text stopped early at the size limit asked for: `tokens` is a lower bound
    pub capped: bool,
    pub passages: Vec<PassageSize>,
    /// What the text holds, for the instructions
    pub attached: Attached,
}

/// A passage's share of the context.
#[derive(Debug, Clone, Serialize)]
pub struct PassageSize {
    /// "Romans 14", "Psalm 22 (DRA numbering)"
    pub label: String,
    pub tokens: usize,
    /// Why some or all of it is missing ("WEB has no Tobit")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    pub parts: Vec<PartSize>,
}

/// One translation's text, one commentary's notes, or one collection's references.
#[derive(Debug, Clone, Serialize)]
pub struct PartSize {
    /// "bible", "original", "commentary", or "crossrefs"
    pub kind: &'static str,
    pub id: String,
    pub tokens: usize,
    /// Nothing there for this passage (a translation without the book, a commentary
    /// without notes on it)
    pub empty: bool,
}

/// The sources the context holds, in the order they first appear.
#[derive(Debug, Default, Clone)]
pub struct Attached {
    pub translations: Vec<String>,
    pub commentaries: Vec<String>,
    pub crossrefs: Vec<String>,
    /// Original-language words under the KJV's verses
    pub original_inline: bool,
    /// Original-language words in an `<original>` element of their own
    pub original_block: bool,
    pub definitions: bool,
    pub crossref_text: bool,
}

/// Sizes without the text, for the app's meter.
#[derive(Debug, Serialize)]
pub struct Size {
    pub label: String,
    pub verses: usize,
    pub tokens: usize,
    /// The instructions sent with it (which describe what it holds)
    pub instructions: usize,
    pub capped: bool,
    pub passages: Vec<PassageSize>,
}

/// The size of the text is only worked out up to this many bytes (about 2.6M
/// tokens, more than any model reads), so a whole library asked for at once is
/// answered quickly.
pub const SIZE_CAP: usize = 10_000_000;

/// Build the context for `spec`. With `cap`, writing stops once the text is longer.
pub fn build(data: &DataBundle, lib: &Library, spec: &Spec, cap: Option<usize>) -> Result<Built, String> {
    let mut b = Builder::new(data, lib);
    b.cap = cap;
    b.spec(spec)?;
    let tokens = estimate_tokens(&b.text);
    let label = b.passages.iter().map(|p| p.label.clone()).collect::<Vec<_>>().join("; ");
    Ok(Built { label, text: b.text, verses: b.verses, tokens, capped: b.capped, passages: b.passages, attached: b.attached })
}

pub fn size(data: &DataBundle, lib: &Library, spec: &Spec) -> Result<Size, String> {
    let c = build(data, lib, spec, Some(SIZE_CAP))?;
    let instructions = estimate_tokens(&instructions(lib, &c));
    Ok(Size { label: c.label, verses: c.verses, tokens: c.tokens, instructions, capped: c.capped, passages: c.passages })
}

/// Tokens for `text`, erring a little high. Measured on Qwen's tokenizer: English
/// KJV runs 3.9 characters per token (the whole Bible is ~1.09M tokens); pointed
/// Hebrew and polytonic Greek cost ~1.8 tokens per character because every vowel
/// point and accent is its own code point. The app corrects this per model from the
/// token counts providers report.
pub fn estimate_tokens(text: &str) -> usize {
    let (mut ascii, mut other) = (0usize, 0usize);
    for c in text.chars() {
        if c.is_ascii() {
            ascii += 1;
        } else {
            other += 1;
        }
    }
    (ascii * 10).div_ceil(38) + (other * 9).div_ceil(5)
}

// ---------------------------------------------------------------- typed references

/// A passage the reader typed, ready to attach.
#[derive(Debug, Serialize, PartialEq)]
pub struct Parsed {
    pub bible: String,
    pub refs: String,
    pub label: String,
}

/// The passages in `text` ("Luke 2:14; Rom 5:1-2; Micah 6"), in the numbering of
/// translation `bible` (the one being read), or of the KJV, or of the first
/// translation that has the book, when `bible` hasn't it. Ranges in one chapter
/// ("Gen 1:1, 3, 5-7") are one passage; each other is its own.
pub fn parse(data: &DataBundle, lib: &Library, text: &str, bible: &str) -> Result<Vec<Parsed>, String> {
    let ranges = reference::parse(text).map_err(|e| match e {
        reference::Error::UnknownBook(s) => {
            format!("No book called “{}”", s.trim_end_matches(|c: char| c.is_ascii_digit() || " :.,;-–".contains(c)).trim())
        }
        reference::Error::Empty => "Type a reference, like Luke 2:14 or Romans 5:1-2".to_string(),
        other => {
            let s = other.to_string();
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(s)
        }
    })?;
    let mut groups: Vec<Vec<Range>> = Vec::new();
    for r in ranges {
        let verses = |x: &Range| !x.is_whole_book() && !x.is_whole_chapters() && x.start.0 == x.end.0;
        match groups.last_mut() {
            Some(g) if g[0].book == r.book && g[0].start.0 == r.start.0 && verses(&g[0]) && verses(&r) => g.push(r),
            _ => groups.push(vec![r]),
        }
    }
    let mut b = Builder::new(data, lib);
    let mut out = Vec::new();
    for g in groups {
        let book = g[0].book;
        let has = |id: &str| lib.bible(id).is_some_and(|i| i.books.iter().any(|e| e.code == book));
        let numbering = if has(bible) {
            bible.to_string()
        } else if has("kjv") {
            "kjv".to_string()
        } else {
            lib.bibles()
                .iter()
                .find(|i| i.books.iter().any(|e| e.code == book))
                .map(|i| i.id.clone())
                .ok_or_else(|| format!("No translation here has {}", display(book)))?
        };
        let verses = b.passage_verses(&numbering, &g)?;
        if verses.is_empty() {
            let abbr = lib.bible(&numbering).map_or(numbering.as_str(), |i| i.abbr.as_str());
            return Err(format!("{} has no {}", abbr, g.iter().map(range_text).collect::<Vec<_>>().join(", ")));
        }
        let label = b.passage_label(&numbering, &verses)?;
        out.push(Parsed { bible: numbering, refs: g.iter().map(Range::osis).collect::<Vec<_>>().join(" "), label });
    }
    Ok(out)
}

/// "Luke 2:99", "Luke 99", "Jude" (a range as typed, for messages)
fn range_text(r: &Range) -> String {
    let d = display(r.book);
    if r.is_whole_book() {
        return d.to_string();
    }
    let head = |c: u32| chapter_heading(r.book, c);
    let ((c, v), (c2, v2)) = (r.start, r.end);
    if r.is_whole_chapters() {
        return if c == c2 { head(c) } else { format!("{}–{}", head(c), c2) };
    }
    let end = if v2 == reference::END { "end".to_string() } else { v2.to_string() };
    match (c == c2, v == v2) {
        (true, true) => format!("{}:{}", head(c), v),
        (true, false) => format!("{}:{}–{}", head(c), v, end),
        _ => format!("{}:{}–{}:{}", head(c), v, c2, end),
    }
}

fn display(code: &str) -> &str {
    books::by_code(code).map_or(code, |b| b.display)
}

/// "Psalm 23", "Romans 14"
fn chapter_heading(code: &str, c: u32) -> String {
    if code == "PSA" { format!("Psalm {}", c) } else { format!("{} {}", display(code), c) }
}

// ---------------------------------------------------------------- building

/// A translation's book: its verses in order, and where each is.
struct BookIndex {
    verses: Arc<Vec<VerseText>>,
    at: HashMap<(u32, String), usize>,
}

/// A note, to know it again: (commentary, book, from, to, hash of its body)
type NoteKey = (String, String, (u32, u32), (u32, u32), u64);

struct Builder<'a> {
    data: &'a DataBundle,
    lib: &'a Library,
    text: String,
    cap: Option<usize>,
    capped: bool,
    verses: usize,
    passages: Vec<PassageSize>,
    attached: Attached,
    books: HashMap<(String, String), Option<Arc<BookIndex>>>,
    /// Strong's numbers in the order they first appear, for the definitions
    strongs: Vec<&'a str>,
    seen_strongs: HashSet<&'a str>,
    /// Notes already given, and the passage they were given with
    notes_given: HashMap<NoteKey, String>,
}

/// "1-2" -> (1, 2), "16" -> (16, 16), "3a" -> (3, 3), "0" (a Psalm title) -> (0, 0)
fn numbers(n: &str) -> Option<(u32, u32)> {
    let lead = |s: &str| s.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok();
    match n.split_once('-') {
        Some((a, b)) => Some((lead(a)?, lead(b)?)),
        None => lead(n).map(|x| (x, x)),
    }
}

/// Whether verse `number` of `chapter` is in `r` (a bridged verse, "1-2", when any
/// verse it holds is; a Psalm title when the range starts before it)
fn in_range(r: &Range, chapter: u32, number: &str) -> bool {
    let Some((lo, hi)) = numbers(number) else { return false };
    (lo..=hi.min(lo + 200)).any(|n| r.start <= (chapter, n) && (chapter, n) <= r.end)
}

/// Text for an XML-style attribute value
fn attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;")
}

impl<'a> Builder<'a> {
    fn new(data: &'a DataBundle, lib: &'a Library) -> Self {
        Builder {
            data,
            lib,
            text: String::new(),
            cap: None,
            capped: false,
            verses: 0,
            passages: Vec::new(),
            attached: Attached::default(),
            books: HashMap::new(),
            strongs: Vec::new(),
            seen_strongs: HashSet::new(),
            notes_given: HashMap::new(),
        }
    }

    /// Whether the text has reached the size limit (and so writing should stop).
    fn full(&mut self) -> bool {
        if let Some(cap) = self.cap
            && self.text.len() > cap
        {
            self.capped = true;
        }
        self.capped
    }

    /// Book `code` of translation `bible`, or None if it hasn't the book.
    fn book(&mut self, bible: &str, code: &str) -> Result<Option<Arc<BookIndex>>, String> {
        let key = (bible.to_string(), code.to_string());
        if let Some(b) = self.books.get(&key) {
            return Ok(b.clone());
        }
        let info = self.lib.bible(bible).ok_or_else(|| format!("no translation {:?}", bible))?;
        let found = if info.books.iter().any(|b| b.code == code) {
            let verses = self.lib.verses(bible, code)?;
            let at = verses.iter().enumerate().map(|(i, v)| ((v.chapter, v.number.clone()), i)).collect();
            Some(Arc::new(BookIndex { verses, at }))
        } else {
            None
        };
        self.books.insert(key, found.clone());
        Ok(found)
    }

    /// The verses of `ranges` in translation `bible`, in order.
    fn passage_verses(&mut self, bible: &str, ranges: &[Range]) -> Result<Vec<Ref>, String> {
        let mut out: Vec<Ref> = Vec::new();
        let mut seen: HashSet<Ref> = HashSet::new();
        for r in ranges {
            let Some(b) = self.book(bible, r.book)? else { continue };
            for v in b.verses.iter() {
                if in_range(r, v.chapter, &v.number) {
                    let x = (r.book.to_string(), v.chapter, v.number.clone());
                    if seen.insert(x.clone()) {
                        out.push(x);
                    }
                }
            }
        }
        Ok(out)
    }

    /// Verses `refs` of translation `from` as translation `to` has them, in its order,
    /// and those it has no counterpart for.
    fn in_translation(&mut self, from: &str, refs: &[Ref], to: &str) -> Result<(Vec<Ref>, Vec<Ref>), String> {
        if from == to {
            return Ok((refs.to_vec(), Vec::new()));
        }
        let mut found: Vec<Ref> = Vec::new();
        let mut seen: HashSet<Ref> = HashSet::new();
        let mut missing = Vec::new();
        for r in refs {
            let m = self.lib.map(from, to, r)?;
            if m.is_empty() {
                missing.push(r.clone());
            }
            for x in m {
                if seen.insert(x.clone()) {
                    found.push(x);
                }
            }
        }
        let mut keyed = Vec::with_capacity(found.len());
        for x in found {
            let at = self.book(to, &x.0)?.and_then(|b| b.at.get(&(x.1, x.2.clone())).copied()).unwrap_or(usize::MAX);
            keyed.push(((books::order(&x.0).unwrap_or(usize::MAX), at), x));
        }
        keyed.sort_by_key(|(k, _)| *k);
        Ok((keyed.into_iter().map(|(_, x)| x).collect(), missing))
    }

    /// "Romans 14", "Romans 14:1–23; 16:25–27", "Matthew 5–7", "Genesis 1:1, 3, 5–7",
    /// "Jude", "Genesis–Deuteronomy", "The whole Bible": `refs` of translation `bible`.
    fn refs_label(&mut self, bible: &str, refs: &[Ref]) -> Result<String, String> {
        let mut parts: Vec<String> = Vec::new();
        let mut whole: Vec<&'static str> = Vec::new();
        let mut only_whole = true;
        let mut i = 0;
        while i < refs.len() {
            let code = refs[i].0.clone();
            let mut j = i;
            while j < refs.len() && refs[j].0 == code {
                j += 1;
            }
            let group = &refs[i..j];
            i = j;
            let Some(b) = self.book(bible, &code)? else { continue };
            let Some(k) = books::by_code(&code) else { continue };
            let mut at: Vec<usize> = group.iter().filter_map(|r| b.at.get(&(r.1, r.2.clone())).copied()).collect();
            at.sort_unstable();
            at.dedup();
            if at.len() == b.verses.len() {
                whole.push(k.code);
                parts.push(k.display.to_string());
                continue;
            }
            only_whole = false;
            let mut runs: Vec<(usize, usize)> = Vec::new();
            for &x in &at {
                match runs.last_mut() {
                    Some(run) if run.1 + 1 == x => run.1 = x,
                    _ => runs.push((x, x)),
                }
            }
            let vs = &b.verses;
            let number = |v: &VerseText| if v.title { "title".to_string() } else { v.number.clone() };
            let mut text = String::new();
            let mut last_chapter: Option<u32> = None;
            for (a, z) in runs {
                let (first, last) = (&vs[a], &vs[z]);
                let starts = a == 0 || vs[a - 1].chapter != first.chapter;
                let ends = z + 1 == vs.len() || vs[z + 1].chapter != last.chapter;
                let chapters = starts && ends;
                let piece = if chapters {
                    if first.chapter == last.chapter { first.chapter.to_string() } else { format!("{}–{}", first.chapter, last.chapter) }
                } else if first.chapter != last.chapter {
                    format!("{}:{}–{}:{}", first.chapter, number(first), last.chapter, number(last))
                } else if a == z {
                    format!("{}:{}", first.chapter, number(first))
                } else {
                    format!("{}:{}–{}", first.chapter, number(first), number(last))
                };
                if text.is_empty() {
                    let name = if code == "PSA" && chapters && first.chapter != last.chapter {
                        "Psalms"
                    } else if code == "PSA" {
                        "Psalm"
                    } else {
                        k.display
                    };
                    text = format!("{} {}", name, piece);
                } else if !chapters && first.chapter == last.chapter && last_chapter == Some(first.chapter) {
                    // Another verse of the same chapter: "Genesis 1:1, 3"
                    text.push_str(", ");
                    text.push_str(piece.split_once(':').map_or(piece.as_str(), |(_, v)| v));
                } else {
                    text.push_str("; ");
                    text.push_str(&piece);
                }
                last_chapter = Some(last.chapter);
            }
            parts.push(text);
        }
        if only_whole && !whole.is_empty() {
            let set: HashSet<&str> = whole.iter().copied().collect();
            let section = |s: Section| books::BOOKS.iter().filter(|b| b.section == s).map(|b| b.code).collect::<HashSet<_>>();
            let (old, new) = (section(Section::Old), section(Section::New));
            if set == old.union(&new).copied().collect() {
                return Ok("The whole Bible".to_string());
            }
            if set == old {
                return Ok("The Old Testament".to_string());
            }
            if set == new {
                return Ok("The New Testament".to_string());
            }
            // Runs of books in the translation's order: "Genesis–Deuteronomy"
            let order: Vec<&str> = self.lib.bible(bible).map(|i| i.books.iter().map(|b| b.code.as_str()).collect()).unwrap_or_default();
            let pos = |c: &str| order.iter().position(|x| *x == c).unwrap_or(usize::MAX);
            let mut runs: Vec<(&str, &str)> = Vec::new();
            for c in &whole {
                match runs.last_mut() {
                    Some(run) if pos(run.1) != usize::MAX && pos(run.1) + 1 == pos(c) => run.1 = c,
                    _ => runs.push((c, c)),
                }
            }
            return Ok(runs
                .into_iter()
                .map(|(a, z)| match pos(z).saturating_sub(pos(a)) {
                    0 => display(a).to_string(),
                    1 => format!("{}, {}", display(a), display(z)),
                    _ => format!("{}–{}", display(a), display(z)),
                })
                .collect::<Vec<_>>()
                .join(", "));
        }
        Ok(parts.join("; "))
    }

    /// A passage's label, saying whose numbering it is in where that isn't the KJV's.
    fn passage_label(&mut self, bible: &str, verses: &[Ref]) -> Result<String, String> {
        let label = self.refs_label(bible, verses)?;
        if bible == "kjv" {
            return Ok(label);
        }
        let (kjv, missing) = self.in_translation(bible, verses, "kjv")?;
        if missing.is_empty() && kjv == verses {
            return Ok(label);
        }
        let abbr = self.lib.bible(bible).map_or(bible, |i| i.abbr.as_str());
        Ok(format!("{} ({} numbering)", label, abbr))
    }

    fn spec(&mut self, spec: &Spec) -> Result<(), String> {
        let default_translations = if spec.translations.is_empty() { vec![kjv_id()] } else { spec.translations.clone() };
        for id in default_translations.iter().chain(spec.passages.iter().flat_map(|p| p.translations.iter().flatten())) {
            self.lib.bible(id).ok_or_else(|| format!("no translation {:?}", id))?;
        }
        for id in spec.commentaries.iter().chain(spec.passages.iter().flat_map(|p| p.commentaries.iter().flatten())) {
            if !self.lib.commentaries().iter().any(|c| &c.id == id) {
                return Err(format!("no commentary {:?}", id));
            }
        }
        for id in spec.crossrefs.iter().chain(spec.passages.iter().flat_map(|p| p.crossrefs.iter().flatten())) {
            if !self.lib.crossrefs().iter().any(|c| &c.id == id) {
                return Err(format!("no cross-references {:?}", id));
            }
        }
        if !spec.passages.is_empty() {
            self.text.push_str("<context>\n");
        }
        for p in &spec.passages {
            if self.full() {
                break;
            }
            let translations = p.translations.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| default_translations.clone());
            let commentaries = p.commentaries.clone().unwrap_or_else(|| spec.commentaries.clone());
            let crossrefs = p.crossrefs.clone().unwrap_or_else(|| spec.crossrefs.clone());
            let original = p.original.unwrap_or(spec.original);
            self.passage(p, &translations, &commentaries, &crossrefs, original, spec)?;
        }
        if !spec.passages.is_empty() {
            if spec.original && spec.definitions && !self.strongs.is_empty() && !self.full() {
                self.definitions();
            }
            self.text.push_str("</context>\n");
        }
        Ok(())
    }

    fn passage(&mut self, p: &PassageSpec, translations: &[String], commentaries: &[String], crossrefs: &[String], original: bool, spec: &Spec) -> Result<(), String> {
        let info = self.lib.bible(&p.bible).ok_or_else(|| format!("no translation {:?}", p.bible))?;
        let abbr = info.abbr.clone();
        let start = self.text.len();
        let mut size = PassageSize { label: String::new(), tokens: 0, problem: None, parts: Vec::new() };
        let ranges = match reference::from_osis(&p.refs) {
            Ok(r) => r,
            Err(e) => {
                size.label = p.refs.clone();
                size.problem = Some(format!("Not a reference: {}", e));
                self.passages.push(size);
                return Ok(());
            }
        };
        let verses = self.passage_verses(&p.bible, &ranges)?;
        let lacking: Vec<&Range> = ranges.iter().filter(|r| !info.books.iter().any(|b| b.code == r.book)).collect();
        if !lacking.is_empty() {
            size.problem = Some(format!("{} has no {}", abbr, lacking.iter().map(|r| display(r.book)).collect::<Vec<_>>().join(", ")));
        }
        if verses.is_empty() {
            size.label = ranges.iter().map(range_text).collect::<Vec<_>>().join("; ");
            size.problem.get_or_insert_with(|| format!("{} has no {}", abbr, size.label));
            self.passages.push(size);
            return Ok(());
        }
        size.label = self.passage_label(&p.bible, &verses)?;
        self.verses += verses.len();
        let numbering = if p.bible == "kjv" { String::new() } else { format!(" numbering=\"{}\"", attr(&abbr)) };
        self.text.push_str(&format!("<passage ref=\"{}\"{}>\n", attr(&size.label), numbering));

        // Its verses as the KJV numbers them, which the commentaries, cross-references,
        // and original-language words follow
        let (kjv, _) = self.in_translation(&p.bible, &verses, "kjv")?;
        let inline_original = original && translations.iter().any(|t| t == "kjv");
        for t in translations {
            if self.full() {
                break;
            }
            let before = self.text.len();
            let empty = self.bible(t, &p.bible, &verses, inline_original)?;
            size.parts.push(PartSize { kind: "bible", id: t.clone(), tokens: estimate_tokens(&self.text[before..]), empty });
        }
        if original && !inline_original && !self.full() {
            let before = self.text.len();
            let empty = self.original_block(&kjv)?;
            size.parts.push(PartSize { kind: "original", id: "kjv".into(), tokens: estimate_tokens(&self.text[before..]), empty });
        }
        let places = self.places(&kjv)?;
        for c in commentaries {
            if self.full() {
                break;
            }
            let before = self.text.len();
            let empty = self.commentary(c, &places, &size.label)?;
            size.parts.push(PartSize { kind: "commentary", id: c.clone(), tokens: estimate_tokens(&self.text[before..]), empty });
        }
        let words_from = translations.first().cloned().unwrap_or_else(kjv_id);
        for x in crossrefs {
            if self.full() {
                break;
            }
            let before = self.text.len();
            let empty = self.crossrefs(x, &places, &words_from, spec)?;
            size.parts.push(PartSize { kind: "crossrefs", id: x.clone(), tokens: estimate_tokens(&self.text[before..]), empty });
        }
        self.text.push_str("</passage>\n");
        size.tokens = estimate_tokens(&self.text[start..]);
        self.passages.push(size);
        Ok(())
    }

    /// Translation `t`'s text of `verses` (numbered as translation `from` numbers
    /// them). Returns whether it has none of them.
    fn bible(&mut self, t: &str, from: &str, verses: &[Ref], original: bool) -> Result<bool, String> {
        let info = self.lib.bible(t).ok_or_else(|| format!("no translation {:?}", t))?;
        let (name, abbr, year) = (info.name.clone(), info.abbr.clone(), info.year.clone());
        if !self.attached.translations.iter().any(|x| x == t) {
            self.attached.translations.push(t.to_string());
        }
        let (refs, missing) = self.in_translation(from, verses, t)?;
        let head = format!("<bible translation=\"{}\" abbr=\"{}\" year=\"{}\"", attr(&name), attr(&abbr), attr(&year));
        if refs.is_empty() {
            self.text.push_str(&format!("{}>\n(Not in this translation.)\n</bible>\n", head));
            return Ok(true);
        }
        let label = self.refs_label(t, &refs)?;
        self.text.push_str(&format!("{} ref=\"{}\">\n", head, attr(&label)));
        if original && t == "kjv" {
            self.attached.original_inline = true;
        }
        let mut chapter: Option<(String, u32)> = None;
        for r in &refs {
            if chapter.as_ref() != Some(&(r.0.clone(), r.1)) {
                self.text.push_str(&format!("## {}\n", chapter_heading(&r.0, r.1)));
                chapter = Some((r.0.clone(), r.1));
            }
            let words = self.verse_text(t, r)?;
            if r.2 == "0" {
                self.text.push_str("(title) ");
            } else {
                self.text.push_str(&r.2);
                self.text.push(' ');
            }
            self.text.push_str(&words);
            self.text.push('\n');
            if original && t == "kjv" {
                self.original_words(r, "   ");
            }
            if self.full() {
                break;
            }
        }
        if !missing.is_empty() && !self.capped {
            let from_label = self.refs_label(from, &missing)?;
            let abbr = self.lib.bible(from).map_or(from, |i| i.abbr.as_str()).to_string();
            self.text.push_str(&format!("(Not in this translation: {} in the {}.)\n", from_label, abbr));
        }
        self.text.push_str("</bible>\n");
        Ok(false)
    }

    /// Verse `r`'s words in translation `t`: the KJV's 66 books from the app's KJV, so
    /// they read exactly as its reader shows them.
    fn verse_text(&mut self, t: &str, r: &Ref) -> Result<String, String> {
        if t == "kjv"
            && let Some(text) = self.core_verse(r)
        {
            return Ok(text.to_string());
        }
        let b = self.book(t, &r.0)?.ok_or_else(|| format!("{} has no {}", t, r.0))?;
        Ok(b.at.get(&(r.1, r.2.clone())).map(|&i| b.verses[i].text.clone()).unwrap_or_default())
    }

    fn core_verse(&self, r: &Ref) -> Option<&'a str> {
        let data: &'a DataBundle = self.data;
        let name = books::by_code(&r.0)?.name;
        let book = data.bible.books.iter().find(|b| b.name == name)?;
        let ch = book.chapters.iter().find(|c| c.number == r.1)?;
        let n: u32 = r.2.parse().ok()?;
        ch.superscription.iter().chain(ch.verses.iter()).find(|v| v.verse_number == n).map(|v| v.text.as_str())
    }

    /// "Hebrew: בְּרֵאשִׁית H7225 in beginning | בָּרָא H1254 created | …" for KJV verse
    /// `r`. Returns whether it has any.
    fn original_words(&mut self, r: &Ref, indent: &str) -> bool {
        let data: &'a DataBundle = self.data;
        let Some(name) = books::by_code(&r.0).map(|b| b.name) else { return false };
        let Ok(n) = r.2.parse::<u32>() else { return false };
        let Some(iv) = data.extended.get_interlinear(name, r.1, n) else { return false };
        let language = match iv.language {
            OriginalLanguage::Hebrew => "Hebrew",
            OriginalLanguage::Aramaic => "Aramaic",
            OriginalLanguage::Greek => "Greek",
        };
        let words: Vec<String> = iv
            .original_words
            .iter()
            .map(|w| {
                let mut word = w.original_text.trim().to_string();
                if let Some(s) = &w.strongs_number {
                    word.push(' ');
                    word.push_str(&strongs_display(s));
                    if self.seen_strongs.insert(s.as_str()) {
                        self.strongs.push(s.as_str());
                    }
                }
                let gloss = format_gloss(&w.english_gloss);
                if !gloss.is_empty() {
                    word.push(' ');
                    word.push_str(&gloss);
                }
                word
            })
            .collect();
        if words.is_empty() {
            return false;
        }
        self.text.push_str(indent);
        self.text.push_str(language);
        self.text.push_str(": ");
        self.text.push_str(&words.join(" | "));
        self.text.push('\n');
        true
    }

    /// The original-language words of KJV verses `kjv`, where the KJV isn't among the
    /// translations given. Returns whether there were none.
    fn original_block(&mut self, kjv: &[Ref]) -> Result<bool, String> {
        let label = self.refs_label("kjv", kjv)?;
        self.attached.original_block = true;
        self.text.push_str(&format!("<original numbering=\"KJV\" ref=\"{}\">\n", attr(&label)));
        let mut chapter: Option<(String, u32)> = None;
        let mut any = false;
        for r in kjv {
            let at = self.text.len();
            if chapter.as_ref() != Some(&(r.0.clone(), r.1)) {
                self.text.push_str(&format!("## {}\n", chapter_heading(&r.0, r.1)));
            }
            if r.2 == "0" {
                self.text.push_str("(title) ");
            } else {
                self.text.push_str(&r.2);
                self.text.push(' ');
            }
            if self.original_words(r, "") {
                any = true;
                chapter = Some((r.0.clone(), r.1));
            } else {
                self.text.truncate(at);
            }
            if self.full() {
                break;
            }
        }
        if !any {
            self.text.push_str("(No original-language text for this passage.)\n");
        }
        self.text.push_str("</original>\n");
        Ok(!any)
    }

    /// KJV verses `kjv` as numbered places, and the chapters they cover whole.
    fn places(&mut self, kjv: &[Ref]) -> Result<Places, String> {
        let mut verses: Vec<(String, u32, u32)> = Vec::new();
        for r in kjv {
            if let Some((lo, hi)) = numbers(&r.2) {
                for v in lo..=hi.min(lo + 200) {
                    if v > 0 {
                        verses.push((r.0.clone(), r.1, v));
                    }
                }
            }
        }
        verses.dedup();
        let given: HashSet<&Ref> = kjv.iter().collect();
        let mut whole: HashSet<(String, u32)> = HashSet::new();
        let mut chapters: Vec<(String, u32)> = kjv.iter().map(|r| (r.0.clone(), r.1)).collect();
        chapters.dedup();
        for (code, c) in chapters {
            let Some(b) = self.book("kjv", &code)? else { continue };
            if b.verses.iter().filter(|v| v.chapter == c).all(|v| given.contains(&(code.clone(), c, v.number.clone()))) {
                whole.insert((code, c));
            }
        }
        Ok(Places { verses, whole })
    }

    /// Commentary `id`'s notes on `places`. Returns whether it has none.
    fn commentary(&mut self, id: &str, places: &Places, passage: &str) -> Result<bool, String> {
        let info = self.lib.commentaries().iter().find(|c| c.id == id).ok_or_else(|| format!("no commentary {:?}", id))?.clone();
        if !self.attached.commentaries.iter().any(|x| x == id) {
            self.attached.commentaries.push(id.to_string());
        }
        self.text.push_str(&format!(
            "<commentary name=\"{}\" author=\"{}\" year=\"{}\">\n",
            attr(&info.name),
            attr(&info.author),
            attr(&info.year)
        ));
        let mut codes: Vec<&str> = places.verses.iter().map(|p| p.0.as_str()).collect();
        codes.extend(places.whole.iter().map(|w| w.0.as_str()));
        let mut seen_codes = HashSet::new();
        codes.retain(|c| seen_codes.insert(*c));
        codes.sort_by_key(|c| books::order(c).unwrap_or(usize::MAX));
        let mut any = false;
        for code in codes {
            let notes = self.lib.commentary_book(id, code)?;
            let whole = |c: u32| places.whole.contains(&(code.to_string(), c));
            let mine: Vec<(u32, u32)> = places.verses.iter().filter(|p| p.0 == code).map(|p| (p.1, p.2)).collect();
            for n in notes.iter() {
                let wanted = if n.from.0 == 0 {
                    // The book's introduction, with its first chapter
                    whole(1)
                } else if n.from.1 == 0 && n.to == n.from {
                    // A chapter's introduction, with the whole chapter
                    whole(n.from.0)
                } else {
                    mine.iter().any(|&p| n.from <= p && p <= n.to)
                };
                if !wanted {
                    continue;
                }
                any = true;
                let on = note_label(display(code), code, n.from, n.to);
                let mut h = DefaultHasher::new();
                n.body.hash(&mut h);
                let key = (id.to_string(), code.to_string(), n.from, n.to, h.finish());
                if let Some(with) = self.notes_given.get(&key) {
                    self.text.push_str(&format!("<note on=\"{}\">(Given above, with {}.)</note>\n", attr(&on), with));
                    continue;
                }
                self.notes_given.insert(key, passage.to_string());
                self.text.push_str(&format!("<note on=\"{}\">\n", attr(&on)));
                self.text.push_str(&note_text(&n.body));
                self.text.push_str("\n</note>\n");
                if self.full() {
                    break;
                }
            }
            if self.capped {
                break;
            }
        }
        if !any {
            self.text.push_str("(No notes on this passage.)\n");
        }
        self.text.push_str("</commentary>\n");
        Ok(!any)
    }

    /// Collection `id`'s references from each verse of `places`, with their words in
    /// translation `bible` when the spec asks. Returns whether it has none.
    fn crossrefs(&mut self, id: &str, places: &Places, bible: &str, spec: &Spec) -> Result<bool, String> {
        let info = self.lib.crossrefs().iter().find(|c| c.id == id).ok_or_else(|| format!("no cross-references {:?}", id))?.clone();
        if !self.attached.crossrefs.iter().any(|x| x == id) {
            self.attached.crossrefs.push(id.to_string());
        }
        if spec.crossref_text {
            self.attached.crossref_text = true;
        }
        self.text.push_str(&format!("<crossrefs name=\"{}\" numbering=\"KJV\">\n", attr(&info.name)));
        let limit = if spec.crossref_limit == 0 { usize::MAX } else { spec.crossref_limit };
        let mut any = false;
        for (code, c, v) in &places.verses {
            let lines = self.lib.crossrefs_from(id, code, *c, *v)?;
            if lines.iter().all(|l| l.refs.is_empty()) {
                continue;
            }
            any = true;
            self.text.push_str(&format!("{}:{}\n", chapter_heading(code, *c), v));
            for line in lines {
                let take: Vec<_> = line.refs.iter().take(limit).collect();
                let keyword = line.text.trim();
                if spec.crossref_text {
                    if !keyword.is_empty() {
                        self.text.push_str(keyword);
                        self.text.push('\n');
                    }
                    for t in take {
                        let p = translations::passage(self.data, self.lib, bible, &t.to, t.votes)?;
                        let several = p.verses.len() > 1;
                        let words: Vec<String> =
                            p.verses.iter().map(|(n, text)| if several { format!("{} {}", n, text) } else { text.clone() }).collect();
                        let mut s = format!("- {}", p.label);
                        if p.from_kjv {
                            s.push_str(" (KJV)");
                        }
                        if !words.is_empty() {
                            s.push_str(": ");
                            s.push_str(&words.join(" "));
                        }
                        if p.more {
                            s.push_str(" …");
                        }
                        s.push('\n');
                        self.text.push_str(&s);
                    }
                } else {
                    let labels: Vec<String> = take
                        .iter()
                        .map(|t| translations::passage(self.data, self.lib, bible, &t.to, t.votes).map(|p| p.label))
                        .collect::<Result<_, _>>()?;
                    if keyword.is_empty() {
                        self.text.push_str(&format!("{}\n", labels.join("; ")));
                    } else {
                        self.text.push_str(&format!("{} {}\n", keyword, labels.join("; ")));
                    }
                }
            }
            if self.full() {
                break;
            }
        }
        if !any {
            self.text.push_str("(No cross-references from this passage.)\n");
        }
        self.text.push_str("</crossrefs>\n");
        Ok(!any)
    }

    /// "## H430 אֱלֹהִים · e.lo.him · H:N-M · God" and the entry, for each Strong's
    /// number in the context, once
    fn definitions(&mut self) {
        self.attached.definitions = true;
        self.text.push_str("<definitions>\n");
        let data: &'a DataBundle = self.data;
        for key in self.strongs.clone() {
            let Some(e) = data.extended.get_lexicon_entry(key) else {
                continue;
            };
            let head: Vec<&str> = [e.original_word.trim(), e.transliteration.trim(), e.morph.trim(), e.gloss.trim()]
                .into_iter()
                .filter(|x| !x.is_empty())
                .collect();
            self.text.push_str(&format!("## {} {}\n", strongs_display(key), head.join(" · ")));
            for line in e.definition.lines() {
                let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
                if !line.is_empty() {
                    self.text.push_str(&line);
                    self.text.push('\n');
                }
            }
            if self.full() {
                break;
            }
        }
        self.text.push_str("</definitions>\n");
    }
}

/// A passage's verses in the KJV's numbering: each numbered verse, and the chapters
/// it covers whole (whose introductions the commentaries' notes include).
struct Places {
    verses: Vec<(String, u32, u32)>,
    whole: HashSet<(String, u32)>,
}

// ---------------------------------------------------------------- instructions

/// How the assistant should behave, and what the context holds. Kept free of
/// anything that changes from turn to turn so providers can cache it with the
/// context that follows.
pub fn instructions(lib: &Library, built: &Built) -> String {
    let a = &built.attached;
    let mut s = String::from(
        "You are the study assistant in KJV Interlinear, a Bible study app with the King James Version \
         (1769 Oxford text) and its Hebrew, Aramaic, and Greek, many other English translations, \
         commentaries, and cross-references.\n\n",
    );
    if built.text.is_empty() {
        s.push_str(
            "No passage is attached to this conversation. Answer from your knowledge of the Bible, and \
             give references (Book chapter:verse) the reader can check.\n\n",
        );
    } else {
        s.push_str(&format!("The reader has attached {} below, inside <context>. Each <passage> holds:\n", built.label));
        let names: Vec<String> =
            a.translations.iter().filter_map(|id| lib.bible(id)).map(|b| format!("{} ({}, {})", b.name, b.abbr, b.year)).collect();
        s.push_str(&format!(
            "- Its text in {}, inside <bible>. Headings (##) mark chapters, each line starts with its verse \
             number, and \"(title)\" marks a psalm's title. Translations number some verses differently, \
             so each <bible> gives the passage as that translation numbers it.",
            list(&names)
        ));
        if a.original_inline {
            s.push_str(
                " Under each KJV verse are its Hebrew, Aramaic, or Greek words, each with its Strong's number \
                 and a short English gloss.",
            );
        }
        s.push('\n');
        if a.original_block {
            s.push_str(
                "- Inside <original>, its Hebrew, Aramaic, or Greek words verse by verse (in the KJV's \
                 numbering), each with its Strong's number and a short English gloss.\n",
            );
        }
        if !a.commentaries.is_empty() {
            let names: Vec<String> = a
                .commentaries
                .iter()
                .filter_map(|id| lib.commentaries().iter().find(|c| &c.id == id))
                .map(|c| format!("{} ({}, {}; {})", c.name, c.author, c.year, c.tradition))
                .collect();
            s.push_str(&format!(
                "- Notes on it from {}, inside <commentary>, each <note> saying which verses it is on (in the \
                 KJV's numbering). The notes are the published text, unabridged. When you use them, say whose \
                 view it is rather than presenting it as settled fact, and bear in mind when and where each \
                 was written.\n",
                list(&names)
            ));
        }
        if !a.crossrefs.is_empty() {
            let names: Vec<String> =
                a.crossrefs.iter().filter_map(|id| lib.crossrefs().iter().find(|c| &c.id == id)).map(|c| c.name.clone()).collect();
            s.push_str(&format!(
                "- Cross-references from {}, inside <crossrefs>: for each verse (in the KJV's numbering), \
                 places elsewhere in Scripture that bear on it{}.\n",
                list(&names),
                if a.crossref_text { ", with their words" } else { "" }
            ));
        }
        if a.definitions {
            s.push_str(
                "After the passages, inside <definitions>, is the full lexicon entry (from STEP Bible's Hebrew \
                 and Greek lexicons) for each Strong's number above, once each. Use them for a word's range of \
                 meaning, and say which sense you think fits a verse and why.\n",
            );
        }
        s.push_str(
            "\nWhen you quote Scripture, quote the attached text exactly and give the reference (Book \
             chapter:verse) and, where more than one translation is attached, the translation. If a \
             question needs passages that aren't attached, you may draw on your wider knowledge of the \
             Bible, but say which references you are citing from memory so the reader can check them.\n\n",
        );
    }
    s.push_str(
        "Guidelines:\n\
         - Distinguish what the text says from how it has been interpreted. Where Christian traditions \
         read a passage differently, say so briefly and fairly instead of presenting one view as the \
         only one.\n\
         - Be careful with the original languages: don't overstate what a word means, and say when a \
         point goes beyond the glosses and standard lexicons.\n\
         - If you are unsure of a fact, a date, or a reference, say so.\n\
         - Answer the question asked. Use short paragraphs, and lists or headings only when they help.",
    );
    s
}

/// "A", "A and B", "A, B, and C"
fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{} and {}", a, b),
        [rest @ .., last] => format!("{}, and {}", rest.join(", "), last),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_err_high_for_pointed_hebrew() {
        assert_eq!(estimate_tokens("In the beginning"), 5);
        // 11 code points of pointed Hebrew
        assert_eq!(estimate_tokens("בְּרֵאשִׁית"), 20);
    }

    #[test]
    fn verse_numbers() {
        assert_eq!(numbers("16"), Some((16, 16)));
        assert_eq!(numbers("1-2"), Some((1, 2)));
        assert_eq!(numbers("3a"), Some((3, 3)));
        assert_eq!(numbers("0"), Some((0, 0)));
        let r = reference::parse("Psalm 51:1-2").unwrap()[0];
        assert!(!in_range(&r, 51, "0"));
        assert!(in_range(&r, 51, "2-3"));
        let whole = reference::parse("Psalm 51").unwrap()[0];
        assert!(in_range(&whole, 51, "0"));
    }

    #[test]
    fn lists() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(list(&s(&["A"])), "A");
        assert_eq!(list(&s(&["A", "B"])), "A and B");
        assert_eq!(list(&s(&["A", "B", "C"])), "A, B, and C");
    }
}
