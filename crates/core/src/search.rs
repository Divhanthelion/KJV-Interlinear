//! Searching any translation (the app's own KJV included) or any commentary, one
//! source at a time, so the app can show each source's results as they come.
//!
//! Each book is folded once (kjv_library::search keeps it while there's room) and
//! scanned for the folded query, so a match is exactly what the app's search has
//! always matched: case, curly quotes, dashes, and "æ" set aside, anywhere in a verse
//! (or note), across word boundaries. The library's word index rules out books that
//! can't hold the query, so a rare word reads only the books it is in.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use kjv_library::Library;
use kjv_library::books::{self, Section};
use kjv_library::search::Corpus;
use serde::{Deserialize, Serialize};

use crate::api::Scope;
use crate::bundle::DataBundle;
use crate::text::{Segment, find_folded_ranges, fold_for_search, segments};
use crate::translations::note_label;

/// Most results sent at once for one source; the total is always exact.
pub const LIMIT: usize = 500;

fn default_limit() -> usize {
    LIMIT
}

fn default_scope() -> Scope {
    Scope::All
}

/// What is searched: a translation or a commentary (their ids can be the same:
/// "tyndale" is William Tyndale's Bible and the Tyndale study notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Bible,
    Commentary,
}

fn default_kind() -> Kind {
    Kind::Bible
}

#[derive(Debug, Deserialize)]
pub struct SearchArgs {
    pub query: String,
    #[serde(default = "default_kind")]
    pub kind: Kind,
    /// The translation's or the commentary's id
    pub source: String,
    /// Which books: all, the Old Testament (with the Apocrypha), the New, or `book`
    #[serde(default = "default_scope")]
    pub scope: Scope,
    /// The app's key for the book, with scope "book"
    #[serde(default)]
    pub book: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct SourceResults {
    pub source: String,
    pub kind: Kind,
    pub query: String,
    /// Every verse (or note) that matches
    pub total: usize,
    /// The first `limit`, in the app's order of books
    pub hits: Vec<Hit>,
    /// Books read, and books the index ruled out
    pub read: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct Hit {
    /// Where it is: the app's book key, chapter, and verse (its first number; 0 for a
    /// Psalm title). A note is placed where it begins, in the KJV's numbering
    /// (chapter 0 for a book's introduction, verse 0 for a chapter's).
    pub book: String,
    pub chapter: u32,
    pub verse: u32,
    /// "John 3:16", "Psalm 51 (title)", "Luke 2:8-20", "Romans (introduction)"
    pub reference: String,
    /// The verse, or the words of the note around its first match, with matches marked
    pub segments: Vec<Segment>,
}

/// What a source is, and its books in the app's order.
enum Source {
    Bible(Vec<String>),
    Commentary(Vec<String>),
}

fn source(lib: &Library, kind: Kind, id: &str) -> Result<Source, String> {
    match kind {
        Kind::Bible => lib.bible(id).map(|b| Source::Bible(b.books.iter().map(|e| e.code.clone()).collect())).ok_or_else(|| format!("no translation {:?}", id)),
        Kind::Commentary => {
            lib.commentaries().iter().find(|c| c.id == id).map(|c| Source::Commentary(c.books.clone())).ok_or_else(|| format!("no commentary {:?}", id))
        }
    }
}

fn in_scope(code: &str, scope: Scope, book: Option<&str>) -> bool {
    let Some(b) = books::by_code(code) else { return false };
    match scope {
        Scope::All => true,
        Scope::Old => b.section != Section::New,
        Scope::New => b.section == Section::New,
        Scope::Book => book == Some(b.name),
    }
}

/// The app's KJV book for `code`, if it has it (the 66; the Apocrypha is the library's).
fn core_book<'a>(data: &'a DataBundle, code: &str) -> Option<&'a crate::models::Book> {
    let name = books::by_code(code)?.name;
    data.bible.books.iter().find(|b| b.name == name)
}

/// A KJV book's verses as the reader shows them: (chapter, verse, text), the Psalm
/// title (verse 0) first in its chapter.
fn core_verses(book: &crate::models::Book) -> Vec<(u32, u32, &str)> {
    book.chapters
        .iter()
        .flat_map(|ch| ch.superscription.iter().chain(ch.verses.iter()).map(move |v| (ch.number, v.verse_number, v.text.as_str())))
        .collect()
}

