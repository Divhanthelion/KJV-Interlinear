//! The library's translations for the app's interface: the catalogue, and any
//! chapter of any translation with its heading and neighbours.

use kjv_library::books::{self, Section};
use kjv_library::view::ChapterView;
use kjv_library::{BibleInfo, Library};
use serde::Serialize;

use crate::api::ChapterRef;

#[derive(Debug, Serialize)]
pub struct BibleSummary {
    pub id: String,
    pub abbr: String,
    pub name: String,
    pub year: String,
    pub group: String,
    pub licence: String,
    pub credit: String,
    pub about: String,
    pub books: Vec<BibleBook>,
}

#[derive(Debug, Serialize)]
pub struct BibleBook {
    /// USFM code ("1SA"), as Scripture references in notes use
    pub code: String,
    /// The app's key ("First Samuel")
    pub name: String,
    /// "1 Samuel"
    pub display: String,
    pub abbr: String,
    /// "old", "apocrypha", or "new"
    pub section: &'static str,
    /// The translation's own name for the book ("Kings I")
    pub title: String,
    pub chapters: usize,
    /// Its chapter numbers (not always 1..=chapters)
    pub numbers: Vec<u32>,
}

fn section_key(s: Section) -> &'static str {
    match s {
        Section::Old => "old",
        Section::Apocrypha => "apocrypha",
        Section::New => "new",
    }
}