/// "Psalm 23:4", "Psalm 51 (title)", "John 3:16", "Psalm 9:1-2"
fn verse_label(code: &str, chapter: u32, verse: &str) -> String {
    let head = if code == "PSA" { format!("Psalm {}", chapter) } else { format!("{} {}", books::by_code(code).map_or(code, |b| b.display), chapter) };
    if verse == "0" { format!("{} (title)", head) } else { format!("{}:{}", head, verse) }
}

/// Up to this many characters of a note are shown around its first match.
const BEFORE: usize = 90;
const AFTER: usize = 210;

/// Where `folded` (a folded query) first occurs in `text`: its byte range there.
/// Folds only as far as the match, so a long note costs little when it matches early.
fn first_match(text: &str, folded: &str) -> Option<(usize, usize)> {
    if folded.is_empty() {
        return None;
    }
    let mut out = String::new();
    // For each byte of `out`, where its char starts in `text`
    let mut from: Vec<usize> = Vec::new();
    for (i, c) in text.char_indices() {
        let before = out.len();
        kjv_library::text::fold_char(c, &mut out);
        from.extend(std::iter::repeat_n(i, out.len() - before));
        if out.ends_with(folded) {
            let start = from[out.len() - folded.len()];
            return Some((start, i + c.len_utf8()));
        }
    }
    None
}

/// The words of a note around its first match of `query`, with every match in them marked.
fn snippet(text: &str, query: &str) -> Vec<Segment> {
    // One line, as the result list shows it
    let flat: String = text.lines().map(|l| l.trim_start_matches("### ").trim_start_matches("- ")).collect::<Vec<_>>().join(" ");
    let (start, end) = first_match(&flat, &fold_for_search(query)).unwrap_or((0, 0));
    // So many characters either side, widened to whole words
    let mut a = start;
    for (n, (i, c)) in flat[..start].char_indices().rev().enumerate() {
        a = i;
        if n >= BEFORE && c.is_whitespace() {
            a = i + c.len_utf8();
            break;
        }
    }
    if start == 0 {
        a = 0;
    }
    let mut b = flat.len();
    for (n, (i, c)) in flat[end..].char_indices().enumerate() {
        if n >= AFTER && c.is_whitespace() {
            b = end + i;
            break;
        }
    }
    let window = &flat[a..b];
    let mut out = segments(window.trim_end(), &[], &find_folded_ranges(window.trim_end(), query));
    if a > 0 {
        out.insert(0, Segment { text: "… ".into(), red: false, hit: false });
    }
    if b < flat.len() {
        out.push(Segment { text: " …".into(), red: false, hit: false });
    }
    out
}

/// Search source `args.source` for `args.query`.
pub fn search(data: &DataBundle, lib: &Library, args: &SearchArgs) -> Result<SourceResults, String> {
    let query = args.query.trim();
    let folded = fold_for_search(query);
    let src = source(lib, args.kind, &args.source)?;
    let (codes, prefix) = match &src {
        Source::Bible(c) => (c, "bible"),
        Source::Commentary(c) => (c, "comm"),
    };
    let mut out = SourceResults { source: args.source.clone(), kind: args.kind, query: query.to_string(), total: 0, hits: Vec::new(), read: 0, skipped: 0 };
    if folded.trim().is_empty() {
        return Ok(out);
    }
    let candidates = lib.search_index().and_then(|index| index.candidates(&folded).map(|set| (index, set)));
    let is_kjv = args.kind == Kind::Bible && args.source == "kjv";
    let mut wanted: Vec<&str> = Vec::new();
    for code in codes.iter().filter(|c| in_scope(c, args.scope, args.book.as_deref())) {
        // The app's KJV is its own text, not the index's: always read
        let own = is_kjv && core_book(data, code).is_some();
        if !own
            && let Some((index, set)) = &candidates
            && let Some(n) = index.chunk(&format!("{prefix}/{}/{code}", args.source))
            && !set.contains(n)
        {
            out.skipped += 1;
            continue;
        }
        wanted.push(code);
    }
    out.read = wanted.len();

    // Each book folded (once) and scanned, on every core
    let corpus = |code: &str| -> Result<Arc<Corpus>, String> {
        match &src {
            Source::Bible(_) if is_kjv && core_book(data, code).is_some() => {
                let book = core_book(data, code).unwrap();
                lib.corpus(&format!("app/kjv/{code}"), || Ok(Corpus::new(core_verses(book).into_iter().map(|(_, _, t)| t))))
            }
            Source::Bible(_) => lib.bible_corpus(&args.source, code),
            Source::Commentary(_) => lib.commentary_corpus(&args.source, code),
        }
    };
    let found: Vec<Result<Vec<usize>, String>> = parallel(wanted.len(), |i| corpus(wanted[i]).map(|c| c.find(&folded)));
    for (code, docs) in wanted.iter().zip(found) {
        let docs = docs?;
        out.total += docs.len();
        let room = args.limit.saturating_sub(out.hits.len());
        if room == 0 || docs.is_empty() {
            continue;
        }
        let name = books::by_code(code).map_or(*code, |b| b.name).to_string();
        match &src {
            Source::Bible(_) if is_kjv && core_book(data, code).is_some() => {
                let verses = core_verses(core_book(data, code).unwrap());
                for &d in docs.iter().take(room) {
                    let (c, v, text) = verses[d];
                    out.hits.push(Hit {
                        book: name.clone(),
                        chapter: c,
                        verse: v,
                        reference: verse_label(code, c, &v.to_string()),
                        segments: segments(text, &[], &find_folded_ranges(text, query)),
                    });
                }
            }
            Source::Bible(_) => {
                let verses = lib.verses(&args.source, code)?;
                for &d in docs.iter().take(room) {
                    let v = &verses[d];
                    out.hits.push(Hit {
                        book: name.clone(),
                        chapter: v.chapter,
                        verse: v.number.split('-').next().and_then(|n| n.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()).unwrap_or(0),
                        reference: verse_label(code, v.chapter, &v.number),
                        segments: segments(&v.text, &[], &find_folded_ranges(&v.text, query)),
                    });
                }
            }
            Source::Commentary(_) => {
                let notes = lib.commentary_book(&args.source, code)?;
                let display = books::by_code(code).map_or(*code, |b| b.display);
                for &d in docs.iter().take(room) {
                    let n = &notes[d];
                    out.hits.push(Hit {
                        book: name.clone(),
                        chapter: n.from.0,
                        verse: n.from.1,
                        reference: note_label(display, code, n.from, n.to),
                        segments: snippet(&kjv_library::notes::text(&n.body), query),
                    });
                }
            }
        }
    }
    Ok(out)
}

/// `f(0)` … `f(n - 1)` on every core, in order.
fn parallel<T: Send>(n: usize, f: impl Fn(usize) -> T + Sync) -> Vec<T> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(n.max(1));
    if threads <= 1 {
        return (0..n).map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let done: Mutex<Vec<(usize, T)>> = Mutex::new(Vec::with_capacity(n));
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    let out = f(i);
                    done.lock().unwrap().push((i, out));
                }
            });
        }
    });
    let mut out = done.into_inner().unwrap();
    out.sort_by_key(|(i, _)| *i);
    out.into_iter().map(|(_, x)| x).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &[Segment]) -> String {
        s.iter().map(|x| if x.hit { format!("[{}]", x.text) } else { x.text.clone() }).collect()
    }

    #[test]
    fn snippets_show_the_words_around_the_first_match() {
        let note = format!("### Heading\n{} Melchizedek king of Salem {}", "word ".repeat(40), "more ".repeat(80));
        let s = text(&snippet(&note, "melchizedek"));
        assert!(s.starts_with("… "), "{}", s);
        assert!(s.contains("[Melchizedek] king of Salem"), "{}", s);
        assert!(s.ends_with(" …"), "{}", s);
        let short = text(&snippet("### The Case\nOf Abraham.", "abraham"));
        assert_eq!(short, "The Case Of [Abraham].");
    }

    #[test]
    fn first_matches_are_found_in_the_original_text() {
        let t = "Render unto Cæsar the things which are Cæsar’s";
        let (a, b) = first_match(t, &fold_for_search("caesar's")).unwrap();
        assert_eq!(&t[a..b], "Cæsar’s");
        let (a, b) = first_match(t, "caesar").unwrap();
        assert_eq!(&t[a..b], "Cæsar");
        assert_eq!(first_match(t, "zebra"), None);
    }

    #[test]
    fn labels() {
        assert_eq!(verse_label("PSA", 51, "0"), "Psalm 51 (title)");
        assert_eq!(verse_label("JHN", 3, "16"), "John 3:16");
        assert_eq!(verse_label("1SA", 1, "1-2"), "1 Samuel 1:1-2");
    }
}