pub fn bibles(lib: &Library) -> Vec<BibleSummary> {
    lib.bibles()
        .iter()
        .map(|b| BibleSummary {
            id: b.id.clone(),
            abbr: b.abbr.clone(),
            name: b.name.clone(),
            year: b.year.clone(),
            group: b.group.clone(),
            licence: b.licence.clone(),
            credit: b.credit.clone(),
            about: b.about.clone(),
            books: b
                .books
                .iter()
                .filter_map(|e| {
                    let k = books::by_code(&e.code)?;
                    Some(BibleBook {
                        code: k.code.to_string(),
                        name: k.name.to_string(),
                        display: k.display.to_string(),
                        abbr: k.abbr.to_string(),
                        section: section_key(k.section),
                        title: e.name.clone(),
                        chapters: e.chapters,
                        numbers: e.numbers.clone(),
                    })
                })
                .collect(),
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct BibleChapter {
    pub bible: String,
    pub abbr: String,
    /// The app's book key ("Psalms")
    pub book: String,
    pub chapter: u32,
    /// "Psalm 23", "1 Samuel 3"
    pub heading: String,
    #[serde(flatten)]
    pub view: ChapterView,
    pub prev: Option<ChapterRef>,
    pub next: Option<ChapterRef>,
}

fn heading(display: &str, code: &str, chapter: u32) -> String {
    if code == "PSA" { format!("Psalm {}", chapter) } else { format!("{} {}", display, chapter) }
}

/// The chapter numbers book `code` of translation `info` has, in order.
fn chapter_numbers(lib: &Library, info: &BibleInfo, code: &str) -> Result<Vec<u32>, String> {
    Ok(lib.book(&info.id, code)?.chapters.iter().map(|c| c.number).collect())
}

/// Chapter `chapter` of `book` (the app's key, e.g. "First Samuel") in translation `bible`.
pub fn chapter(lib: &Library, bible: &str, book: &str, chapter: u32) -> Result<BibleChapter, String> {
    let info = lib.bible(bible).ok_or_else(|| format!("no translation {:?}", bible))?;
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let view = lib.chapter(bible, k.code, chapter)?;

    let position = info.books.iter().position(|b| b.code == k.code).ok_or_else(|| format!("{} has no {}", info.abbr, k.display))?;
    let numbers = chapter_numbers(lib, info, k.code)?;
    let at = numbers.iter().position(|&n| n == chapter).unwrap_or(0);
    let neighbour = |code: &str, number: u32| -> ChapterRef {
        ChapterRef { book: books::by_code(code).map_or(code, |b| b.name).to_string(), chapter: number }
    };
    let prev = if at > 0 {
        Some(neighbour(k.code, numbers[at - 1]))
    } else if position > 0 {
        let code = &info.books[position - 1].code;
        chapter_numbers(lib, info, code)?.last().map(|&n| neighbour(code, n))
    } else {
        None
    };
    let next = if at + 1 < numbers.len() {
        Some(neighbour(k.code, numbers[at + 1]))
    } else if let Some(b) = info.books.get(position + 1) {
        chapter_numbers(lib, info, &b.code)?.first().map(|&n| neighbour(&b.code, n))
    } else {
        None
    };

    Ok(BibleChapter {
        bible: info.id.clone(),
        abbr: info.abbr.clone(),
        book: k.name.to_string(),
        chapter,
        heading: heading(k.display, k.code, chapter),
        view,
        prev,
        next,
    })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Mapped {
    /// The app's book key
    pub book: String,
    pub chapter: u32,
    /// The verse as numbered there ("16", "1-2"; "0" a Psalm title)
    pub verse: String,
}

/// Where verse `verse` of `book` `chapter` in translation `from` is in translation
/// `to`: the first corresponding verse, or None if `to` has no counterpart. Verse 0
/// stands for the chapter (its first verse is mapped).
pub fn map(lib: &Library, from: &str, to: &str, book: &str, chapter: u32, verse: u32) -> Result<Option<Mapped>, String> {
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let number = if verse > 0 {
        verse.to_string()
    } else {
        // The chapter's first verse (or its title)
        let b = lib.book(from, k.code)?;
        let first = kjv_library::usfm::verses(&b).into_iter().find(|v| v.chapter == chapter);
        match first {
            Some(v) => v.number,
            None => return Ok(None),
        }
    };
    // A verse number may sit inside a bridged verse ("1-2")
    let r = {
        let exact = (k.code.to_string(), chapter, number.clone());
        if lib.has_verse(from, &exact) {
            exact
        } else {
            let b = lib.book(from, k.code)?;
            let n: u32 = number.parse().unwrap_or(0);
            kjv_library::usfm::verses(&b)
                .into_iter()
                .find(|v| {
                    v.chapter == chapter
                        && v.number.split_once('-').is_some_and(|(lo, hi)| {
                            lo.parse::<u32>().is_ok_and(|lo| lo <= n) && hi.parse::<u32>().is_ok_and(|hi| n <= hi)
                        })
                })
                .map(|v| (k.code.to_string(), chapter, v.number))
                .unwrap_or(exact)
        }
    };
    let found = lib.map(from, to, &r)?;
    Ok(found.into_iter().next().and_then(|(code, c, v)| {
        Some(Mapped { book: books::by_code(&code)?.name.to_string(), chapter: c, verse: v })
    }))
}

// ---------------------------------------------------------------- commentaries

#[derive(Debug, Serialize)]
pub struct CommentaryNotes {
    pub id: String,
    pub name: String,
    pub author: String,
    pub tradition: String,
    pub credit: String,
    pub notes: Vec<NoteView>,
}

#[derive(Debug, Serialize)]
pub struct NoteView {
    /// "John 3:14-16", "John 3 (introduction)", "John (introduction)"
    pub label: String,
    pub body: String,
}

fn note_label(display: &str, code: &str, from: (u32, u32), to: (u32, u32)) -> String {
    let head = |c: u32| if code == "PSA" { format!("Psalm {}", c) } else { format!("{} {}", display, c) };
    match (from, to) {
        ((0, _), _) => format!("{} (introduction)", display),
        ((c, 0), (c2, 0)) if c == c2 => format!("{} (introduction)", head(c)),
        ((c, v), (c2, v2)) if (c, v) == (c2, v2) => format!("{}:{}", head(c), v),
        ((c, v), (c2, v2)) if c == c2 => format!("{}:{}-{}", head(c), v, v2),
        ((c, v), (c2, v2)) => format!("{}:{}-{}:{}", head(c), v, c2, v2),
    }
}

/// The notes of commentaries `ids` on verse `verse` of `book` `chapter` as numbered in
/// translation `bible` (every commentary is keyed to the KJV, so the verse is mapped
/// to the KJV first). Verse 0: the chapter's introductions.
pub fn notes(lib: &Library, ids: &[String], bible: &str, book: &str, chapter: u32, verse: u32) -> Result<Vec<CommentaryNotes>, String> {
    let k = books::by_name(book).ok_or_else(|| format!("no book named {:?}", book))?;
    let places: Vec<(String, u32, u32)> = if verse == 0 {
        vec![(k.code.to_string(), chapter, 0)]
    } else {
        lib.map(bible, "kjv", &(k.code.to_string(), chapter, verse.to_string()))?
            .into_iter()
            .filter_map(|(code, c, v)| Some((code, c, v.split('-').next()?.parse().ok()?)))
            .collect()
    };
    let mut out = Vec::new();
    for id in ids {
        let info = lib.commentaries().iter().find(|c| &c.id == id).ok_or_else(|| format!("no commentary {:?}", id))?;
        let mut notes: Vec<NoteView> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (code, c, v) in &places {
            let display = books::by_code(code).map_or(code.as_str(), |b| b.display);
            for n in lib.notes_on(id, code, *c, *v)? {
                if seen.insert((code.clone(), n.from, n.to)) {
                    notes.push(NoteView { label: note_label(display, code, n.from, n.to), body: n.body });
                }
            }
        }
        out.push(CommentaryNotes {
            id: info.id.clone(),
            name: info.name.clone(),
            author: info.author.clone(),
            tradition: info.tradition.clone(),
            credit: info.credit.clone(),
            notes,
        });
    }
    Ok(out)
}
